//! The few Windows calls this crate needs: random bytes, SHA-256 and RS256
//! signature checks through CNG (bcrypt.dll), and generic credentials in
//! Credential Manager (advapi32.dll). Everything unsafe in the crate is here.

use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
use windows_sys::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
};
use windows_sys::Win32::Security::Cryptography::{
    BCryptDestroyKey, BCryptGenRandom, BCryptHash, BCryptImportKeyPair, BCryptVerifySignature,
    BCRYPT_KEY_HANDLE, BCRYPT_PAD_PKCS1, BCRYPT_PKCS1_PADDING_INFO, BCRYPT_RSAKEY_BLOB,
    BCRYPT_RSAPUBLIC_BLOB, BCRYPT_RSAPUBLIC_MAGIC, BCRYPT_RSA_ALG_HANDLE, BCRYPT_SHA256_ALGORITHM,
    BCRYPT_SHA256_ALG_HANDLE, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
};

/// Fill `buf` from the system random number generator.
pub fn random_bytes(buf: &mut [u8]) {
    // SAFETY: the pointer and length describe `buf`; the system RNG flag
    // needs no algorithm handle.
    let status = unsafe {
        BCryptGenRandom(
            null_mut(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    assert_eq!(status, 0, "the system random number generator failed");
}

/// SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    // SAFETY: the SHA-256 pseudo handle needs no setup; the pointers and
    // lengths describe `data` and `out`.
    let status = unsafe {
        BCryptHash(
            BCRYPT_SHA256_ALG_HANDLE,
            null(),
            0,
            data.as_ptr(),
            data.len() as u32,
            out.as_mut_ptr(),
            32,
        )
    };
    assert_eq!(status, 0, "SHA-256 failed");
    out
}

/// Check an RSASSA-PKCS1-v1_5 SHA-256 signature (JWS "RS256") of `message`
/// with the public key given by its big-endian modulus `n` and exponent `e`.
pub fn rs256_verify(n: &[u8], e: &[u8], message: &[u8], signature: &[u8]) -> bool {
    let n = strip_zeros(n);
    let e = strip_zeros(e);
    if n.is_empty() || e.is_empty() || n.len() > 1024 || e.len() > 8 {
        return false;
    }
    let header = BCRYPT_RSAKEY_BLOB {
        Magic: BCRYPT_RSAPUBLIC_MAGIC,
        BitLength: (n.len() * 8) as u32,
        cbPublicExp: e.len() as u32,
        cbModulus: n.len() as u32,
        cbPrime1: 0,
        cbPrime2: 0,
    };
    let mut blob =
        Vec::with_capacity(std::mem::size_of::<BCRYPT_RSAKEY_BLOB>() + e.len() + n.len());
    // SAFETY: BCRYPT_RSAKEY_BLOB is a plain struct of six u32 values.
    blob.extend_from_slice(unsafe {
        std::slice::from_raw_parts(
            (&header as *const BCRYPT_RSAKEY_BLOB).cast::<u8>(),
            std::mem::size_of::<BCRYPT_RSAKEY_BLOB>(),
        )
    });
    blob.extend_from_slice(e);
    blob.extend_from_slice(n);
    let mut key: BCRYPT_KEY_HANDLE = null_mut();
    // SAFETY: the blob is a public key blob laid out as CNG documents it.
    let status = unsafe {
        BCryptImportKeyPair(
            BCRYPT_RSA_ALG_HANDLE,
            null_mut(),
            BCRYPT_RSAPUBLIC_BLOB,
            &mut key,
            blob.as_ptr(),
            blob.len() as u32,
            0,
        )
    };
    if status != 0 {
        return false;
    }
    let digest = sha256(message);
    let padding = BCRYPT_PKCS1_PADDING_INFO {
        pszAlgId: BCRYPT_SHA256_ALGORITHM,
    };
    // SAFETY: `key` was imported above and is destroyed right after.
    let status = unsafe {
        let s = BCryptVerifySignature(
            key,
            (&padding as *const BCRYPT_PKCS1_PADDING_INFO).cast(),
            digest.as_ptr(),
            32,
            signature.as_ptr(),
            signature.len() as u32,
            BCRYPT_PAD_PKCS1,
        );
        BCryptDestroyKey(key);
        s
    };
    status == 0
}

fn strip_zeros(b: &[u8]) -> &[u8] {
    let first = b.iter().position(|&x| x != 0).unwrap_or(b.len());
    &b[first..]
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Credential Manager writes and deletes from several threads of one
/// process at once were seen to lose updates: a credential just written
/// read back as missing, and deletes that reported success left the entry
/// in place. Every write and delete in this process takes this lock.
static CRED_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn write_lock() -> std::sync::MutexGuard<'static, ()> {
    CRED_WRITE_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

/// Read the blob of the generic credential `target`. `Ok(None)` when there
/// is none. Errors carry only the Windows error number.
pub fn cred_read(target: &str) -> Result<Option<Vec<u8>>, u32> {
    let name = wide(target);
    let mut cred: *mut CREDENTIALW = null_mut();
    // SAFETY: `name` is NUL terminated; on success `cred` points at a
    // record Windows allocated, which is copied and freed here.
    unsafe {
        if CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) == 0 {
            let err = GetLastError();
            return if err == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err(err)
            };
        }
        let c = &*cred;
        let bytes = if c.CredentialBlobSize == 0 || c.CredentialBlob.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()
        };
        CredFree(cred as *const _);
        Ok(Some(bytes))
    }
}

/// The largest blob one generic credential holds (CRED_MAX_CREDENTIAL_BLOB_SIZE).
pub const CRED_MAX_BLOB: usize = 5 * 512;

/// Write (create or replace) the generic credential `target`, kept for this
/// Windows user on this computer only (not roaming).
pub fn cred_write(target: &str, blob: &[u8]) -> Result<(), u32> {
    if blob.len() > CRED_MAX_BLOB {
        // ERROR_INVALID_PARAMETER; callers split large values first.
        return Err(87);
    }
    let mut name = wide(target);
    let mut user = wide("OpenXER");
    let cred = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: name.as_mut_ptr(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_ptr() as *mut u8,
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: user.as_mut_ptr(),
        ..Default::default()
    };
    let _lock = write_lock();
    // SAFETY: every pointer in `cred` lives until the call returns.
    if unsafe { CredWriteW(&cred, 0) } == 0 {
        // SAFETY: plain thread error read.
        return Err(unsafe { GetLastError() });
    }
    Ok(())
}

/// Delete the generic credential `target`; deleting one that is not there
/// is not an error.
pub fn cred_delete(target: &str) -> Result<(), u32> {
    let name = wide(target);
    let _lock = write_lock();
    // SAFETY: `name` is NUL terminated.
    unsafe {
        if CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) == 0 {
            let err = GetLastError();
            if err != ERROR_NOT_FOUND {
                return Err(err);
            }
        }
    }
    Ok(())
}
/// Whether Windows has a proxy set up for this user: a named proxy or a
/// proxy script in the Internet Options settings, or a WinHTTP proxy
/// (`netsh winhttp set proxy`). "Automatically detect settings" alone is
/// not counted: it is on by default and usually finds nothing.
pub fn system_proxy_configured() -> bool {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::Networking::WinHttp::{
        WinHttpGetDefaultProxyConfiguration, WinHttpGetIEProxyConfigForCurrentUser,
        WINHTTP_ACCESS_TYPE_NAMED_PROXY, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WINHTTP_PROXY_INFO,
    };
    fn free(p: *mut u16) -> bool {
        if p.is_null() {
            return false;
        }
        // SAFETY: WinHTTP allocates these strings with GlobalAlloc.
        unsafe {
            GlobalFree(p.cast());
        }
        true
    }
    let mut found = false;
    let mut ie = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG {
        fAutoDetect: 0,
        lpszAutoConfigUrl: null_mut(),
        lpszProxy: null_mut(),
        lpszProxyBypass: null_mut(),
    };
    // SAFETY: the struct is written by the call; its strings are freed below.
    if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut ie) } != 0 {
        found |= free(ie.lpszProxy);
        found |= free(ie.lpszAutoConfigUrl);
        free(ie.lpszProxyBypass);
    }
    let mut info = WINHTTP_PROXY_INFO::default();
    // SAFETY: as above.
    if unsafe { WinHttpGetDefaultProxyConfiguration(&mut info) } != 0 {
        let named = info.dwAccessType == WINHTTP_ACCESS_TYPE_NAMED_PROXY;
        let has = free(info.lpszProxy);
        free(info.lpszProxyBypass);
        found |= named && has;
    }
    found
}

/// A throwaway RSA key for tests: signs with RS256 and gives its public
/// modulus and exponent.
#[cfg(any(test, feature = "testkit"))]
pub struct TestRsaKey {
    key: BCRYPT_KEY_HANDLE,
    pub n: Vec<u8>,
    pub e: Vec<u8>,
}

// SAFETY: CNG key handles may be used from any thread.
#[cfg(any(test, feature = "testkit"))]
unsafe impl Send for TestRsaKey {}
#[cfg(any(test, feature = "testkit"))]
unsafe impl Sync for TestRsaKey {}

#[cfg(any(test, feature = "testkit"))]
impl TestRsaKey {
    #[cfg(test)]
    pub fn generate() -> TestRsaKey {
        TestRsaKey::generate_bits(2048)
    }

    pub fn generate_bits(bits: u32) -> TestRsaKey {
        use windows_sys::Win32::Security::Cryptography::{
            BCryptExportKey, BCryptFinalizeKeyPair, BCryptGenerateKeyPair,
        };
        let mut key: BCRYPT_KEY_HANDLE = null_mut();
        // SAFETY: standard CNG key generation and export into a sized buffer.
        unsafe {
            assert_eq!(
                BCryptGenerateKeyPair(BCRYPT_RSA_ALG_HANDLE, &mut key, bits, 0),
                0
            );
            assert_eq!(BCryptFinalizeKeyPair(key, 0), 0);
            let mut len = 0u32;
            assert_eq!(
                BCryptExportKey(
                    key,
                    null_mut(),
                    BCRYPT_RSAPUBLIC_BLOB,
                    null_mut(),
                    0,
                    &mut len,
                    0
                ),
                0
            );
            let mut buf = vec![0u8; len as usize];
            assert_eq!(
                BCryptExportKey(
                    key,
                    null_mut(),
                    BCRYPT_RSAPUBLIC_BLOB,
                    buf.as_mut_ptr(),
                    len,
                    &mut len,
                    0
                ),
                0
            );
            let head: BCRYPT_RSAKEY_BLOB = std::ptr::read_unaligned(buf.as_ptr().cast());
            let at = std::mem::size_of::<BCRYPT_RSAKEY_BLOB>();
            let e = buf[at..at + head.cbPublicExp as usize].to_vec();
            let at = at + head.cbPublicExp as usize;
            let n = buf[at..at + head.cbModulus as usize].to_vec();
            TestRsaKey { key, n, e }
        }
    }

    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        use windows_sys::Win32::Security::Cryptography::BCryptSignHash;
        let digest = sha256(message);
        let padding = BCRYPT_PKCS1_PADDING_INFO {
            pszAlgId: BCRYPT_SHA256_ALGORITHM,
        };
        let mut out = vec![0u8; self.n.len()];
        let mut len = 0u32;
        // SAFETY: the key is live; the buffer is the modulus size.
        unsafe {
            assert_eq!(
                BCryptSignHash(
                    self.key,
                    (&padding as *const BCRYPT_PKCS1_PADDING_INFO).cast(),
                    digest.as_ptr(),
                    32,
                    out.as_mut_ptr(),
                    out.len() as u32,
                    &mut len,
                    BCRYPT_PAD_PKCS1
                ),
                0
            );
        }
        out.truncate(len as usize);
        out
    }
}

#[cfg(any(test, feature = "testkit"))]
impl Drop for TestRsaKey {
    fn drop(&mut self) {
        // SAFETY: the key was generated by this value and is destroyed once.
        unsafe {
            BCryptDestroyKey(self.key);
        }
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
