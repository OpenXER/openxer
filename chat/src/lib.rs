//! Chat with OpenAI models on the user's own ChatGPT plan ("Sign in with
//! ChatGPT" for open-source, locally run apps), for the Chat window.
//!
//! - [`Auth`]: sign-in through the user's browser (the caller opens it),
//!   token refresh, sign-out, status. Tokens live in Windows Credential
//!   Manager ([`WindowsCredentialStore`]).
//! - [`ChatClient`]: streamed turns against the Responses API with a tool
//!   loop over a [`Tools`] set, reported as [`ChatEvent`]s on a channel.
//! - [`Conversation`]: the history and its token [`Budgets`].
//!
//! Everything blocks on worker threads; the UI thread only polls channels.
//! No async runtime.

mod auth;
mod client;
pub mod compat;
mod conversation;
pub mod http;
mod jwt;
mod sse;
mod store;
mod stream;
mod tools;
#[cfg(not(windows))]
mod unix;
mod util;
#[cfg(windows)]
mod win32;
#[cfg(not(windows))]
use unix as win32;

#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
#[cfg(test)]
mod tests;

pub use auth::{
    Account, Auth, AuthConfig, AuthError, OpenBrowser, OpenBrowserFn, SignInHandle, SignInStatus,
    SignOutOutcome, AGENT_NAME, DYNAMIC_CLIENT_ID, PLAN_SCOPE, SCOPES,
};
pub use client::{
    default_model, ApiConfig, ChatClient, ChatError, ChatEvent, ModelInfo, TurnHandle, TurnRequest,
    USAGE_LIMIT_CODE, USAGE_SETTINGS_URL,
};
pub use conversation::{
    cap_tool_result, summarize_result, Budgets, Conversation, Role, BASE_INSTRUCTIONS,
};
pub use jwt::{IdClaims, IdTokenVerifier};
pub use sse::{SseEvent, SseParser};
pub use store::{
    CredentialStore, Item, KeyId, MemoryStore, StoreError, WindowsCredentialStore,
    DEFAULT_CREDENTIAL_PREFIX,
};
pub use stream::Usage;
pub use tools::{default_target, NoTools, ProposedChanges, ToolDef, ToolOutput, Tools};
pub use util::{CancelToken, Secret};

/// What the sign-in dialog must show before the first sign-in, with a
/// "Continue with ChatGPT" button and Cancel.
pub const CONSENT_TEXT: &str = "\
Chat with your ChatGPT plan

If you sign in, OpenXER sends your chat questions, and the schedule data \
its tools read to answer them (for example activity IDs, names, dates, \
logic, resources and costs from the open project), from this computer \
directly to OpenAI. OpenAI processes them under your ChatGPT account, \
and the use counts against your ChatGPT plan's limits. OpenAI's terms and \
privacy policy apply.

Nothing is sent until you sign in and ask a question. Chat sends \
schedule data off this computer only this way or with an API key you \
set up. Your \
sign-in is kept in Windows Credential Manager for your Windows account.

To stop, choose Sign out in the Chat window. To also remove OpenXER's \
access from your account, disconnect it in ChatGPT settings, Security \
and login.";

/// Interface wording from OpenAI's guidelines for apps that use a ChatGPT
/// plan (developers.openai.com/siwc/ui-ux-guidelines).
pub mod ui {
    /// The sign-in button.
    pub const SIGN_IN_BUTTON: &str = "Continue with ChatGPT";
    /// Shown once, after the first sign-in with plan usage allowed.
    pub const WELCOME_TITLE: &str = "You're using your ChatGPT plan";
    pub const WELCOME_BODY: &str =
        "Eligible usage in this app uses your ChatGPT plan. Manage usage in your ChatGPT settings.";
    pub const WELCOME_BUTTON: &str = "Got it";
    /// Near the composer or model picker while requests use the plan.
    pub const PLAN_INDICATOR: &str = "Using ChatGPT plan";
    /// Beside the indicator, and the main action of the usage-limit message;
    /// opens [`crate::USAGE_SETTINGS_URL`].
    pub const MANAGE_USAGE: &str = "Manage usage";
    pub const USAGE_LIMIT_TITLE: &str = "Usage limit reached";
    pub const USAGE_LIMIT_BODY: &str = "Review your plan or this app's limit in ChatGPT settings.";
}
