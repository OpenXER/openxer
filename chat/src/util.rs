//! Small shared pieces: secrets that never print, cancellation, base64url
//! and form encoding.

use std::fmt;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A secret string (a token). It has no `Display`, its `Debug` prints
/// `<redacted>`, and its memory is overwritten when it is dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Secret {
        Secret(value.into())
    }
    /// The secret value. Call only to put it on the wire or into storage.
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        // SAFETY: zero bytes are valid UTF-8, and the string is dropped
        // right after.
        let bytes = unsafe { self.0.as_bytes_mut() };
        for b in bytes.iter_mut() {
            // A volatile write so the compiler keeps it.
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

/// Cancels a sign-in, a request or a chat turn from another thread. Cancel
/// also shuts down the socket the work is blocked on, so it stops at once.
#[derive(Clone, Default)]
pub struct CancelToken {
    inner: Arc<CancelInner>,
}

#[derive(Default)]
struct CancelInner {
    cancelled: AtomicBool,
    next_id: std::sync::atomic::AtomicU64,
    sockets: Mutex<Vec<(u64, TcpStream)>>,
}

/// Keeps a socket registered with a [`CancelToken`]; dropping it removes
/// the registration (and the handle copy) on every exit path.
pub(crate) struct WatchGuard {
    token: CancelToken,
    id: u64,
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        self.token
            .inner
            .sockets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|(id, _)| *id != self.id);
    }
}

impl fmt::Debug for WatchGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WatchGuard({})", self.id)
    }
}

impl fmt::Debug for CancelToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CancelToken({})", self.is_cancelled())
    }
}

impl CancelToken {
    pub fn new() -> CancelToken {
        CancelToken::default()
    }
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        let sockets = std::mem::take(&mut *self.inner.sockets.lock().unwrap());
        for (_, s) in sockets {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }
    /// Register a socket to shut down on cancel (a clone of the one in
    /// use) until the returned guard is dropped.
    pub(crate) fn watch(&self, socket: &TcpStream) -> WatchGuard {
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        if let Ok(clone) = socket.try_clone() {
            if self.is_cancelled() {
                let _ = clone.shutdown(std::net::Shutdown::Both);
            } else {
                self.inner.sockets.lock().unwrap().push((id, clone));
            }
        }
        WatchGuard {
            token: self.clone(),
            id,
        }
    }
    /// How many sockets are registered now (for tests).
    #[cfg(test)]
    pub(crate) fn watched(&self) -> usize {
        self.inner.sockets.lock().unwrap().len()
    }
    /// Sleep up to `d`, waking early on cancel. True when cancelled.
    pub(crate) fn sleep(&self, d: Duration) -> bool {
        let end = Instant::now() + d;
        while Instant::now() < end {
            if self.is_cancelled() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10).min(end - Instant::now()));
        }
        self.is_cancelled()
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Unpadded base64url (RFC 4648 section 5).
pub fn b64url(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

/// Decode base64url, padded or not. Also accepts the standard alphabet.
pub fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => continue,
            _ => return None,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// `n` random bytes as base64url.
pub fn random_token(n: usize) -> String {
    let mut b = vec![0u8; n];
    crate::win32::random_bytes(&mut b);
    b64url(&b)
}

/// A random version 4 UUID, as `urn:uuid:...`.
pub fn random_urn_uuid() -> String {
    let mut b = [0u8; 16];
    crate::win32::random_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// Percent-encode for a query string or form body (unreserved kept).
pub fn form_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Join pairs as `a=b&c=d`, each part encoded.
pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", form_encode(k), form_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Decode a query or form value (`+` is a space).
pub fn form_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse `a=b&c=d` into decoded pairs.
pub fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (form_decode(k), form_decode(v)),
            None => (form_decode(p), String::new()),
        })
        .collect()
}

/// Keep only characters that are safe to show from a server-supplied error
/// code: letters, digits, `_`, `.`, `-`, at most 80 of them.
pub fn safe_code(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        .take(80)
        .collect()
}

/// Cut `s` to at most `max` bytes at a character boundary.
pub fn cut(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_round_trips_and_matches_rfc_vectors() {
        assert_eq!(b64url(b""), "");
        assert_eq!(b64url(b"f"), "Zg");
        assert_eq!(b64url(b"fo"), "Zm8");
        assert_eq!(b64url(b"foo"), "Zm9v");
        assert_eq!(b64url(b"foob"), "Zm9vYg");
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
        for n in 0..40u8 {
            let data: Vec<u8> = (0..n).map(|i| i.wrapping_mul(37)).collect();
            assert_eq!(b64url_decode(&b64url(&data)).unwrap(), data);
        }
        assert_eq!(b64url_decode("Zm9vYg==").unwrap(), b"foob");
        assert!(b64url_decode("Zm9v!").is_none());
    }

    #[test]
    fn pkce_challenge_matches_rfc7636_appendix_b() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = b64url(&crate::win32::sha256(verifier.as_bytes()));
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn form_encoding_round_trips() {
        let s = "a b&c=d/é:~";
        assert_eq!(form_decode(&form_encode(s)), s);
        assert_eq!(form_decode("a+b%2"), "a b%2");
        let q = parse_query("code=x%2By&state=s+t&flag");
        assert_eq!(q[0], ("code".into(), "x+y".into()));
        assert_eq!(q[1], ("state".into(), "s t".into()));
        assert_eq!(q[2], ("flag".into(), String::new()));
    }

    #[test]
    fn secret_debug_is_redacted() {
        let s = Secret::new("tok-123");
        assert_eq!(format!("{s:?}"), "<redacted>");
        assert_eq!(s.expose(), "tok-123");
    }

    #[test]
    fn urn_uuid_shape() {
        let u = random_urn_uuid();
        assert!(u.starts_with("urn:uuid:"));
        assert_eq!(u.len(), 9 + 36);
        assert_eq!(&u[9 + 14..9 + 15], "4");
    }

    #[test]
    fn cut_respects_char_boundaries() {
        assert_eq!(cut("héllo", 2), "h");
        assert_eq!(cut("abc", 10), "abc");
    }
}
