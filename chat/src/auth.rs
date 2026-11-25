//! Sign in with ChatGPT for a locally run app: the authorization code flow
//! with PKCE (S256) through the user's browser and a one-shot loopback
//! listener on 127.0.0.1, dynamic client registration on the first sign-in,
//! token refresh (rotating refresh tokens, one refresh at a time), sign-out
//! with revocation, and a status query.
//!
//! The browser is opened by the caller: [`Auth::sign_in`] takes a function
//! that is handed the authorization URL. This crate never opens one.

use crate::http::{self, HttpConfig, HttpError, Url};
use crate::jwt::{Expected, IdTokenVerifier, JwksVerifier};
use crate::store::{CredentialStore, Item};
use crate::util::{self, form, CancelToken, Secret};
use serde_json::{json, Value};
use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The client ID sent on the first sign-in, before OpenAI issues one.
pub const DYNAMIC_CLIENT_ID: &str = "dynamic_agent_client";
/// The scopes for ChatGPT plan usage.
pub const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
/// The scope that allows inference on the user's plan.
pub const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
/// The name shown to the user on the first sign-in (`agent_name_hint`).
pub const AGENT_NAME: &str = "OpenXER";
/// How long a refresh token lasts (30 days, renewed by each refresh).
pub const REFRESH_LIFETIME: u64 = 30 * 24 * 3600;
/// Refresh errors after which the stored tokens are useless.
const TERMINAL_REFRESH_CODES: &[&str] = &[
    "invalid_grant",
    "invalid_refresh_token",
    "token_expired",
    "refresh_token_expired",
    "refresh_token_invalidated",
    "refresh_token_reused",
];

/// Endpoints and settings for sign-in. [`AuthConfig::openai`] is the real
/// service; tests point every URL at a local fake.
#[derive(Clone, Debug)]
pub struct AuthConfig {
    /// The `iss` the ID token must carry.
    pub issuer: String,
    pub authorize_url: Url,
    pub token_url: Url,
    pub revoke_url: Url,
    pub jwks_url: Url,
    /// The API the tokens are for (`resource`).
    pub resource: String,
    pub scope: String,
    pub agent_name: String,
    /// The loopback callback path; only the port may change between sign-ins.
    pub callback_path: String,
    /// The callback port tried first; when it is taken any free port is used.
    pub preferred_port: u16,
    /// How long to wait for the browser to come back.
    pub callback_timeout: Duration,
    /// Refresh when the access token has less than this left.
    pub refresh_margin: Duration,
    pub http: HttpConfig,
    /// Waits between revocation attempts on sign-out.
    pub revoke_retry_delays: Vec<Duration>,
}

impl AuthConfig {
    /// OpenAI's production endpoints, from
    /// `https://auth.openai.com/.well-known/openid-configuration`.
    pub fn openai() -> AuthConfig {
        let u = |s: &str| Url::parse(s).expect("built-in URL");
        AuthConfig {
            issuer: "https://auth.openai.com".into(),
            authorize_url: u("https://auth.openai.com/api/accounts/authorize"),
            token_url: u("https://auth.openai.com/api/accounts/oauth/token"),
            revoke_url: u("https://auth.openai.com/api/accounts/oauth/revoke"),
            jwks_url: u("https://auth.openai.com/.well-known/jwks.json"),
            resource: "https://api.openai.com/v1".into(),
            scope: SCOPES.into(),
            agent_name: AGENT_NAME.into(),
            callback_path: "/auth/callback".into(),
            preferred_port: 1455,
            callback_timeout: Duration::from_secs(300),
            refresh_margin: Duration::from_secs(300),
            http: HttpConfig {
                connect_timeout: Duration::from_secs(15),
                read_timeout: Duration::from_secs(30),
            },
            revoke_retry_delays: vec![Duration::from_millis(500), Duration::from_secs(2)],
        }
    }
}

/// The signed-in account, from the validated ID token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub subject: String,
    pub email: Option<String>,
    pub name: Option<String>,
}

impl Account {
    /// The email, else the name, else "your ChatGPT account".
    pub fn label(&self) -> String {
        self.email
            .clone()
            .or_else(|| self.name.clone())
            .unwrap_or_else(|| "your ChatGPT account".into())
    }
}

/// Where the sign-in stands. Reading it makes no network request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignInStatus {
    SignedOut,
    /// Signed in. `plan_usage` is false when the user signed in but did not
    /// allow OpenXER to use their ChatGPT plan; chat is then refused.
    SignedIn {
        account: Account,
        plan_usage: bool,
    },
    /// The stored sign-in can no longer be renewed (its refresh token is
    /// past its 30 days); the user must sign in again.
    Expired {
        account: Account,
    },
}

/// Why sign-in or a token request failed. Messages never hold tokens,
/// codes, or server bodies; server error codes are reduced to safe text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthError {
    Cancelled,
    /// The browser did not come back in time. (A callback with the wrong
    /// `state` is refused and waiting goes on, so it ends here too.)
    TimedOut,
    /// The user declined in the browser.
    ConsentDenied,
    /// The authorization server returned another error.
    Authorization {
        code: String,
    },
    /// The callback named a client ID other than the saved one.
    ClientMismatch,
    /// The first sign-in's callback carried no issued client ID.
    MissingClientId,
    /// The code exchange failed.
    Exchange {
        status: u16,
        code: String,
    },
    /// The ID token failed a check.
    IdToken(String),
    /// The account that signed in is not the one stored for this client.
    IdentityMismatch,
    /// The loopback listener could not start.
    Listener(String),
    /// The caller's open-browser function failed.
    Browser(String),
    /// No usable sign-in (never signed in, signed out, or the refresh token
    /// was rejected and the tokens were cleared).
    SignedOut,
    Network(String),
    /// The connection failed and a proxy server is set up on this computer;
    /// sign-in does not support proxies yet.
    ProxyNotSupported,
    /// The authorization server failed (5xx); credentials are kept.
    Server {
        status: u16,
    },
    Store(String),
    BadResponse(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::Cancelled => f.write_str("Sign-in was cancelled."),
            AuthError::TimedOut => f.write_str("Sign-in timed out waiting for the browser."),
            AuthError::ProxyNotSupported => f.write_str(
                "Could not connect. A proxy server is set up on this computer, and sign-in with ChatGPT does not support proxies yet.",
            ),
            AuthError::ConsentDenied => f.write_str("Sign-in was declined in the browser."),
            AuthError::Authorization { code } => write!(f, "Sign-in failed ({code})."),
            AuthError::ClientMismatch => {
                f.write_str("The sign-in came back for a different app registration.")
            }
            AuthError::MissingClientId => {
                f.write_str("The sign-in did not return an app registration.")
            }
            AuthError::Exchange { status, code } => {
                write!(f, "Sign-in could not be completed (HTTP {status}, {code}).")
            }
            AuthError::IdToken(r) => write!(f, "Sign-in could not be verified: {r}."),
            AuthError::IdentityMismatch => {
                f.write_str("A different ChatGPT account signed in than the one stored.")
            }
            AuthError::Listener(e) => write!(f, "Could not wait for the browser: {e}."),
            AuthError::Browser(e) => write!(f, "Could not open the browser: {e}."),
            AuthError::SignedOut => f.write_str("Not signed in to ChatGPT."),
            AuthError::Network(e) => write!(f, "Network error: {e}."),
            AuthError::Server { status } => {
                write!(f, "The sign-in service is not available (HTTP {status}).")
            }
            AuthError::Store(e) => write!(f, "{e}."),
            AuthError::BadResponse(e) => {
                write!(f, "Unexpected reply from the sign-in service: {e}.")
            }
        }
    }
}

impl std::error::Error for AuthError {}

fn net(e: HttpError) -> AuthError {
    match e {
        HttpError::Cancelled => AuthError::Cancelled,
        HttpError::Proxy => AuthError::ProxyNotSupported,
        e => AuthError::Network(e.to_string()),
    }
}

/// What sign-out did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignOutOutcome {
    /// OpenAI confirmed the refresh token is revoked. When false the local
    /// tokens are still deleted, and the user can disconnect the app in
    /// ChatGPT settings.
    pub revoked: bool,
}

/// The stored token set.
#[derive(Clone)]
struct Session {
    client_id: String,
    subject: String,
    email: Option<String>,
    name: Option<String>,
    id_token: Secret,
    access_token: Secret,
    refresh_token: Secret,
    /// When the access token was issued (Unix seconds).
    issued_at: u64,
    expires_at: u64,
    refresh_expires_at: u64,
    earliest_refresh_at: Option<u64>,
    scopes: String,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Session(expires_at {})", self.expires_at)
    }
}

impl Session {
    fn to_secret(&self) -> Secret {
        Secret::new(
            json!({
                "v": 1,
                "client_id": self.client_id,
                "subject": self.subject,
                "email": self.email,
                "name": self.name,
                "id_token": self.id_token.expose(),
                "access_token": self.access_token.expose(),
                "refresh_token": self.refresh_token.expose(),
                "issued_at": self.issued_at,
                "expires_at": self.expires_at,
                "refresh_expires_at": self.refresh_expires_at,
                "earliest_refresh_at": self.earliest_refresh_at,
                "scopes": self.scopes,
            })
            .to_string(),
        )
    }

    fn from_secret(s: &Secret) -> Option<Session> {
        let v: Value = serde_json::from_str(s.expose()).ok()?;
        let text = |k: &str| v[k].as_str().map(str::to_string);
        let expires_at = v["expires_at"].as_u64()?;
        Some(Session {
            issued_at: v["issued_at"]
                .as_u64()
                .unwrap_or(expires_at.saturating_sub(3600)),
            client_id: text("client_id")?,
            subject: text("subject")?,
            email: text("email"),
            name: text("name"),
            id_token: Secret::new(text("id_token").unwrap_or_default()),
            access_token: Secret::new(text("access_token")?),
            refresh_token: Secret::new(text("refresh_token")?),
            expires_at,
            refresh_expires_at: v["refresh_expires_at"].as_u64()?,
            earliest_refresh_at: v["earliest_refresh_at"].as_u64(),
            scopes: text("scopes").unwrap_or_default(),
        })
    }

    fn account(&self) -> Account {
        Account {
            subject: self.subject.clone(),
            email: self.email.clone(),
            name: self.name.clone(),
        }
    }

    fn plan_usage(&self) -> bool {
        self.scopes.split_whitespace().any(|s| s == PLAN_SCOPE)
    }
}

/// A function that opens the given URL in the user's browser.
pub type OpenBrowser<'a> = &'a (dyn Fn(&str) -> Result<(), String> + Sync);
/// An owned [`OpenBrowser`], for [`Auth::start_sign_in`].
pub type OpenBrowserFn = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Sign-in, tokens and sign-out. Share it with `Arc`; every method is
/// safe to call from several threads.
pub struct Auth {
    config: AuthConfig,
    store: Arc<dyn CredentialStore>,
    verifier: Arc<dyn IdTokenVerifier>,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    refresh_lock: Mutex<()>,
    refreshes: AtomicUsize,
    /// This value's copy of the stored session (`None`: not read yet).
    known: Mutex<Option<Option<Session>>>,
}

impl fmt::Debug for Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Auth({:?})", self.config.token_url)
    }
}

fn system_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Read a seconds timestamp that may be a number, a numeric string or
/// milliseconds.
fn timestamp(v: &Value) -> Option<u64> {
    let n = v
        .as_u64()
        .or_else(|| v.as_f64().map(|f| f as u64))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))?;
    Some(if n > 100_000_000_000 { n / 1000 } else { n })
}

/// The `error` code of an OAuth error body, reduced to safe characters.
fn oauth_code(body: &[u8]) -> String {
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let code = v["error"]
        .as_str()
        .or_else(|| v["error"]["code"].as_str())
        .or_else(|| v["code"].as_str())
        .unwrap_or("unknown_error");
    util::safe_code(code)
}

impl Auth {
    /// Real ID token checks against `config.jwks_url`, the system clock.
    pub fn new(config: AuthConfig, store: Arc<dyn CredentialStore>) -> Auth {
        let verifier = Arc::new(JwksVerifier::new(config.jwks_url.clone(), config.http));
        Auth {
            config,
            store,
            verifier,
            clock: Arc::new(system_now),
            refresh_lock: Mutex::new(()),
            refreshes: AtomicUsize::new(0),
            known: Mutex::new(None),
        }
    }

    /// Replace the ID token verifier.
    pub fn with_verifier(mut self, verifier: Arc<dyn IdTokenVerifier>) -> Auth {
        self.verifier = verifier;
        self
    }

    /// Replace the clock (Unix seconds), for tests.
    pub fn with_clock(mut self, clock: Arc<dyn Fn() -> u64 + Send + Sync>) -> Auth {
        self.clock = clock;
        self
    }

    pub fn config(&self) -> &AuthConfig {
        &self.config
    }

    /// How many token refreshes this value has made.
    pub fn refresh_count(&self) -> usize {
        self.refreshes.load(Ordering::SeqCst)
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    fn store_err(e: crate::store::StoreError) -> AuthError {
        AuthError::Store(e.to_string())
    }

    /// The session, from this value's own copy once it has read or written
    /// one: Credential Manager can show a write a moment late, and reading
    /// back the token set a refresh just replaced would reuse a retired
    /// refresh token. (Another process changing the store is not seen
    /// until restart; the app runs one instance.)
    fn load(&self) -> Result<Option<Session>, AuthError> {
        let mut known = self.known.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(s) = known.as_ref() {
            return Ok(s.clone());
        }
        let s = self
            .store
            .read(Item::Session)
            .map_err(Self::store_err)?
            .and_then(|s| Session::from_secret(&s));
        *known = Some(s.clone());
        Ok(s)
    }

    fn save(&self, s: &Session) -> Result<(), AuthError> {
        self.store
            .write(Item::Session, &s.to_secret())
            .map_err(Self::store_err)?;
        *self.known.lock().unwrap_or_else(|p| p.into_inner()) = Some(Some(s.clone()));
        Ok(())
    }

    fn clear_session(&self) -> Result<(), AuthError> {
        *self.known.lock().unwrap_or_else(|p| p.into_inner()) = Some(None);
        self.store.delete(Item::Session).map_err(Self::store_err)
    }

    /// This computer's host ID, made and saved on first use.
    pub fn host_id(&self) -> Result<String, AuthError> {
        if let Some(h) = self.store.read(Item::HostId).map_err(Self::store_err)? {
            if !h.is_empty() {
                return Ok(h.expose().to_string());
            }
        }
        let id = util::random_urn_uuid();
        self.store
            .write(Item::HostId, &Secret::new(id.clone()))
            .map_err(Self::store_err)?;
        Ok(id)
    }

    /// The client ID OpenAI issued to this user, once signed in once.
    pub fn issued_client_id(&self) -> Result<Option<String>, AuthError> {
        Ok(self
            .store
            .read(Item::ClientId)
            .map_err(Self::store_err)?
            .map(|s| s.expose().to_string())
            .filter(|s| !s.is_empty() && s != DYNAMIC_CLIENT_ID))
    }

    /// Where the sign-in stands, from storage only.
    pub fn status(&self) -> SignInStatus {
        match self.load() {
            Ok(Some(s)) if self.now() >= s.refresh_expires_at => SignInStatus::Expired {
                account: s.account(),
            },
            Ok(Some(s)) => SignInStatus::SignedIn {
                account: s.account(),
                plan_usage: s.plan_usage(),
            },
            _ => SignInStatus::SignedOut,
        }
    }

    /// Run the whole sign-in: start the loopback listener, hand the
    /// authorization URL to `open_browser`, wait for the browser to come
    /// back (until `config.callback_timeout` or `cancel`), exchange the code
    /// and store the tokens. Blocks; run it on a worker thread.
    pub fn sign_in(
        &self,
        open_browser: OpenBrowser<'_>,
        cancel: &CancelToken,
    ) -> Result<Account, AuthError> {
        let host_id = self.host_id()?;
        let saved_client = self.issued_client_id()?;
        let previous = self.load()?;
        let listener = TcpListener::bind(("127.0.0.1", self.config.preferred_port))
            .or_else(|_| TcpListener::bind(("127.0.0.1", 0)))
            .map_err(|e| AuthError::Listener(e.kind().to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|e| AuthError::Listener(e.kind().to_string()))?
            .port();
        let redirect_uri = format!("http://127.0.0.1:{port}{}", self.config.callback_path);
        let verifier = Secret::new(util::random_token(32));
        let challenge = util::b64url(&crate::win32::sha256(verifier.expose().as_bytes()));
        let state = util::random_token(32);
        let nonce = util::random_token(32);
        let client_for_request = saved_client.as_deref().unwrap_or(DYNAMIC_CLIENT_ID);
        // On a returning sign-in, hints from the stored sign-in of this
        // client skip the account picker.
        let returning = previous
            .as_ref()
            .filter(|p| saved_client.as_ref() == Some(&p.client_id));
        let hint_token = returning
            .map(|p| p.id_token.clone())
            .filter(|t| !t.is_empty());
        let login_hint = returning.and_then(|p| p.email.clone());
        let mut params: Vec<(&str, &str)> = vec![
            ("response_type", "code"),
            ("client_id", client_for_request),
            ("redirect_uri", &redirect_uri),
            ("scope", &self.config.scope),
            ("resource", &self.config.resource),
            ("state", &state),
            ("nonce", &nonce),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("ext_agent_host_id", &host_id),
        ];
        if saved_client.is_none() {
            params.push(("agent_name_hint", &self.config.agent_name));
        }
        if let Some(t) = &hint_token {
            params.push(("id_token_hint", t.expose()));
        }
        if let Some(e) = &login_hint {
            params.push(("login_hint", e));
        }
        let mut url = self.config.authorize_url.clone();
        url.path = format!(
            "{}{}{}",
            url.path,
            if url.path.contains('?') { "&" } else { "?" },
            form(&params)
        );
        let full = format!(
            "{}://{}:{}{}",
            if url.secure { "https" } else { "http" },
            url.host,
            url.port,
            url.path
        );
        let full = Secret::new(full);
        open_browser(full.expose())
            .map_err(|e| AuthError::Browser(util::cut(&e, 200).to_string()))?;

        let callback = wait_for_callback(
            &listener,
            &self.config.callback_path,
            &state,
            self.config.callback_timeout,
            cancel,
        )?;
        drop(listener);

        // The issued client ID: saved before the exchange so a failed
        // exchange does not register a second client next time.
        let client_id = match (&saved_client, callback.client_id.as_deref()) {
            (None, Some(c)) if !c.is_empty() && c != DYNAMIC_CLIENT_ID => {
                self.store
                    .write(Item::ClientId, &Secret::new(c))
                    .map_err(Self::store_err)?;
                c.to_string()
            }
            (None, _) => return Err(AuthError::MissingClientId),
            (Some(saved), Some(c)) if c != saved => return Err(AuthError::ClientMismatch),
            (Some(saved), _) => saved.clone(),
        };

        let body = form(&[
            ("grant_type", "authorization_code"),
            ("client_id", &client_id),
            ("code", callback.code.expose()),
            ("code_verifier", verifier.expose()),
            ("redirect_uri", &redirect_uri),
            ("resource", &self.config.resource),
        ]);
        let body = Secret::new(body);
        let (status, reply) = http::post_form(
            &self.config.token_url,
            body.expose(),
            &self.config.http,
            cancel,
        )
        .map_err(net)?;
        if status != 200 {
            return Err(AuthError::Exchange {
                status,
                code: oauth_code(&reply),
            });
        }
        let tokens = TokenReply::parse(&reply)?;
        let id_token = tokens
            .id_token
            .ok_or_else(|| AuthError::BadResponse("no ID token".into()))?;
        let claims = self
            .verifier
            .verify(
                &id_token,
                &Expected {
                    issuer: &self.config.issuer,
                    audience: &client_id,
                    nonce: Some(&nonce),
                    now: self.now(),
                },
                cancel,
            )
            .map_err(AuthError::IdToken)?;
        let refresh_token = tokens
            .refresh_token
            .ok_or_else(|| AuthError::BadResponse("no refresh token".into()))?;
        // Store under the refresh lock, checked against what is stored now:
        // a refresh that was running cannot overwrite this sign-in.
        let _guard = self.refresh_lock.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(p) = self.load()? {
            if p.client_id == client_id && p.subject != claims.subject {
                return Err(AuthError::IdentityMismatch);
            }
        }
        let now = self.now();
        let session = Session {
            client_id,
            subject: claims.subject.clone(),
            email: claims.email.clone(),
            name: claims.name.clone(),
            id_token,
            access_token: tokens.access_token,
            refresh_token,
            issued_at: now,
            expires_at: now + tokens.expires_in,
            refresh_expires_at: now + REFRESH_LIFETIME,
            earliest_refresh_at: tokens.earliest_refresh_at,
            scopes: tokens.scope.or(callback.scope).unwrap_or_default(),
        };
        self.save(&session)?;
        Ok(session.account())
    }

    /// [`Auth::sign_in`] on its own thread. `open_browser` is called on that
    /// thread. Poll or wait on the receiver; cancel through the handle.
    pub fn start_sign_in(self: &Arc<Self>, open_browser: OpenBrowserFn) -> SignInHandle {
        let cancel = CancelToken::new();
        let (tx, rx) = mpsc::channel();
        let (me, c) = (self.clone(), cancel.clone());
        std::thread::spawn(move || {
            let _ = tx.send(me.sign_in(&*open_browser, &c));
        });
        SignInHandle { cancel, result: rx }
    }

    /// A current access token, refreshed first when it is close to expiry.
    /// Refreshes are serialized: callers that arrive together cause one.
    pub fn access_token(&self, cancel: &CancelToken) -> Result<Secret, AuthError> {
        let _guard = self.refresh_lock.lock().unwrap_or_else(|p| p.into_inner());
        let s = self.load()?.ok_or(AuthError::SignedOut)?;
        let now = self.now();
        if now + self.margin_for(&s) < s.expires_at {
            return Ok(s.access_token);
        }
        if let Some(earliest) = s.earliest_refresh_at {
            if now < earliest && now < s.expires_at {
                return Ok(s.access_token);
            }
        }
        self.refresh_locked(s, cancel)
    }

    /// The API rejected `rejected` (HTTP 401): refresh once, unless another
    /// caller already replaced that token, and return the new one.
    pub fn refresh_after_rejection(
        &self,
        rejected: &Secret,
        cancel: &CancelToken,
    ) -> Result<Secret, AuthError> {
        let _guard = self.refresh_lock.lock().unwrap_or_else(|p| p.into_inner());
        let s = self.load()?.ok_or(AuthError::SignedOut)?;
        if &s.access_token != rejected {
            return Ok(s.access_token);
        }
        self.refresh_locked(s, cancel)
    }

    /// Renew the tokens now, whatever their age: proves the stored sign-in
    /// can still be renewed (Check Setup). Refreshes are serialized as in
    /// [`Auth::access_token`].
    pub fn refresh_now(&self, cancel: &CancelToken) -> Result<(), AuthError> {
        let _guard = self.refresh_lock.lock().unwrap_or_else(|p| p.into_inner());
        let s = self.load()?.ok_or(AuthError::SignedOut)?;
        self.refresh_locked(s, cancel).map(|_| ())
    }

    /// Whether the stored grant allows plan usage.
    pub fn plan_usage_enabled(&self) -> bool {
        matches!(
            self.status(),
            SignInStatus::SignedIn {
                plan_usage: true,
                ..
            }
        )
    }

    fn refresh_locked(&self, s: Session, cancel: &CancelToken) -> Result<Secret, AuthError> {
        if self.now() >= s.refresh_expires_at {
            self.clear_session()?;
            return Err(AuthError::SignedOut);
        }
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        let body = Secret::new(form(&[
            ("grant_type", "refresh_token"),
            ("client_id", &s.client_id),
            ("refresh_token", s.refresh_token.expose()),
            ("resource", &self.config.resource),
        ]));
        let (status, reply) = http::post_form(
            &self.config.token_url,
            body.expose(),
            &self.config.http,
            cancel,
        )
        .map_err(net)?;
        if status >= 500 {
            return Err(AuthError::Server { status });
        }
        if status != 200 {
            let code = oauth_code(&reply);
            if code == "invalid_client" {
                self.clear_session()?;
                self.store.delete(Item::ClientId).map_err(Self::store_err)?;
                return Err(AuthError::SignedOut);
            }
            if TERMINAL_REFRESH_CODES.contains(&code.as_str()) || status == 401 {
                self.clear_session()?;
                return Err(AuthError::SignedOut);
            }
            return Err(AuthError::Exchange { status, code });
        }
        let t = TokenReply::parse(&reply)?;
        let now = self.now();
        let mut next = s.clone();
        next.access_token = t.access_token;
        if let Some(r) = t.refresh_token {
            next.refresh_token = r;
            next.refresh_expires_at = now + REFRESH_LIFETIME;
        }
        next.issued_at = now;
        next.expires_at = now + t.expires_in;
        next.earliest_refresh_at = t.earliest_refresh_at;
        if let Some(scope) = t.scope {
            next.scopes = scope;
        }
        // The server has retired the old refresh token: store the rotated
        // set before anything else can fail (key fetch, checks, cancel).
        self.save_retrying(&next)?;
        if let Some(id) = t.id_token {
            let checked = self.verifier.verify(
                &id,
                &Expected {
                    issuer: &self.config.issuer,
                    audience: &s.client_id,
                    nonce: None,
                    now: self.now(),
                },
                cancel,
            );
            match checked {
                Ok(claims) if claims.subject == s.subject => {
                    next.email = claims.email.or(next.email);
                    next.name = claims.name.or(next.name);
                    next.id_token = id;
                    // Only the account details and hint change here; the
                    // tokens are already stored.
                    let _ = self.save(&next);
                }
                Ok(_) => return Err(AuthError::IdentityMismatch),
                // The old ID token (and account) stay; the new tokens are
                // valid and kept.
                Err(_) if cancel.is_cancelled() => return Err(AuthError::Cancelled),
                Err(_) => {}
            }
        }
        Ok(next.access_token)
    }

    /// Save, trying again twice on a storage error: losing a rotated token
    /// set signs the user out.
    fn save_retrying(&self, s: &Session) -> Result<(), AuthError> {
        let mut last = None;
        for i in 0..3 {
            match self.save(s) {
                Ok(()) => return Ok(()),
                Err(e) => last = Some(e),
            }
            std::thread::sleep(Duration::from_millis(50 * (i + 1)));
        }
        Err(last.unwrap_or(AuthError::Store("storage failed".into())))
    }

    /// The refresh margin for a token: the configured margin, but at most
    /// half the token's lifetime, so a short-lived token is not refreshed
    /// on every call.
    fn margin_for(&self, s: &Session) -> u64 {
        let lifetime = s.expires_at.saturating_sub(s.issued_at);
        self.config.refresh_margin.as_secs().min(lifetime / 2)
    }

    /// Sign out: ask OpenAI to revoke the refresh token (retrying network
    /// and server failures), then delete the stored tokens either way. The
    /// issued client ID and host ID are kept for the next sign-in.
    pub fn sign_out(&self, cancel: &CancelToken) -> Result<SignOutOutcome, AuthError> {
        let _guard = self.refresh_lock.lock().unwrap_or_else(|p| p.into_inner());
        let Some(s) = self.load()? else {
            self.clear_session()?;
            return Ok(SignOutOutcome { revoked: true });
        };
        let body = Secret::new(form(&[
            ("token", s.refresh_token.expose()),
            ("token_type_hint", "refresh_token"),
            ("client_id", &s.client_id),
        ]));
        let mut revoked = false;
        let mut delays = self.config.revoke_retry_delays.iter();
        loop {
            match http::post_form(
                &self.config.revoke_url,
                body.expose(),
                &self.config.http,
                cancel,
            ) {
                Ok((200, _)) => {
                    revoked = true;
                    break;
                }
                Ok((status, _)) if status < 500 => break,
                Err(HttpError::Cancelled) => break,
                _ => {}
            }
            match delays.next() {
                Some(d) if !cancel.sleep(*d) => {}
                _ => break,
            }
        }
        self.clear_session()?;
        Ok(SignOutOutcome { revoked })
    }
}

/// A sign-in running on its own thread. Dropping the handle cancels it, so
/// the callback port is released at once.
pub struct SignInHandle {
    pub cancel: CancelToken,
    pub result: mpsc::Receiver<Result<Account, AuthError>>,
}

impl Drop for SignInHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

struct TokenReply {
    access_token: Secret,
    refresh_token: Option<Secret>,
    id_token: Option<Secret>,
    expires_in: u64,
    scope: Option<String>,
    earliest_refresh_at: Option<u64>,
}

impl TokenReply {
    fn parse(body: &[u8]) -> Result<TokenReply, AuthError> {
        let v: Value = serde_json::from_slice(body)
            .map_err(|_| AuthError::BadResponse("the token reply is not JSON".into()))?;
        let secret = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(Secret::new);
        Ok(TokenReply {
            access_token: secret("access_token")
                .ok_or_else(|| AuthError::BadResponse("no access token".into()))?,
            refresh_token: secret("refresh_token"),
            id_token: secret("id_token"),
            expires_in: timestamp(&v["expires_in"])
                .unwrap_or(3600)
                .clamp(30, 7 * 24 * 3600),
            scope: v["scope"].as_str().map(str::to_string),
            earliest_refresh_at: timestamp(&v["earliest_refresh_at"]),
        })
    }
}

struct Callback {
    code: Secret,
    client_id: Option<String>,
    scope: Option<String>,
}

const PAGE_OK: &str = "Signed in. You can close this tab and go back to OpenXER.";
const PAGE_STRAY: &str = "This is not the sign-in OpenXER is waiting for.";
const PAGE_FAILED: &str =
    "Sign-in did not finish. You can close this tab and try again in OpenXER.";

fn page(status: &str, text: &str) -> String {
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>OpenXER</title></head>\
         <body style=\"font-family:sans-serif;margin:3em\"><p>{text}</p></body></html>"
    );
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn respond(conn: &mut TcpStream, status: &str, text: &str) {
    let _ = conn.write_all(page(status, text).as_bytes());
    let _ = conn.flush();
    let _ = conn.shutdown(std::net::Shutdown::Both);
}

/// How long one browser connection may take to send its request.
const REQUEST_LIMIT: Duration = Duration::from_secs(2);

enum Request {
    Target(String),
    /// Not a usable request (closed, too slow, too large, not GET).
    Bad,
    Cancelled,
    TimedOut,
}

/// Read one request line and headers from a browser connection, waking
/// every 50 ms so cancel and the sign-in deadline are honored while a slow
/// or idle connection is open.
fn read_request_target(conn: &mut TcpStream, deadline: Instant, cancel: &CancelToken) -> Request {
    let _ = conn.set_nonblocking(false);
    let _ = conn.set_read_timeout(Some(Duration::from_millis(50)));
    let limit = Instant::now() + REQUEST_LIMIT;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        if cancel.is_cancelled() {
            return Request::Cancelled;
        }
        let now = Instant::now();
        if now >= deadline {
            return Request::TimedOut;
        }
        if buf.len() > 16 * 1024 || now >= limit {
            return Request::Bad;
        }
        match conn.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    let line = String::from_utf8_lossy(&buf);
    let Some(first) = line.lines().next() else {
        return Request::Bad;
    };
    let mut parts = first.split(' ');
    match (parts.next(), parts.next()) {
        (Some("GET"), Some(target)) => Request::Target(target.to_string()),
        _ => Request::Bad,
    }
}

fn wait_for_callback(
    listener: &TcpListener,
    path: &str,
    state: &str,
    timeout: Duration,
    cancel: &CancelToken,
) -> Result<Callback, AuthError> {
    listener
        .set_nonblocking(true)
        .map_err(|e| AuthError::Listener(e.kind().to_string()))?;
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.is_cancelled() {
            return Err(AuthError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(AuthError::TimedOut);
        }
        let mut conn = match listener.accept() {
            Ok((c, _)) => c,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            Err(e) => return Err(AuthError::Listener(e.kind().to_string())),
        };
        let target = match read_request_target(&mut conn, deadline, cancel) {
            Request::Target(t) => t,
            Request::Bad => {
                respond(&mut conn, "400 Bad Request", PAGE_FAILED);
                continue;
            }
            Request::Cancelled => return Err(AuthError::Cancelled),
            Request::TimedOut => return Err(AuthError::TimedOut),
        };
        let (p, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
        if p != path {
            respond(&mut conn, "404 Not Found", "Not found.");
            continue;
        }
        let q = util::parse_query(query);
        let get = |k: &str| q.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        if get("state").as_deref() != Some(state) {
            // Not this sign-in's answer (another page or program can reach
            // the port): refuse it and keep waiting for the real one.
            respond(&mut conn, "400 Bad Request", PAGE_STRAY);
            continue;
        }
        if let Some(err) = get("error") {
            respond(&mut conn, "200 OK", PAGE_FAILED);
            return Err(if err == "access_denied" {
                AuthError::ConsentDenied
            } else {
                AuthError::Authorization {
                    code: util::safe_code(&err),
                }
            });
        }
        let Some(code) = get("code").filter(|c| !c.is_empty()) else {
            respond(&mut conn, "400 Bad Request", PAGE_FAILED);
            return Err(AuthError::BadResponse("the callback has no code".into()));
        };
        respond(&mut conn, "200 OK", PAGE_OK);
        return Ok(Callback {
            code: Secret::new(code),
            client_id: get("client_id"),
            scope: get("scope"),
        });
    }
}
