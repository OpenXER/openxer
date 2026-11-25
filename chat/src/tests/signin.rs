//! Sign-in, tokens and sign-out.

use super::*;
use crate::auth::{AuthError, SignInStatus, AGENT_NAME, DYNAMIC_CLIENT_ID};
use crate::store::{CredentialStore, Item};
use crate::util::CancelToken;
use std::net::TcpListener;
use std::time::Instant;

#[test]
fn first_sign_in_registers_a_client_and_the_next_reuses_it() {
    let fx = Fixture::new();
    assert_eq!(fx.auth.status(), SignInStatus::SignedOut);
    let account = fx
        .auth
        .sign_in(&fx.browser(Browser::approve_first()), &CancelToken::new())
        .unwrap();
    assert_eq!(account.email.as_deref(), Some("pat@example.test"));
    assert_eq!(account.name.as_deref(), Some("Pat Example"));
    assert_eq!(account.label(), "pat@example.test");

    let a = fx.authorize(0);
    assert_eq!(a["client_id"], DYNAMIC_CLIENT_ID);
    assert_eq!(a["agent_name_hint"], AGENT_NAME);
    assert_eq!(a["response_type"], "code");
    assert_eq!(a["scope"], SCOPES);
    assert_eq!(a["resource"], "https://api.openai.com/v1");
    assert_eq!(a["code_challenge_method"], "S256");
    assert_eq!(a["code_challenge"].len(), 43);
    assert!(a["state"].len() >= 43 && a["nonce"].len() >= 43);
    assert_ne!(a["state"], a["nonce"]);
    assert!(a["ext_agent_host_id"].starts_with("urn:uuid:"));
    assert!(a["redirect_uri"].starts_with("http://127.0.0.1:"));
    assert!(a["redirect_uri"].ends_with("/auth/callback"));
    assert!(!a.contains_key("id_token_hint"));

    // The issued client ID is saved, never the dynamic one.
    let saved = fx.store.read(Item::ClientId).unwrap().unwrap();
    assert_eq!(saved.expose(), ISSUED_CLIENT);
    assert_eq!(
        fx.auth.status(),
        SignInStatus::SignedIn {
            account: account.clone(),
            plan_usage: true
        }
    );
    let s = fx.stored_session();
    assert_eq!(s["access_token"], "AT-SECRET-1");
    assert_eq!(s["refresh_token"], "RT-SECRET-1");
    assert_eq!(s["expires_at"], T0 + 3600);

    // Signing in again while signed in: the saved client, the same host,
    // hints from the stored sign-in, no name hint.
    fx.auth
        .sign_in(&fx.browser(Browser::approve_again()), &CancelToken::new())
        .unwrap();
    let b = fx.authorize(1);
    assert_eq!(b["client_id"], ISSUED_CLIENT);
    assert!(!b.contains_key("agent_name_hint"));
    assert_eq!(b["ext_agent_host_id"], a["ext_agent_host_id"]);
    assert!(b["id_token_hint"].starts_with("eyJ"));
    assert_eq!(b["login_hint"], "pat@example.test");
    assert_ne!(b["state"], a["state"]);
    assert_ne!(b["code_challenge"], a["code_challenge"]);

    // After sign-out the client and host are kept and reused.
    let out = fx.auth.sign_out(&CancelToken::new()).unwrap();
    assert!(out.revoked);
    assert_eq!(fx.auth.status(), SignInStatus::SignedOut);
    assert!(fx.store.has(Item::ClientId) && fx.store.has(Item::HostId));
    fx.auth
        .sign_in(&fx.browser(Browser::approve_again()), &CancelToken::new())
        .unwrap();
    let c = fx.authorize(2);
    assert_eq!(c["client_id"], ISSUED_CLIENT);
    assert_eq!(c["ext_agent_host_id"], a["ext_agent_host_id"]);
    assert!(!c.contains_key("id_token_hint"));
    assert!(matches!(fx.auth.status(), SignInStatus::SignedIn { .. }));
}

#[test]
fn the_callback_ignores_other_paths() {
    let fx = Fixture::new();
    fx.auth
        .sign_in(
            &fx.browser(Browser::FaviconThenApprove),
            &CancelToken::new(),
        )
        .unwrap();
    assert!(matches!(fx.auth.status(), SignInStatus::SignedIn { .. }));
}

#[test]
fn wrong_state_is_refused_and_nothing_is_exchanged() {
    let fx = Fixture::with_config(|c| c.callback_timeout = Duration::from_millis(800));
    let e = fx
        .auth
        .sign_in(&fx.browser(Browser::WrongState), &CancelToken::new())
        .unwrap_err();
    // The stray answers do not end the sign-in; it waits on until timeout.
    assert_eq!(e, AuthError::TimedOut);
    assert_eq!(fx.server.count("POST", "/token"), 0);
    assert!(!fx.store.has(Item::Session) && !fx.store.has(Item::ClientId));
}

#[test]
fn stray_requests_do_not_end_a_sign_in() {
    let fx = Fixture::new();
    fx.auth
        .sign_in(&fx.browser(Browser::StrayThenApprove), &CancelToken::new())
        .unwrap();
    assert!(matches!(fx.auth.status(), SignInStatus::SignedIn { .. }));
    assert_eq!(fx.server.count("POST", "/token"), 1);
}

#[test]
fn an_idle_connection_does_not_hold_off_cancel_or_the_deadline() {
    let fx = Fixture::new();
    let cancel = CancelToken::new();
    let c2 = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        c2.cancel();
    });
    let t = Instant::now();
    let e = fx
        .auth
        .sign_in(&fx.browser(Browser::Hold), &cancel)
        .unwrap_err();
    assert_eq!(e, AuthError::Cancelled);
    assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());

    let fx = Fixture::with_config(|c| c.callback_timeout = Duration::from_millis(400));
    let t = Instant::now();
    let e = fx
        .auth
        .sign_in(&fx.browser(Browser::Hold), &CancelToken::new())
        .unwrap_err();
    assert_eq!(e, AuthError::TimedOut);
    assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
}

#[test]
fn dropping_the_handle_cancels_and_frees_the_port() {
    let fx = Fixture::new();
    let h = fx
        .auth
        .start_sign_in(Box::new(fx.browser(Browser::Nothing)));
    // Wait until the browser was asked, so the port is bound.
    let t = Instant::now();
    while fx.state.lock().unwrap().authorize.is_empty() {
        assert!(t.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    let port = fx.callback_port(0);
    drop(h);
    let t = Instant::now();
    loop {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            break;
        }
        assert!(
            t.elapsed() < Duration::from_secs(1),
            "the port stayed bound after the handle was dropped"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_fresh_sign_in_is_not_overwritten_by_a_refresh_in_flight() {
    let fx = Fixture::new().signed_in();
    fx.state.lock().unwrap().refresh_stall = Duration::from_millis(700);
    fx.set_time(T0 + 4000);
    let a = fx.auth.clone();
    let refresh = std::thread::spawn(move || a.access_token(&CancelToken::new()));
    // Let the refresh reach the server (generation 2), then sign in again
    // (generation 3) while the refresh reply is held back.
    let t = Instant::now();
    while fx.state.lock().unwrap().generation < 2 {
        assert!(t.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
    fx.auth
        .sign_in(&fx.browser(Browser::approve_again()), &CancelToken::new())
        .unwrap();
    refresh.join().unwrap().unwrap();
    let s = fx.stored_session();
    assert_eq!(
        s["access_token"], "AT-SECRET-3",
        "the sign-in is stored last"
    );
    assert_eq!(s["refresh_token"], "RT-SECRET-3");
}

#[test]
fn a_refresh_with_an_id_token_updates_the_account() {
    let fx = Fixture::new().signed_in();
    fx.state.lock().unwrap().refresh_id_email = Some("new@example.test".into());
    fx.set_time(T0 + 4000);
    assert_eq!(
        fx.auth.access_token(&CancelToken::new()).unwrap().expose(),
        "AT-SECRET-2"
    );
    let s = fx.stored_session();
    assert_eq!(s["email"], "new@example.test");
    assert_eq!(s["refresh_token"], "RT-SECRET-2");
}

#[test]
fn rotated_tokens_survive_a_failed_key_fetch() {
    let fx = Fixture::new().signed_in();
    fx.state.lock().unwrap().refresh_id_email = Some("new@example.test".into());
    fx.server
        .route("GET", "/jwks", |_| Reply::json(500, &json!({})));
    fx.set_time(T0 + 4000);
    // A second instance has no keys cached, so it must fetch them.
    let auth = fx.second_auth();
    let token = auth.access_token(&CancelToken::new()).unwrap();
    assert_eq!(token.expose(), "AT-SECRET-2");
    let s = fx.stored_session();
    assert_eq!(
        s["refresh_token"], "RT-SECRET-2",
        "the rotated token is kept"
    );
    assert_eq!(
        s["email"], "pat@example.test",
        "the unchecked ID token is not used"
    );
    // The next refresh uses the rotated token: not reused, not signed out.
    fx.set_time(T0 + 8000);
    assert_eq!(
        auth.access_token(&CancelToken::new()).unwrap().expose(),
        "AT-SECRET-3"
    );
    assert!(matches!(auth.status(), SignInStatus::SignedIn { .. }));
}

#[test]
fn rotated_tokens_survive_cancel_during_the_key_fetch() {
    let fx = Fixture::new().signed_in();
    fx.state.lock().unwrap().refresh_id_email = Some("new@example.test".into());
    let jwks = fx.server.seen("GET", "/jwks");
    assert_eq!(jwks.len(), 1);
    let mut slow = Reply::json(200, &json!({"keys": []}));
    slow.stall = Duration::from_secs(3);
    fx.server.route("GET", "/jwks", move |_| slow.clone());
    fx.set_time(T0 + 4000);
    let auth = fx.second_auth();
    let cancel = CancelToken::new();
    let c2 = cancel.clone();
    let server = &fx.server;
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let t = Instant::now();
            while server.count("GET", "/jwks") < 2 {
                assert!(t.elapsed() < Duration::from_secs(5));
                std::thread::sleep(Duration::from_millis(5));
            }
            c2.cancel();
        });
        let t = Instant::now();
        assert_eq!(
            auth.access_token(&cancel).unwrap_err(),
            AuthError::Cancelled
        );
        assert!(t.elapsed() < Duration::from_secs(2));
    });
    assert_eq!(fx.stored_session()["refresh_token"], "RT-SECRET-2");
    fx.state.lock().unwrap().refresh_id_email = None;
    fx.set_time(T0 + 8000);
    assert_eq!(
        auth.access_token(&CancelToken::new()).unwrap().expose(),
        "AT-SECRET-3"
    );
}

#[test]
fn a_short_lived_token_is_not_refreshed_on_every_call() {
    let fx = Fixture::new();
    fx.state.lock().unwrap().expires_in = 60;
    let fx = fx.signed_in();
    let c = CancelToken::new();
    for t in [T0, T0 + 10, T0 + 29] {
        fx.set_time(t);
        assert_eq!(fx.auth.access_token(&c).unwrap().expose(), "AT-SECRET-1");
    }
    assert_eq!(fx.auth.refresh_count(), 0);
    // Within half the lifetime of the end.
    fx.set_time(T0 + 31);
    assert_eq!(fx.auth.access_token(&c).unwrap().expose(), "AT-SECRET-2");
    assert_eq!(fx.auth.access_token(&c).unwrap().expose(), "AT-SECRET-2");
    assert_eq!(fx.auth.refresh_count(), 1);
}

#[test]
fn denied_consent_stops_without_exchange() {
    let fx = Fixture::new();
    let e = fx
        .auth
        .sign_in(&fx.browser(Browser::Deny), &CancelToken::new())
        .unwrap_err();
    assert_eq!(e, AuthError::ConsentDenied);
    assert_eq!(fx.server.count("POST", "/token"), 0);
    assert_eq!(fx.auth.status(), SignInStatus::SignedOut);
}

#[test]
fn sign_in_times_out() {
    let fx = Fixture::with_config(|c| c.callback_timeout = Duration::from_millis(300));
    let t = Instant::now();
    let e = fx
        .auth
        .sign_in(&fx.browser(Browser::Nothing), &CancelToken::new())
        .unwrap_err();
    assert_eq!(e, AuthError::TimedOut);
    assert!(t.elapsed() < Duration::from_secs(3));
}

#[test]
fn sign_in_cancels_promptly_and_frees_the_port() {
    let fx = Fixture::new();
    let cancel = CancelToken::new();
    let c2 = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        c2.cancel();
    });
    let t = Instant::now();
    let e = fx
        .auth
        .sign_in(&fx.browser(Browser::Nothing), &cancel)
        .unwrap_err();
    assert_eq!(e, AuthError::Cancelled);
    assert!(t.elapsed() < Duration::from_secs(2));
    let redirect = fx.authorize(0)["redirect_uri"].clone();
    let port: u16 = redirect
        .trim_start_matches("http://127.0.0.1:")
        .split('/')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    TcpListener::bind(("127.0.0.1", port)).expect("the callback port is released");
}

#[test]
fn the_handle_runs_sign_in_on_a_thread() {
    let fx = Fixture::new();
    let browser = fx.browser(Browser::approve_first());
    let h = fx.auth.start_sign_in(Box::new(browser));
    let r = h.result.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(r.is_ok());
    let h = fx
        .auth
        .start_sign_in(Box::new(fx.browser(Browser::Nothing)));
    h.cancel.cancel();
    assert_eq!(
        h.result.recv_timeout(Duration::from_secs(5)).unwrap(),
        Err(AuthError::Cancelled)
    );
}

#[test]
fn a_taken_port_falls_back_to_another() {
    let busy = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let fx = Fixture::with_config(|c| c.preferred_port = port);
    fx.auth
        .sign_in(&fx.browser(Browser::approve_first()), &CancelToken::new())
        .unwrap();
    let redirect = &fx.authorize(0)["redirect_uri"];
    assert!(!redirect.contains(&format!(":{port}/")), "{redirect}");
    assert!(redirect.ends_with("/auth/callback"));
}

#[test]
fn a_failed_exchange_keeps_the_issued_client_for_next_time() {
    let fx = Fixture::new();
    fx.state.lock().unwrap().token_override = Some((
        400,
        json!({"error": "invalid_grant", "error_description": "CODE-SECRET used"}),
    ));
    let e = fx
        .auth
        .sign_in(&fx.browser(Browser::approve_first()), &CancelToken::new())
        .unwrap_err();
    assert_eq!(
        e,
        AuthError::Exchange {
            status: 400,
            code: "invalid_grant".into()
        }
    );
    assert_no_secrets(&e.to_string());
    assert!(!fx.store.has(Item::Session));
    assert_eq!(
        fx.auth.issued_client_id().unwrap().as_deref(),
        Some(ISSUED_CLIENT)
    );
    fx.state.lock().unwrap().token_override = None;
    fx.auth
        .sign_in(&fx.browser(Browser::approve_again()), &CancelToken::new())
        .unwrap();
    assert_eq!(fx.authorize(1)["client_id"], ISSUED_CLIENT);
}

#[test]
fn callback_client_id_rules() {
    // First registration must return an issued client ID.
    let fx = Fixture::new();
    let e = fx
        .auth
        .sign_in(&fx.browser(Browser::approve_again()), &CancelToken::new())
        .unwrap_err();
    assert_eq!(e, AuthError::MissingClientId);
    let e = fx
        .auth
        .sign_in(
            &fx.browser(Browser::Approve {
                client_id: Some(DYNAMIC_CLIENT_ID.into()),
                scope: SCOPES.into(),
            }),
            &CancelToken::new(),
        )
        .unwrap_err();
    assert_eq!(e, AuthError::MissingClientId);
    // A returning sign-in that names another client is rejected.
    let fx = Fixture::new().signed_in();
    let e = fx
        .auth
        .sign_in(
            &fx.browser(Browser::Approve {
                client_id: Some("oaiapp_someone_else".into()),
                scope: SCOPES.into(),
            }),
            &CancelToken::new(),
        )
        .unwrap_err();
    assert_eq!(e, AuthError::ClientMismatch);
}

#[test]
fn without_the_plan_scope_the_sign_in_is_kept_but_plan_usage_is_off() {
    let fx = Fixture::new();
    fx.auth
        .sign_in(
            &fx.browser(Browser::Approve {
                client_id: Some(ISSUED_CLIENT.into()),
                scope: "openid profile email offline_access".into(),
            }),
            &CancelToken::new(),
        )
        .unwrap();
    assert!(matches!(
        fx.auth.status(),
        SignInStatus::SignedIn {
            plan_usage: false,
            ..
        }
    ));
    assert!(!fx.auth.plan_usage_enabled());
}

#[test]
fn a_failing_browser_is_reported() {
    let fx = Fixture::new();
    let e = fx
        .auth
        .sign_in(
            &|_: &str| Err("no handler".to_string()),
            &CancelToken::new(),
        )
        .unwrap_err();
    assert_eq!(e, AuthError::Browser("no handler".into()));
}

#[test]
fn an_id_token_with_a_bad_signature_is_rejected() {
    let fx = Fixture::new();
    // Swap the signer's keys: the token no longer matches the JWKS.
    fx.server.route_json(
        "GET",
        "/jwks",
        200,
        crate::jwt::testing::Signer::new("kid-1").jwks(),
    );
    let e = fx
        .auth
        .sign_in(&fx.browser(Browser::approve_first()), &CancelToken::new())
        .unwrap_err();
    assert!(matches!(e, AuthError::IdToken(_)), "{e:?}");
    assert!(!fx.store.has(Item::Session));
}

#[test]
fn the_access_token_is_refreshed_before_it_expires() {
    let fx = Fixture::new().signed_in();
    let c = CancelToken::new();
    fx.set_time(T0 + 600);
    assert_eq!(fx.auth.access_token(&c).unwrap().expose(), "AT-SECRET-1");
    assert_eq!(fx.auth.refresh_count(), 0);
    // Within the five-minute margin.
    fx.set_time(T0 + 3600 - 200);
    assert_eq!(fx.auth.access_token(&c).unwrap().expose(), "AT-SECRET-2");
    assert_eq!(fx.auth.refresh_count(), 1);
    let sent = fx.server.seen("POST", "/token");
    let f = sent.last().unwrap().form();
    assert_eq!(f["grant_type"], "refresh_token");
    assert_eq!(f["client_id"], ISSUED_CLIENT);
    assert_eq!(f["refresh_token"], "RT-SECRET-1");
    assert_eq!(f["resource"], "https://api.openai.com/v1");
    assert!(!f.contains_key("scope"));
    // The rotated refresh token is stored and used next time.
    let s = fx.stored_session();
    assert_eq!(s["refresh_token"], "RT-SECRET-2");
    assert_eq!(s["expires_at"], T0 + 3600 - 200 + 3600);
    assert_eq!(s["refresh_expires_at"], T0 + 3600 - 200 + 30 * 24 * 3600);
    fx.set_time(T0 + 2 * 3600);
    assert_eq!(fx.auth.access_token(&c).unwrap().expose(), "AT-SECRET-3");
    let f = fx.server.seen("POST", "/token").last().unwrap().form();
    assert_eq!(f["refresh_token"], "RT-SECRET-2");
    // The account is kept across refreshes.
    assert!(
        matches!(fx.auth.status(), SignInStatus::SignedIn { account, .. } if account.email.as_deref() == Some("pat@example.test"))
    );
}

#[test]
fn callers_arriving_together_cause_one_refresh() {
    let fx = Fixture::new().signed_in();
    fx.state.lock().unwrap().refresh_stall = Duration::from_millis(300);
    fx.set_time(T0 + 4000);
    let tokens: Vec<String> = (0..8)
        .map(|_| {
            let a = fx.auth.clone();
            std::thread::spawn(move || {
                a.access_token(&CancelToken::new())
                    .unwrap()
                    .expose()
                    .to_string()
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect();
    assert!(tokens.iter().all(|t| t == "AT-SECRET-2"), "{tokens:?}");
    assert_eq!(fx.auth.refresh_count(), 1);
    assert_eq!(fx.server.count("POST", "/token"), 2); // sign-in + one refresh
}

#[test]
fn a_rejection_refreshes_once_even_when_reported_twice() {
    let fx = Fixture::new().signed_in();
    let c = CancelToken::new();
    let old = fx.auth.access_token(&c).unwrap();
    let new = fx.auth.refresh_after_rejection(&old, &c).unwrap();
    assert_eq!(new.expose(), "AT-SECRET-2");
    // A second caller that saw the same old token gets the new one, no refresh.
    let again = fx.auth.refresh_after_rejection(&old, &c).unwrap();
    assert_eq!(again, new);
    assert_eq!(fx.auth.refresh_count(), 1);
}

#[test]
fn a_rejected_refresh_token_signs_out() {
    for code in [
        "invalid_grant",
        "refresh_token_reused",
        "refresh_token_expired",
    ] {
        let fx = Fixture::new().signed_in();
        fx.state.lock().unwrap().refresh_override = Some((400, json!({"error": code})));
        fx.set_time(T0 + 4000);
        let e = fx.auth.access_token(&CancelToken::new()).unwrap_err();
        assert_eq!(e, AuthError::SignedOut);
        assert_eq!(fx.auth.status(), SignInStatus::SignedOut);
        assert!(!fx.store.has(Item::Session));
        assert!(
            fx.store.has(Item::ClientId),
            "the client is kept for {code}"
        );
    }
    // invalid_client: the registration itself is unusable.
    let fx = Fixture::new().signed_in();
    fx.state.lock().unwrap().refresh_override = Some((401, json!({"error": "invalid_client"})));
    fx.set_time(T0 + 4000);
    assert_eq!(
        fx.auth.access_token(&CancelToken::new()).unwrap_err(),
        AuthError::SignedOut
    );
    assert!(!fx.store.has(Item::ClientId));
}

#[test]
fn a_server_failure_during_refresh_keeps_the_tokens() {
    let fx = Fixture::new().signed_in();
    fx.state.lock().unwrap().refresh_override =
        Some((503, json!({"error": "temporarily_unavailable"})));
    fx.set_time(T0 + 4000);
    assert_eq!(
        fx.auth.access_token(&CancelToken::new()).unwrap_err(),
        AuthError::Server { status: 503 }
    );
    assert!(fx.store.has(Item::Session));
}

#[test]
fn a_sign_in_past_thirty_days_is_expired() {
    let fx = Fixture::new().signed_in();
    fx.set_time(T0 + 31 * 24 * 3600);
    assert!(matches!(fx.auth.status(), SignInStatus::Expired { .. }));
    assert_eq!(
        fx.auth.access_token(&CancelToken::new()).unwrap_err(),
        AuthError::SignedOut
    );
    assert_eq!(fx.auth.status(), SignInStatus::SignedOut);
}

#[test]
fn sign_out_revokes_and_clears_even_when_revocation_fails() {
    let fx = Fixture::new().signed_in();
    fx.auth.sign_out(&CancelToken::new()).unwrap();
    let f = fx.server.seen("POST", "/revoke")[0].form();
    assert_eq!(f["token"], "RT-SECRET-1");
    assert_eq!(f["token_type_hint"], "refresh_token");
    assert_eq!(f["client_id"], ISSUED_CLIENT);
    assert!(!fx.store.has(Item::Session));

    let fx = Fixture::new().signed_in();
    fx.server.route("POST", "/revoke", |_| Reply::empty(503));
    let out = fx.auth.sign_out(&CancelToken::new()).unwrap();
    assert!(!out.revoked);
    assert_eq!(
        fx.server.count("POST", "/revoke"),
        3,
        "one try and two retries"
    );
    assert!(!fx.store.has(Item::Session));
    assert_eq!(
        fx.auth.access_token(&CancelToken::new()).unwrap_err(),
        AuthError::SignedOut
    );
    // Signing out when signed out is fine.
    assert!(fx.auth.sign_out(&CancelToken::new()).is_ok());
}

#[test]
fn nothing_secret_shows_in_debug_output_or_errors() {
    let fx = Fixture::new().signed_in();
    let c = CancelToken::new();
    let token = fx.auth.access_token(&c).unwrap();
    let mut texts = vec![
        format!("{token:?}"),
        format!("{:?}", fx.auth),
        format!("{:?}", fx.auth.status()),
        format!("{:?}", fx.store),
        format!("{:?}", fx.auth.config()),
    ];
    // Errors from every failing path.
    fx.state.lock().unwrap().refresh_override = Some((
        400,
        json!({"error": "invalid_grant", "error_description": "RT-SECRET-1 is bad"}),
    ));
    fx.set_time(T0 + 4000);
    let e = fx.auth.access_token(&c).unwrap_err();
    texts.push(format!("{e:?} {e}"));
    let fx = Fixture::new();
    fx.state.lock().unwrap().token_override = Some((
        400,
        json!({"error": "invalid_grant", "error_description": "CODE-SECRET AT-SECRET-1"}),
    ));
    let e = fx
        .auth
        .sign_in(&fx.browser(Browser::approve_first()), &c)
        .unwrap_err();
    texts.push(format!("{e:?} {e}"));
    for b in [Browser::WrongState, Browser::Deny] {
        let e = fx.auth.sign_in(&fx.browser(b), &c).unwrap_err();
        texts.push(format!("{e:?} {e}"));
    }
    for t in &texts {
        assert_no_secrets(t);
    }
}

/// A store that, once armed, says it wrote the session but keeps the old
/// one (as Windows was seen to do under heavy load).
struct LossyStore {
    inner: crate::store::MemoryStore,
    lose: std::sync::atomic::AtomicBool,
}

impl crate::store::CredentialStore for LossyStore {
    fn read(&self, item: Item) -> Result<Option<crate::Secret>, crate::StoreError> {
        self.inner.read(item)
    }
    fn write(&self, item: Item, value: &crate::Secret) -> Result<(), crate::StoreError> {
        if item == Item::Session && self.lose.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }
        self.inner.write(item, value)
    }
    fn delete(&self, item: Item) -> Result<(), crate::StoreError> {
        self.inner.delete(item)
    }
}

#[test]
fn this_process_keeps_using_the_tokens_it_saved_last() {
    let fx = Fixture::new();
    let store = Arc::new(LossyStore {
        inner: crate::store::MemoryStore::new(),
        lose: std::sync::atomic::AtomicBool::new(false),
    });
    let clock = fx.clock.clone();
    let auth = Arc::new(
        crate::Auth::new(fx.auth.config().clone(), store.clone()).with_clock(Arc::new(move || {
            clock.load(std::sync::atomic::Ordering::SeqCst)
        })),
    );
    auth.sign_in(&fx.browser(Browser::approve_first()), &CancelToken::new())
        .unwrap();
    store.lose.store(true, std::sync::atomic::Ordering::SeqCst);
    fx.set_time(T0 + 4000);
    assert_eq!(
        auth.access_token(&CancelToken::new()).unwrap().expose(),
        "AT-SECRET-2"
    );
    // The store still holds generation 1, but the next refresh sends the
    // rotated token this process saved, so it is not refused as reused.
    fx.set_time(T0 + 8000);
    assert_eq!(
        auth.access_token(&CancelToken::new()).unwrap().expose(),
        "AT-SECRET-3"
    );
    let f = fx.server.seen("POST", "/token").last().unwrap().form();
    assert_eq!(f["refresh_token"], "RT-SECRET-2");
    assert!(matches!(auth.status(), SignInStatus::SignedIn { .. }));
}

#[test]
fn refresh_now_renews_the_tokens_whatever_their_age() {
    let fx = Fixture::new().signed_in();
    let before = fx.auth.refresh_count();
    fx.auth.refresh_now(&CancelToken::new()).unwrap();
    assert_eq!(fx.auth.refresh_count(), before + 1);
    assert!(fx.auth.plan_usage_enabled(), "still signed in");
    let rejected = Fixture::new().signed_in();
    rejected.state.lock().unwrap().refresh_override =
        Some((400, json!({"error": "refresh_token_expired"})));
    assert_eq!(
        rejected.auth.refresh_now(&CancelToken::new()),
        Err(AuthError::SignedOut)
    );
    let never = Fixture::new();
    assert_eq!(
        never.auth.refresh_now(&CancelToken::new()),
        Err(AuthError::SignedOut)
    );
}

#[test]
fn reach_connects_without_sending_a_request() {
    let fx = Fixture::new();
    let url = fx.server.url("/probe");
    crate::http::reach(&url, &HttpConfig::default(), &CancelToken::new()).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fx.server.count("GET", "/probe"), 0, "nothing was asked");
    let closed = crate::http::Url::parse("http://127.0.0.1:9/").unwrap();
    let quick = HttpConfig {
        connect_timeout: Duration::from_millis(500),
        read_timeout: Duration::from_millis(500),
    };
    assert!(crate::http::reach(&closed, &quick, &CancelToken::new()).is_err());
}
