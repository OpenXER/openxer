//! Non-Windows platform calls for `openxer-chat`: random bytes, SHA-256 and
//! RS256 signature checks through OpenSSL (already linked by `native-tls`),
//! and private per-user credential files (`0700` directory, `0600` files)
//! under the OpenXER XDG data directory.

use openssl::bn::BigNum;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rand::rand_bytes;
use openssl::rsa::{Padding, Rsa};
use openssl::sign::Verifier;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Fill `buf` from the system cryptographic RNG via OpenSSL.
pub fn random_bytes(buf: &mut [u8]) {
    rand_bytes(buf).expect("the system random number generator failed");
}

/// SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    openssl::sha::sha256(data)
}

/// Check an RSASSA-PKCS1-v1_5 SHA-256 signature (JWS "RS256") of `message`
/// with the public key given by its big-endian modulus `n` and exponent `e`.
pub fn rs256_verify(n: &[u8], e: &[u8], message: &[u8], signature: &[u8]) -> bool {
    let n = strip_zeros(n);
    let e = strip_zeros(e);
    if n.is_empty() || e.is_empty() || n.len() > 1024 || e.len() > 8 {
        return false;
    }
    let Ok(bn_n) = BigNum::from_slice(n) else {
        return false;
    };
    let Ok(bn_e) = BigNum::from_slice(e) else {
        return false;
    };
    let Ok(rsa) = Rsa::from_public_components(bn_n, bn_e) else {
        return false;
    };
    let Ok(pkey) = PKey::from_rsa(rsa) else {
        return false;
    };
    let Ok(mut verifier) = Verifier::new(MessageDigest::sha256(), &pkey) else {
        return false;
    };
    if verifier.set_rsa_padding(Padding::PKCS1).is_err() {
        return false;
    }
    verifier.verify_oneshot(signature, message).unwrap_or(false)
}

fn strip_zeros(b: &[u8]) -> &[u8] {
    let first = b.iter().position(|&x| x != 0).unwrap_or(b.len());
    &b[first..]
}

/// Serializes credential writes and deletes inside this process.
static CRED_WRITE_LOCK: Mutex<()> = Mutex::new(());
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn write_lock() -> std::sync::MutexGuard<'static, ()> {
    CRED_WRITE_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

fn cred_dir() -> PathBuf {
    let base = std::env::var_os("OPENXER_DATA_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_DATA_HOME")
                .filter(|v| !v.is_empty())
                .map(|v| PathBuf::from(v).join("openxer"))
        })
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|v| PathBuf::from(v).join(".local/share/openxer"))
        })
        .unwrap_or_else(|| std::env::temp_dir().join("openxer"));
    base.join("credentials")
}

fn cred_path(target: &str) -> PathBuf {
    let digest = sha256(target.as_bytes());
    let mut name = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut name, "{b:02x}");
    }
    cred_dir().join(name)
}

fn io_err(e: io::Error) -> u32 {
    e.raw_os_error()
        .and_then(|c| u32::try_from(c).ok())
        .unwrap_or(1)
}

/// Largest blob one stored credential slot holds (matches the Windows limit
/// so `store.rs` splits values the same way on every platform).
pub const CRED_MAX_BLOB: usize = 5 * 512;

/// Read the credential blob for `target`, or `Ok(None)` when absent.
pub fn cred_read(target: &str) -> Result<Option<Vec<u8>>, u32> {
    match fs::read(cred_path(target)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(e)),
    }
}

/// Atomically write the credential blob for `target` with `0600` permissions
/// inside a `0700` directory.
pub fn cred_write(target: &str, blob: &[u8]) -> Result<(), u32> {
    if blob.len() > CRED_MAX_BLOB {
        return Err(22);
    }
    let _lock = write_lock();
    let dir = cred_dir();
    if !dir.exists() {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .map_err(io_err)?;
    }
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    let dest = cred_path(target);
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = dest.with_extension(format!("tmp.{}.{seq}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(io_err)?;
    if let Err(e) = file.write_all(blob).and_then(|()| file.flush()) {
        let _ = fs::remove_file(&tmp);
        return Err(io_err(e));
    }
    drop(file);
    if let Err(e) = fs::rename(&tmp, &dest) {
        let _ = fs::remove_file(&tmp);
        return Err(io_err(e));
    }
    Ok(())
}

/// Delete the credential blob for `target`; missing entries are not an error.
pub fn cred_delete(target: &str) -> Result<(), u32> {
    let _lock = write_lock();
    match fs::remove_file(cred_path(target)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(e)),
    }
}

/// On Unix, proxy configuration comes from environment variables, which
/// `http.rs` checks directly.
pub fn system_proxy_configured() -> bool {
    false
}

/// A throwaway RSA key for tests: signs with RS256 and gives its public
/// modulus and exponent.
#[cfg(any(test, feature = "testkit"))]
pub struct TestRsaKey {
    key: PKey<openssl::pkey::Private>,
    pub n: Vec<u8>,
    pub e: Vec<u8>,
}

#[cfg(any(test, feature = "testkit"))]
impl TestRsaKey {
    #[cfg(test)]
    pub fn generate() -> TestRsaKey {
        TestRsaKey::generate_bits(2048)
    }

    pub fn generate_bits(bits: u32) -> TestRsaKey {
        let rsa = Rsa::generate(bits).expect("generate test RSA key");
        let n = rsa.n().to_vec();
        let e = rsa.e().to_vec();
        let key = PKey::from_rsa(rsa).expect("wrap test RSA key");
        TestRsaKey { key, n, e }
    }

    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        use openssl::sign::Signer;
        let mut signer = Signer::new(MessageDigest::sha256(), &self.key).expect("RS256 signer");
        signer
            .set_rsa_padding(Padding::PKCS1)
            .expect("PKCS1 padding");
        signer.sign_oneshot_to_vec(message).expect("RS256 sign")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_standard_vector() {
        let d = sha256(b"abc");
        assert_eq!(
            d[..4],
            [0xba, 0x78, 0x16, 0xbf],
            "SHA-256(abc) starts ba7816bf"
        );
    }

    #[test]
    fn rs256_round_trip_and_tamper() {
        let key = TestRsaKey::generate();
        let sig = key.sign(b"header.payload");
        assert!(rs256_verify(&key.n, &key.e, b"header.payload", &sig));
        assert!(!rs256_verify(&key.n, &key.e, b"header.payloaD", &sig));
        let mut bad = sig.clone();
        bad[10] ^= 1;
        assert!(!rs256_verify(&key.n, &key.e, b"header.payload", &bad));
        assert!(!rs256_verify(&[], &key.e, b"x", &sig));
    }

    #[test]
    fn random_bytes_differ() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        random_bytes(&mut a);
        random_bytes(&mut b);
        assert_ne!(a, b);
    }
}
