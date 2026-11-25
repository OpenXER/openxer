//! A small blocking HTTP/1.1 client: one request per connection
//! (`Connection: close`), TLS through Schannel (`native-tls`), plain HTTP for
//! the loopback test servers. Bodies are read incrementally, so a stream of
//! server-sent events is handed on as it arrives. Cancel shuts the socket
//! down, which ends a blocked read at once.
//!
//! It does not follow redirects, use a proxy or decompress: the endpoints it
//! talks to need none of that.

use crate::util::{CancelToken, WatchGuard};
use std::fmt;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// A parsed `http://` or `https://` URL.
#[derive(Clone, PartialEq, Eq)]
pub struct Url {
    pub secure: bool,
    pub host: String,
    pub port: u16,
    /// Path and query, starting with `/`.
    pub path: String,
}

impl fmt::Debug for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The query is left out: an authorize URL can carry an ID token hint.
        let path = self.path.split('?').next().unwrap_or("");
        write!(
            f,
            "{}://{}:{}{}",
            if self.secure { "https" } else { "http" },
            self.host,
            self.port,
            path
        )
    }
}

impl Url {
    pub fn parse(s: &str) -> Result<Url, String> {
        let (secure, rest) = if let Some(r) = s.strip_prefix("https://") {
            (true, r)
        } else if let Some(r) = s.strip_prefix("http://") {
            (false, r)
        } else {
            return Err("the URL must start with http:// or https://".into());
        };
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let default_port = if secure { 443 } else { 80 };
        let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
            // An IPv6 literal: `[::1]` or `[::1]:8080`.
            let (h, after) = rest
                .split_once(']')
                .ok_or_else(|| "bad IPv6 address in URL".to_string())?;
            let port = match after.strip_prefix(':') {
                Some(p) => p.parse().map_err(|_| "bad port in URL".to_string())?,
                None if after.is_empty() => default_port,
                None => return Err("bad IPv6 address in URL".into()),
            };
            (h, port)
        } else {
            match authority.rsplit_once(':') {
                Some((h, p)) if !h.is_empty() && !p.is_empty() => {
                    (h, p.parse().map_err(|_| "bad port in URL".to_string())?)
                }
                _ => (authority, default_port),
            }
        };
        if host.is_empty() {
            return Err("the URL has no host".into());
        }
        Ok(Url {
            secure,
            host: host.to_string(),
            port,
            path: path.to_string(),
        })
    }

    /// This URL with `path` appended to its path (one slash between).
    pub fn join(&self, path: &str) -> Url {
        let mut u = self.clone();
        let base = u.path.trim_end_matches('/');
        u.path = format!("{}/{}", base, path.trim_start_matches('/'));
        u
    }

    fn host_header(&self) -> String {
        let default = if self.secure { 443 } else { 80 };
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == default {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }
}

/// Why a request failed. Messages hold no request or response content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpError {
    Cancelled,
    Timeout,
    /// Could not connect (no network, refused, DNS).
    Connect(String),
    Tls(String),
    /// The connection broke during the exchange.
    Io(String),
    /// The server's reply was not valid HTTP.
    Malformed(String),
    TooLarge,
    /// The connection failed and a proxy is set up on this computer; this
    /// client does not go through proxies yet.
    Proxy,
    /// The request itself was invalid (a line break in a header).
    InvalidRequest(String),
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::Cancelled => f.write_str("cancelled"),
            HttpError::Timeout => f.write_str("the server did not answer in time"),
            HttpError::Connect(e) => write!(f, "could not connect: {e}"),
            HttpError::Tls(e) => write!(f, "secure connection failed: {e}"),
            HttpError::Io(e) => write!(f, "the connection broke: {e}"),
            HttpError::Malformed(e) => write!(f, "the server sent an invalid reply: {e}"),
            HttpError::TooLarge => f.write_str("the server reply was too large"),
            HttpError::Proxy => f.write_str(
                "could not connect, and a proxy server is set up on this computer; \
                 this connection does not support proxies yet",
            ),
            HttpError::InvalidRequest(e) => write!(f, "invalid request: {e}"),
        }
    }
}

/// Timeouts for one request.
#[derive(Clone, Copy, Debug)]
pub struct HttpConfig {
    pub connect_timeout: Duration,
    /// Longest wait for any bytes once connected (a stream may pause while
    /// the model thinks).
    pub read_timeout: Duration,
}

impl Default for HttpConfig {
    fn default() -> Self {
        HttpConfig {
            connect_timeout: Duration::from_secs(8),
            read_timeout: Duration::from_secs(180),
        }
    }
}

/// How often a blocked read wakes to look at the cancel token. On Windows
/// a socket shutdown from another thread does not reliably end a blocked
/// receive, so reads use a short socket timeout and loop.
const POLL: Duration = Duration::from_millis(50);

/// A TCP stream whose reads block until data, the idle limit, or cancel.
/// TLS runs on top of it, so it never sees a timeout mid-record.
#[derive(Debug)]
struct Socket {
    inner: TcpStream,
    cancel: CancelToken,
    idle_limit: Duration,
}

impl Read for Socket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut idle = Duration::ZERO;
        loop {
            if self.cancel.is_cancelled() {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "cancelled",
                ));
            }
            match self.inner.read(buf) {
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    idle += POLL;
                    if idle >= self.idle_limit {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "read timed out"));
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                r => return r,
            }
        }
    }
}

impl Write for Socket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "cancelled",
            ));
        }
        self.inner.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

enum Conn {
    Plain(Socket),
    Tls(Box<native_tls::TlsStream<Socket>>),
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Conn::Plain(s) => s.read(buf),
            Conn::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Conn::Plain(s) => s.write(buf),
            Conn::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Conn::Plain(s) => s.flush(),
            Conn::Tls(s) => s.flush(),
        }
    }
}

enum Framing {
    Length(u64),
    Chunked { left: u64, done: bool },
    Close,
}

/// A response whose body is read as it arrives (`Read`).
pub struct Response {
    pub status: u16,
    headers: Vec<(String, String)>,
    reader: BufReader<Conn>,
    framing: Framing,
    cancel: CancelToken,
    _watch: WatchGuard,
}

/// The longest status, header or chunk-size line accepted.
const MAX_LINE: usize = 8 * 1024;

impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Response({})", self.status)
    }
}

impl Response {
    /// The first header named `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The whole body, at most `limit` bytes.
    pub fn read_all(mut self, limit: usize) -> Result<Vec<u8>, HttpError> {
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = self.read(&mut buf).map_err(|e| self.map_err(e))?;
            if n == 0 {
                return Ok(out);
            }
            if out.len() + n > limit {
                return Err(HttpError::TooLarge);
            }
            out.extend_from_slice(&buf[..n]);
        }
    }

    /// Read some body bytes, mapping errors to [`HttpError`]. `Ok(0)` at the end.
    pub fn read_some(&mut self, buf: &mut [u8]) -> Result<usize, HttpError> {
        self.read(buf).map_err(|e| self.map_err(e))
    }

    fn map_err(&self, e: io::Error) -> HttpError {
        io_error(e, &self.cancel)
    }

    fn read_line(&mut self) -> io::Result<String> {
        let mut line = Vec::new();
        let n = (&mut self.reader)
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed",
            ));
        }
        if !line.ends_with(b"\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                if line.len() > MAX_LINE {
                    "a header line is too long"
                } else {
                    "the reply ended inside a header line"
                },
            ));
        }
        Ok(String::from_utf8_lossy(&line).trim_end().to_string())
    }
}

impl Read for Response {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        match self.framing {
            Framing::Length(0) => Ok(0),
            Framing::Length(left) => {
                let want = (left as usize).min(buf.len());
                let n = self.reader.read(&mut buf[..want])?;
                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "body cut short",
                    ));
                }
                self.framing = Framing::Length(left - n as u64);
                Ok(n)
            }
            Framing::Close => self.reader.read(buf),
            Framing::Chunked { done: true, .. } => Ok(0),
            Framing::Chunked { left: 0, .. } => {
                let line = self.read_line()?;
                let size = line.split(';').next().unwrap_or("").trim();
                let size = u64::from_str_radix(size, 16)
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad chunk size"))?;
                if size == 0 {
                    // Trailers end at an empty line.
                    loop {
                        if self.read_line()?.is_empty() {
                            break;
                        }
                    }
                    self.framing = Framing::Chunked {
                        left: 0,
                        done: true,
                    };
                    return Ok(0);
                }
                self.framing = Framing::Chunked {
                    left: size,
                    done: false,
                };
                self.read(buf)
            }
            Framing::Chunked { left, .. } => {
                let want = (left as usize).min(buf.len());
                let n = self.reader.read(&mut buf[..want])?;
                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "chunk cut short",
                    ));
                }
                let left = left - n as u64;
                if left == 0 {
                    let end = self.read_line()?;
                    if !end.is_empty() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "missing chunk end",
                        ));
                    }
                }
                self.framing = Framing::Chunked { left, done: false };
                Ok(n)
            }
        }
    }
}

fn io_error(e: io::Error, cancel: &CancelToken) -> HttpError {
    if cancel.is_cancelled() {
        return HttpError::Cancelled;
    }
    match e.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => HttpError::Timeout,
        io::ErrorKind::InvalidData => HttpError::Malformed(e.to_string()),
        _ => HttpError::Io(e.kind().to_string()),
    }
}

/// Whether `url`'s host can be reached: a TCP connect and, for `https`, the
/// TLS handshake, within the configured connect and read timeouts. No
/// request is sent, so nothing is used up on the other side.
pub fn reach(url: &Url, cfg: &HttpConfig, cancel: &CancelToken) -> Result<(), HttpError> {
    let stream = match connect(url, cfg.connect_timeout, cancel) {
        Ok(s) => s,
        Err(HttpError::Connect(_) | HttpError::Timeout) if proxy_configured() => {
            return Err(HttpError::Proxy)
        }
        Err(e) => return Err(e),
    };
    if !url.secure {
        return Ok(());
    }
    let _watch = cancel.watch(&stream);
    stream
        .set_read_timeout(Some(cfg.read_timeout))
        .map_err(|e| io_error(e, cancel))?;
    let connector = native_tls::TlsConnector::new().map_err(|e| HttpError::Tls(e.to_string()))?;
    match connector.connect(&url.host, stream) {
        Ok(_) => Ok(()),
        Err(_) if cancel.is_cancelled() => Err(HttpError::Cancelled),
        Err(native_tls::HandshakeError::Failure(e)) => Err(HttpError::Tls(e.to_string())),
        Err(native_tls::HandshakeError::WouldBlock(_)) => Err(HttpError::Timeout),
    }
}

/// Send one request and read the status line and headers. The body is read
/// from the returned [`Response`].
pub fn send(
    method: &str,
    url: &Url,
    headers: &[(&str, &str)],
    body: &[u8],
    cfg: &HttpConfig,
    cancel: &CancelToken,
) -> Result<Response, HttpError> {
    if cancel.is_cancelled() {
        return Err(HttpError::Cancelled);
    }
    let line_break = |s: &str| s.contains('\r') || s.contains('\n');
    if line_break(method) || line_break(&url.path) || line_break(&url.host) {
        return Err(HttpError::InvalidRequest(
            "a line break in the request line".into(),
        ));
    }
    if headers
        .iter()
        .any(|(k, v)| line_break(k) || line_break(v) || k.contains(':'))
    {
        return Err(HttpError::InvalidRequest("a line break in a header".into()));
    }
    let stream = match connect(url, cfg.connect_timeout, cancel) {
        Ok(s) => s,
        Err(HttpError::Connect(_) | HttpError::Timeout) if proxy_configured() => {
            return Err(HttpError::Proxy)
        }
        Err(e) => return Err(e),
    };
    let watch = cancel.watch(&stream);
    if cancel.is_cancelled() {
        return Err(HttpError::Cancelled);
    }
    stream
        .set_read_timeout(Some(POLL))
        .map_err(|e| io_error(e, cancel))?;
    stream
        .set_write_timeout(Some(cfg.read_timeout))
        .map_err(|e| io_error(e, cancel))?;
    let _ = stream.set_nodelay(true);
    let stream = Socket {
        inner: stream,
        cancel: cancel.clone(),
        idle_limit: cfg.read_timeout,
    };
    let mut conn = if url.secure {
        let connector =
            native_tls::TlsConnector::new().map_err(|e| HttpError::Tls(e.to_string()))?;
        match connector.connect(&url.host, stream) {
            Ok(s) => Conn::Tls(Box::new(s)),
            Err(_) if cancel.is_cancelled() => return Err(HttpError::Cancelled),
            Err(native_tls::HandshakeError::Failure(e)) => {
                return Err(HttpError::Tls(e.to_string()))
            }
            Err(native_tls::HandshakeError::WouldBlock(_)) => return Err(HttpError::Timeout),
        }
    } else {
        Conn::Plain(stream)
    };
    let mut head = format!(
        "{method} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept-Encoding: identity\r\nUser-Agent: OpenXER/{}\r\n",
        url.path,
        url.host_header(),
        env!("CARGO_PKG_VERSION")
    );
    for (k, v) in headers {
        head.push_str(k);
        head.push_str(": ");
        head.push_str(v);
        head.push_str("\r\n");
    }
    if !body.is_empty() || method == "POST" {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    let mut wire = head.into_bytes();
    wire.extend_from_slice(body);
    conn.write_all(&wire).map_err(|e| io_error(e, cancel))?;
    conn.flush().map_err(|e| io_error(e, cancel))?;
    // Overwrite the copy that held the authorization header and body.
    for b in wire.iter_mut() {
        *b = 0;
    }

    let mut resp = Response {
        status: 0,
        headers: Vec::new(),
        reader: BufReader::with_capacity(16 * 1024, conn),
        framing: Framing::Close,
        cancel: cancel.clone(),
        _watch: watch,
    };
    // Interim 1xx replies (100 Continue, 103 Early Hints) come before the
    // real one; skip them, a few at most.
    for interim in 0.. {
        if interim > 8 {
            return Err(HttpError::Malformed("too many interim replies".into()));
        }
        let status_line = resp.read_line().map_err(|e| io_error(e, cancel))?;
        let mut parts = status_line.splitn(3, ' ');
        let version = parts.next().unwrap_or("");
        if !version.starts_with("HTTP/1.") {
            return Err(HttpError::Malformed("no HTTP status line".into()));
        }
        resp.status = parts
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| HttpError::Malformed("bad status code".into()))?;
        resp.headers.clear();
        let mut total = 0;
        loop {
            let line = resp.read_line().map_err(|e| io_error(e, cancel))?;
            if line.is_empty() {
                break;
            }
            total += line.len();
            if total > 64 * 1024 || resp.headers.len() > 200 {
                return Err(HttpError::TooLarge);
            }
            if let Some((k, v)) = line.split_once(':') {
                resp.headers
                    .push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        if !(100..200).contains(&resp.status) || resp.status == 101 {
            break;
        }
    }
    let chunked = resp
        .header("transfer-encoding")
        .map(|v| v.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);
    resp.framing = if chunked {
        Framing::Chunked {
            left: 0,
            done: false,
        }
    } else if let Some(n) = resp.header("content-length").and_then(|v| v.parse().ok()) {
        Framing::Length(n)
    } else if resp.status == 204 || resp.status == 304 || method == "HEAD" {
        Framing::Length(0)
    } else {
        Framing::Close
    };
    Ok(resp)
}

#[cfg(test)]
thread_local! {
    /// Tests set this to pretend a proxy is (or is not) set up.
    pub(crate) static FAKE_PROXY: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Whether a proxy is set up: `HTTPS_PROXY` or `ALL_PROXY` in the
/// environment, or the Windows proxy settings.
pub fn proxy_configured() -> bool {
    #[cfg(test)]
    if let Some(v) = FAKE_PROXY.with(|f| f.get()) {
        return v;
    }
    #[cfg(feature = "testkit")]
    match crate::testkit::FAKE_PROXY_ALL.load(std::sync::atomic::Ordering::SeqCst) {
        1 => return false,
        2 => return true,
        _ => {}
    }
    let env = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]
        .iter()
        .any(|k| std::env::var(k).is_ok_and(|v| !v.trim().is_empty()));
    env || crate::win32::system_proxy_configured()
}

/// Look up the host and connect, within `limit` in total, on a helper
/// thread so cancel is honored at once (a name lookup cannot be
/// interrupted). A connection made after cancel is dropped by the helper.
fn connect(url: &Url, limit: Duration, cancel: &CancelToken) -> Result<TcpStream, HttpError> {
    let (tx, rx) = std::sync::mpsc::channel();
    let (host, port) = (url.host.clone(), url.port);
    let deadline = std::time::Instant::now() + limit;
    std::thread::spawn(move || {
        let result = (|| {
            let addrs: Vec<_> = (host.as_str(), port)
                .to_socket_addrs()
                .map_err(|e| HttpError::Connect(format!("name lookup failed ({})", e.kind())))?
                .collect();
            let mut last = HttpError::Connect("no address for the host".into());
            for a in addrs {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    return Err(HttpError::Timeout);
                }
                match TcpStream::connect_timeout(&a, left) {
                    Ok(s) => return Ok(s),
                    Err(e) if e.kind() == io::ErrorKind::TimedOut => last = HttpError::Timeout,
                    Err(e) => last = HttpError::Connect(e.kind().to_string()),
                }
            }
            Err(last)
        })();
        let _ = tx.send(result);
    });
    loop {
        if cancel.is_cancelled() {
            return Err(HttpError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(r) => return r,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if std::time::Instant::now() >= deadline + Duration::from_millis(500) {
                    return Err(HttpError::Timeout);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(HttpError::Connect("the connection attempt failed".into()))
            }
        }
    }
}

/// POST a form body and read the whole reply (status, body).
pub fn post_form(
    url: &Url,
    form: &str,
    cfg: &HttpConfig,
    cancel: &CancelToken,
) -> Result<(u16, Vec<u8>), HttpError> {
    let resp = send(
        "POST",
        url,
        &[
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Accept", "application/json"),
        ],
        form.as_bytes(),
        cfg,
        cancel,
    )?;
    let status = resp.status;
    let body = resp.read_all(1 << 20)?;
    Ok((status, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::time::Instant;

    fn serve_once(reply: Vec<u8>) -> (Url, std::thread::JoinHandle<Vec<u8>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = s.read(&mut buf).unwrap();
                req.extend_from_slice(&buf[..n]);
                if n == 0 || req.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            s.write_all(&reply).unwrap();
            req
        });
        (
            Url::parse(&format!("http://127.0.0.1:{port}/v1")).unwrap(),
            h,
        )
    }

    #[test]
    fn url_parsing() {
        let u = Url::parse("https://api.example.test/v1").unwrap();
        assert!(u.secure);
        assert_eq!(
            (u.host.as_str(), u.port, u.path.as_str()),
            ("api.example.test", 443, "/v1")
        );
        assert_eq!(u.join("models").path, "/v1/models");
        assert_eq!(u.join("/models").path, "/v1/models");
        let u = Url::parse("http://127.0.0.1:9/").unwrap();
        assert_eq!(u.port, 9);
        assert!(Url::parse("ftp://x").is_err());
        assert!(Url::parse("http://").is_err());
        let u = Url::parse("https://a.test/authorize?id_token_hint=secret").unwrap();
        assert!(!format!("{u:?}").contains("secret"));
    }

    #[test]
    fn reads_content_length_and_chunked_bodies() {
        let (url, h) = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello".to_vec());
        let r = send(
            "GET",
            &url,
            &[("X-Test", "1")],
            b"",
            &HttpConfig::default(),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.read_all(100).unwrap(), b"hello");
        let req = String::from_utf8(h.join().unwrap()).unwrap();
        assert!(req.starts_with("GET /v1 HTTP/1.1\r\n"));
        assert!(req.contains("X-Test: 1\r\n"));

        let (url, _h) = serve_once(
            b"HTTP/1.1 201 Created\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5;x=y\r\npedia\r\n0\r\nTrailer: 1\r\n\r\n".to_vec(),
        );
        let r = send(
            "GET",
            &url,
            &[],
            b"",
            &HttpConfig::default(),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(r.status, 201);
        assert_eq!(r.read_all(100).unwrap(), b"Wikipedia");

        let (url, _h) = serve_once(b"HTTP/1.1 200 OK\r\n\r\nuntil close".to_vec());
        let r = send(
            "GET",
            &url,
            &[],
            b"",
            &HttpConfig::default(),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(r.read_all(100).unwrap(), b"until close");
    }

    #[test]
    fn body_limit_and_cut_short() {
        let (url, _h) = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\nshort".to_vec());
        let r = send(
            "GET",
            &url,
            &[],
            b"",
            &HttpConfig::default(),
            &CancelToken::new(),
        )
        .unwrap();
        assert!(matches!(r.read_all(100), Err(HttpError::Io(_))));
        let (url, _h) =
            serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n0123456789".to_vec());
        let r = send(
            "GET",
            &url,
            &[],
            b"",
            &HttpConfig::default(),
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(r.read_all(4), Err(HttpError::TooLarge));
        let (url, _h) = serve_once(b"garbage\r\n\r\n".to_vec());
        assert!(matches!(
            send(
                "GET",
                &url,
                &[],
                b"",
                &HttpConfig::default(),
                &CancelToken::new()
            ),
            Err(HttpError::Malformed(_))
        ));
    }

    #[test]
    fn connection_refused_is_a_connect_error() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        let url = Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap();
        // Windows retries a refused loopback connect for about two seconds.
        let cfg = HttpConfig {
            connect_timeout: Duration::from_secs(6),
            ..Default::default()
        };
        FAKE_PROXY.with(|f| f.set(Some(false)));
        let r = send("GET", &url, &[], b"", &cfg, &CancelToken::new());
        FAKE_PROXY.with(|f| f.set(None));
        assert!(matches!(r, Err(HttpError::Connect(_))), "{r:?}");
    }

    #[test]
    fn cancel_ends_a_blocked_read_promptly() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let _server = std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            std::thread::sleep(Duration::from_secs(5));
            drop(s);
        });
        let url = Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap();
        let cancel = CancelToken::new();
        let c2 = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            c2.cancel();
        });
        let t = Instant::now();
        let r = send("GET", &url, &[], b"", &HttpConfig::default(), &cancel);
        assert_eq!(r.unwrap_err(), HttpError::Cancelled);
        assert!(t.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn read_timeout_is_reported() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let _server = std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            std::thread::sleep(Duration::from_secs(2));
            drop(s);
        });
        let url = Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap();
        let cfg = HttpConfig {
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_millis(200),
        };
        assert_eq!(
            send("GET", &url, &[], b"", &cfg, &CancelToken::new()).unwrap_err(),
            HttpError::Timeout
        );
    }

    fn get(url: &Url, cancel: &CancelToken) -> Result<Response, HttpError> {
        send("GET", url, &[], b"", &HttpConfig::default(), cancel)
    }

    #[test]
    fn interim_replies_are_skipped() {
        let (url, _h) = serve_once(
            b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 103 Early Hints\r\nLink: </x>\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok"
                .to_vec(),
        );
        let r = get(&url, &CancelToken::new()).unwrap();
        assert_eq!(r.status, 200);
        assert!(r.header("link").is_none(), "interim headers are dropped");
        assert_eq!(r.read_all(10).unwrap(), b"ok");
    }

    #[test]
    fn an_oversized_header_line_is_rejected_not_split() {
        let mut reply = b"HTTP/1.1 200 OK\r\nX-Big: ".to_vec();
        reply.extend(std::iter::repeat_n(b'a', MAX_LINE + 10));
        reply.extend_from_slice(b"\r\nContent-Length: 2\r\n\r\nok");
        let (url, _h) = serve_once(reply);
        assert!(matches!(
            get(&url, &CancelToken::new()),
            Err(HttpError::Malformed(m)) if m.contains("too long")
        ));
        // A reply that ends inside a header line.
        let (url, _h) = serve_once(b"HTTP/1.1 200 OK\r\nX-Half: abc".to_vec());
        assert!(matches!(
            get(&url, &CancelToken::new()),
            Err(HttpError::Malformed(_))
        ));
    }

    #[test]
    fn line_breaks_in_request_headers_are_refused_before_connecting() {
        let url = Url::parse("http://127.0.0.1:9/").unwrap();
        for h in [("X-A", "v\r\nX-Injected: 1"), ("X-A\n", "v"), ("X:A", "v")] {
            assert!(matches!(
                send(
                    "GET",
                    &url,
                    &[h],
                    b"",
                    &HttpConfig::default(),
                    &CancelToken::new()
                ),
                Err(HttpError::InvalidRequest(_))
            ));
        }
        let mut bad = url.clone();
        bad.path = "/x HTTP/1.1\r\nX: y".into();
        assert!(matches!(
            get(&bad, &CancelToken::new()),
            Err(HttpError::InvalidRequest(_))
        ));
    }

    #[test]
    fn sockets_are_deregistered_on_every_exit_path() {
        let cancel = CancelToken::new();
        // A whole reply read.
        let (url, _h) = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec());
        let (status, _) = post_form(&url, "a=b", &HttpConfig::default(), &cancel).unwrap();
        assert_eq!(status, 200);
        assert_eq!(cancel.watched(), 0);
        // A non-200 reply whose body is never read.
        let (url, _h) = serve_once(b"HTTP/1.1 500 X\r\nContent-Length: 3\r\n\r\nbad".to_vec());
        let r = get(&url, &cancel).unwrap();
        assert_eq!(cancel.watched(), 1, "registered while the reply is open");
        drop(r);
        assert_eq!(cancel.watched(), 0);
        // A malformed reply.
        let (url, _h) = serve_once(b"nonsense\r\n\r\n".to_vec());
        assert!(get(&url, &cancel).is_err());
        assert_eq!(cancel.watched(), 0);
        // Many requests do not pile up.
        for _ in 0..20 {
            let (url, _h) = serve_once(b"HTTP/1.1 204 No Content\r\n\r\n".to_vec());
            get(&url, &cancel).unwrap().read_all(10).unwrap();
        }
        assert_eq!(cancel.watched(), 0);
    }

    #[test]
    fn a_failed_connection_behind_a_proxy_says_so() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        let url = Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap();
        FAKE_PROXY.with(|f| f.set(Some(true)));
        let e = get(&url, &CancelToken::new()).unwrap_err();
        FAKE_PROXY.with(|f| f.set(Some(false)));
        assert_eq!(e, HttpError::Proxy);
        assert!(e.to_string().contains("proxy"));
        assert!(matches!(
            get(&url, &CancelToken::new()),
            Err(HttpError::Connect(_))
        ));
        FAKE_PROXY.with(|f| f.set(None));
    }

    #[test]
    fn cancel_before_the_connection_is_made_returns_at_once() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let t = Instant::now();
        let url = Url::parse("http://127.0.0.1:9/").unwrap();
        assert_eq!(get(&url, &cancel).unwrap_err(), HttpError::Cancelled);
        assert!(t.elapsed() < Duration::from_millis(200));
    }

    #[test]
    fn error_text_carries_no_request_content() {
        let secret = "Bearer AT-SECRET-http";
        let replies: Vec<Vec<u8>> = vec![
            b"garbage\r\n\r\n".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 99\r\n\r\ncut".to_vec(),
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n".to_vec(),
            Vec::new(),
        ];
        for reply in replies {
            let (url, _h) = serve_once(reply);
            let r = send(
                "POST",
                &url,
                &[("Authorization", secret)],
                b"refresh_token=RT-SECRET-http",
                &HttpConfig::default(),
                &CancelToken::new(),
            );
            let text = match r {
                Ok(r) => match r.read_all(1000) {
                    Ok(_) => continue,
                    Err(e) => format!("{e:?} {e}"),
                },
                Err(e) => format!("{e:?} {e}"),
            };
            assert!(!text.contains("SECRET"), "{text}");
        }
    }
}
