//! ID token checks: an RS256 signature against the issuer's published keys
//! (JWKS), then issuer, audience, expiry and nonce, as the sign-in docs
//! require. Reasons are fixed phrases; token content never appears in them.

use crate::http::{self, HttpConfig, Url};
use crate::util::{b64url_decode, CancelToken, Secret};
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// What the ID token says about the account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdClaims {
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub name: Option<String>,
}

/// The values an ID token must carry.
#[derive(Clone, Debug)]
pub struct Expected<'a> {
    pub issuer: &'a str,
    pub audience: &'a str,
    /// The nonce sent with this sign-in; `None` for a token from a refresh,
    /// which carries the original nonce or none.
    pub nonce: Option<&'a str>,
    /// Now, Unix seconds.
    pub now: u64,
}

/// Checks ID tokens. The real one is [`JwksVerifier`]; tests may pass their own.
pub trait IdTokenVerifier: Send + Sync {
    fn verify(
        &self,
        token: &Secret,
        expected: &Expected<'_>,
        cancel: &CancelToken,
    ) -> Result<IdClaims, String>;
}

const CLOCK_SKEW: u64 = 5;

#[derive(Clone)]
struct Jwk {
    kid: Option<String>,
    n: Vec<u8>,
    e: Vec<u8>,
}

/// Checks RS256 ID tokens against the keys at a JWKS URL, fetched on first
/// use and again when a token names a key it does not know (at most once a
/// minute).
pub struct JwksVerifier {
    jwks_url: Url,
    http: HttpConfig,
    keys: Mutex<(Vec<Jwk>, Option<Instant>)>,
}

impl std::fmt::Debug for JwksVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JwksVerifier({:?})", self.jwks_url)
    }
}

impl JwksVerifier {
    pub fn new(jwks_url: Url, http: HttpConfig) -> JwksVerifier {
        JwksVerifier {
            jwks_url,
            http,
            keys: Mutex::new((Vec::new(), None)),
        }
    }

    fn fetch(&self, cancel: &CancelToken) -> Result<Vec<Jwk>, String> {
        let resp = http::send(
            "GET",
            &self.jwks_url,
            &[("Accept", "application/json")],
            b"",
            &self.http,
            cancel,
        )
        .map_err(|e| format!("could not fetch the signing keys ({e})"))?;
        if resp.status != 200 {
            return Err(format!(
                "the signing keys request failed (HTTP {})",
                resp.status
            ));
        }
        let body = resp
            .read_all(1 << 20)
            .map_err(|e| format!("could not fetch the signing keys ({e})"))?;
        let v: Value = serde_json::from_slice(&body)
            .map_err(|_| "the signing keys are not JSON".to_string())?;
        let mut out = Vec::new();
        for k in v["keys"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
            if k["kty"] != "RSA" {
                continue;
            }
            if k.get("use").is_some_and(|u| u != "sig") {
                continue;
            }
            if k.get("alg").is_some_and(|a| a != "RS256") {
                continue;
            }
            let (Some(n), Some(e)) = (
                k["n"].as_str().and_then(b64url_decode),
                k["e"].as_str().and_then(b64url_decode),
            ) else {
                continue;
            };
            out.push(Jwk {
                kid: k["kid"].as_str().map(str::to_string),
                n,
                e,
            });
        }
        Ok(out)
    }

    fn key_for(&self, kid: Option<&str>, cancel: &CancelToken) -> Result<Jwk, String> {
        let mut guard = self.keys.lock().unwrap();
        let find = |keys: &[Jwk]| -> Option<Jwk> {
            match kid {
                Some(kid) => keys.iter().find(|k| k.kid.as_deref() == Some(kid)).cloned(),
                None if keys.len() == 1 => keys.first().cloned(),
                None => None,
            }
        };
        let key = match find(&guard.0) {
            Some(k) => k,
            None => {
                let recent = guard
                    .1
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(60));
                if !recent || guard.0.is_empty() {
                    let keys = self.fetch(cancel)?;
                    *guard = (keys, Some(Instant::now()));
                }
                find(&guard.0)
                    .ok_or_else(|| "the ID token is signed with an unknown key".to_string())?
            }
        };
        if strip_zeros(&key.n).len() * 8 < MIN_RSA_BITS {
            return Err("the ID token signing key is too short".into());
        }
        Ok(key)
    }
}

/// The smallest RSA modulus accepted for ID token signatures.
pub const MIN_RSA_BITS: usize = 2048;

fn strip_zeros(b: &[u8]) -> &[u8] {
    let first = b.iter().position(|&x| x != 0).unwrap_or(b.len());
    &b[first..]
}

/// The JSON payload of a JWT, without any checks.
pub fn payload(token: &str) -> Option<Value> {
    let p = token.split('.').nth(1)?;
    serde_json::from_slice(&b64url_decode(p)?).ok()
}

/// Check the claims of a decoded payload.
pub fn check_claims(claims: &Value, expected: &Expected<'_>) -> Result<IdClaims, String> {
    if claims["iss"].as_str() != Some(expected.issuer) {
        return Err("the ID token is from an unexpected issuer".into());
    }
    let (aud_ok, several) = match &claims["aud"] {
        Value::String(a) => (a == expected.audience, false),
        Value::Array(a) => (
            a.iter().any(|x| x.as_str() == Some(expected.audience)),
            a.len() > 1,
        ),
        _ => (false, false),
    };
    if !aud_ok {
        return Err("the ID token is for a different client".into());
    }
    // OpenID Connect Core 3.1.3.7: with several audiences the authorized
    // party must be this client, and a present azp must always be.
    match claims["azp"].as_str() {
        Some(azp) if azp != expected.audience => {
            return Err("the ID token was issued to a different client".into())
        }
        None if several => return Err("the ID token names no authorized client".into()),
        _ => {}
    }
    let exp = claims["exp"].as_u64().ok_or("the ID token has no expiry")?;
    if exp + CLOCK_SKEW < expected.now {
        return Err("the ID token has expired".into());
    }
    let iat = claims["iat"]
        .as_u64()
        .ok_or("the ID token has no issue time")?;
    if iat > expected.now + CLOCK_SKEW {
        return Err("the ID token is issued in the future".into());
    }
    if let Some(nbf) = claims.get("nbf") {
        match nbf.as_u64() {
            Some(nbf) if nbf <= expected.now + CLOCK_SKEW => {}
            _ => return Err("the ID token is not valid yet".into()),
        }
    }
    if let Some(nonce) = expected.nonce {
        if claims["nonce"].as_str() != Some(nonce) {
            return Err("the ID token nonce did not match".into());
        }
    }
    let subject = claims["sub"].as_str().unwrap_or("");
    if subject.is_empty() {
        return Err("the ID token has no subject".into());
    }
    let text = |k: &str| {
        claims[k]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    Ok(IdClaims {
        subject: subject.to_string(),
        email: text("email"),
        email_verified: claims["email_verified"] == Value::Bool(true),
        name: text("name"),
    })
}

impl IdTokenVerifier for JwksVerifier {
    fn verify(
        &self,
        token: &Secret,
        expected: &Expected<'_>,
        cancel: &CancelToken,
    ) -> Result<IdClaims, String> {
        let t = token.expose();
        let parts: Vec<&str> = t.split('.').collect();
        if parts.len() != 3 {
            return Err("the ID token is not a signed JWT".into());
        }
        let header: Value = b64url_decode(parts[0])
            .and_then(|b| serde_json::from_slice(&b).ok())
            .ok_or("the ID token header is not readable")?;
        if header["alg"] != "RS256" {
            return Err("the ID token is not signed with RS256".into());
        }
        let signature = b64url_decode(parts[2]).ok_or("the ID token signature is not readable")?;
        let key = self.key_for(header["kid"].as_str(), cancel)?;
        let signed = &t[..parts[0].len() + 1 + parts[1].len()];
        if !crate::win32::rs256_verify(&key.n, &key.e, signed.as_bytes(), &signature) {
            return Err("the ID token signature is not valid".into());
        }
        let claims = payload(t).ok_or("the ID token payload is not readable")?;
        check_claims(&claims, expected)
    }
}

#[cfg(any(test, feature = "testkit"))]
pub(crate) mod testing {
    use crate::util::b64url;
    use crate::win32::TestRsaKey;
    use serde_json::{json, Value};

    /// A signing key and its JWKS document, for fake servers.
    pub struct Signer {
        pub key: TestRsaKey,
        pub kid: String,
    }

    impl Signer {
        pub fn new(kid: &str) -> Signer {
            Signer::with_bits(kid, 2048)
        }
        pub fn with_bits(kid: &str, bits: u32) -> Signer {
            Signer {
                key: TestRsaKey::generate_bits(bits),
                kid: kid.into(),
            }
        }
        pub fn jwks(&self) -> Value {
            json!({"keys": [{"kty": "RSA", "use": "sig", "alg": "RS256", "kid": self.kid,
                "n": b64url(&self.key.n), "e": b64url(&self.key.e)}]})
        }
        pub fn sign(&self, claims: &Value) -> String {
            let h = b64url(
                json!({"alg": "RS256", "typ": "JWT", "kid": self.kid})
                    .to_string()
                    .as_bytes(),
            );
            let p = b64url(claims.to_string().as_bytes());
            let s = b64url(&self.key.sign(format!("{h}.{p}").as_bytes()));
            format!("{h}.{p}.{s}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::Signer;
    use super::*;
    use crate::testkit::FakeServer;
    use serde_json::json;

    fn claims(now: u64) -> Value {
        json!({"iss": "https://issuer.test", "aud": "oaiapp_1", "exp": now + 3600, "iat": now,
            "nonce": "n1", "sub": "user-1", "email": "pat@example.test", "email_verified": true,
            "name": "Pat Example"})
    }

    fn expected(now: u64) -> Expected<'static> {
        Expected {
            issuer: "https://issuer.test",
            audience: "oaiapp_1",
            nonce: Some("n1"),
            now,
        }
    }

    #[test]
    fn verifies_signature_and_claims_against_jwks() {
        let signer = Signer::new("k1");
        let server = FakeServer::start();
        server.route_json("GET", "/jwks", 200, signer.jwks());
        let v = JwksVerifier::new(server.url("/jwks"), HttpConfig::default());
        let now = 1_900_000_000;
        let token = Secret::new(signer.sign(&claims(now)));
        let c = v
            .verify(&token, &expected(now), &CancelToken::new())
            .unwrap();
        assert_eq!(c.subject, "user-1");
        assert_eq!(c.email.as_deref(), Some("pat@example.test"));
        assert!(c.email_verified);
        assert_eq!(c.name.as_deref(), Some("Pat Example"));
        // The keys are cached: a second check makes no request.
        v.verify(&token, &expected(now), &CancelToken::new())
            .unwrap();
        assert_eq!(server.count("GET", "/jwks"), 1);
    }

    #[test]
    fn rejects_bad_signature_issuer_audience_expiry_and_nonce() {
        let signer = Signer::new("k1");
        let other = Signer::new("k1");
        let server = FakeServer::start();
        server.route_json("GET", "/jwks", 200, signer.jwks());
        let v = JwksVerifier::new(server.url("/jwks"), HttpConfig::default());
        let now = 1_900_000_000;
        let c = CancelToken::new();
        let forged = Secret::new(other.sign(&claims(now)));
        assert_eq!(
            v.verify(&forged, &expected(now), &c).unwrap_err(),
            "the ID token signature is not valid"
        );
        let mut bad = claims(now);
        bad["iss"] = json!("https://evil.test");
        assert!(v
            .verify(&Secret::new(signer.sign(&bad)), &expected(now), &c)
            .unwrap_err()
            .contains("issuer"));
        let mut bad = claims(now);
        bad["aud"] = json!(["someone-else"]);
        assert!(v
            .verify(&Secret::new(signer.sign(&bad)), &expected(now), &c)
            .unwrap_err()
            .contains("client"));
        let mut bad = claims(now);
        bad["exp"] = json!(now - 60);
        assert!(v
            .verify(&Secret::new(signer.sign(&bad)), &expected(now), &c)
            .unwrap_err()
            .contains("expired"));
        let mut bad = claims(now);
        bad["nonce"] = json!("other");
        assert!(v
            .verify(&Secret::new(signer.sign(&bad)), &expected(now), &c)
            .unwrap_err()
            .contains("nonce"));
        let mut bad = claims(now);
        bad["sub"] = json!("");
        assert!(v
            .verify(&Secret::new(signer.sign(&bad)), &expected(now), &c)
            .unwrap_err()
            .contains("subject"));
        assert!(v.verify(&Secret::new("a.b"), &expected(now), &c).is_err());
        // An unknown key ID makes it fetch again, then fail.
        let stranger = Signer::new("k9");
        assert!(v
            .verify(
                &Secret::new(stranger.sign(&claims(now))),
                &expected(now),
                &c
            )
            .unwrap_err()
            .contains("unknown key"));
    }

    #[test]
    fn audience_may_be_a_list_and_skew_is_allowed() {
        let now = 1_900_000_000;
        let mut c = claims(now);
        c["aud"] = json!(["x", "oaiapp_1"]);
        c["azp"] = json!("oaiapp_1");
        c["exp"] = json!(now - 3);
        c["iat"] = json!(now + 3);
        c["nbf"] = json!(now + 3);
        assert!(check_claims(&c, &expected(now)).is_ok());
    }

    #[test]
    fn azp_nbf_and_future_iat_are_enforced() {
        let now = 1_900_000_000;
        let e = expected(now);
        let mut c = claims(now);
        c["aud"] = json!(["x", "oaiapp_1"]);
        assert!(check_claims(&c, &e)
            .unwrap_err()
            .contains("authorized client"));
        c["azp"] = json!("x");
        assert!(check_claims(&c, &e)
            .unwrap_err()
            .contains("different client"));
        let mut c = claims(now);
        c["azp"] = json!("someone");
        assert!(check_claims(&c, &e).is_err(), "a present azp must match");
        let mut c = claims(now);
        c["nbf"] = json!(now + 60);
        assert!(check_claims(&c, &e).unwrap_err().contains("not valid yet"));
        c["nbf"] = json!("soon");
        assert!(check_claims(&c, &e).is_err());
        let mut c = claims(now);
        c["iat"] = json!(now + 60);
        assert!(check_claims(&c, &e).unwrap_err().contains("future"));
    }

    #[test]
    fn a_short_signing_key_is_refused() {
        let weak = Signer::with_bits("k1", 1024);
        let server = FakeServer::start();
        server.route_json("GET", "/jwks", 200, weak.jwks());
        let v = JwksVerifier::new(server.url("/jwks"), HttpConfig::default());
        let now = 1_900_000_000;
        let token = Secret::new(weak.sign(&claims(now)));
        assert_eq!(
            v.verify(&token, &expected(now), &CancelToken::new())
                .unwrap_err(),
            "the ID token signing key is too short"
        );
    }
}
