//! End-to-end tests against fake servers on 127.0.0.1: a fake
//! authorization server, a fake browser that follows the redirect itself,
//! and a fake Responses API. Nothing here reaches a real host or opens a
//! browser.

mod budgets;
mod signin;
mod turns;

use crate::auth::SCOPES;
use crate::http::HttpConfig;
pub use crate::testkit::*;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;
