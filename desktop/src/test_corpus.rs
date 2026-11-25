//! The local XER corpus used by tests and the performance bench. It holds
//! client data, so it lives outside the repository: `OPENXER_CORPUS` names
//! its folder (on a machine that has it, set it in the `[env]` section of the
//! ignored `.cargo/config.toml`). Without it, every corpus test skips.
use std::path::PathBuf;

/// The corpus folder named by `OPENXER_CORPUS`. When the variable is not set
/// this is a path that does not exist, so callers skip as for a missing file.
pub(crate) fn dir() -> PathBuf {
    std::env::var_os("OPENXER_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("OPENXER_CORPUS is not set"))
}

/// One corpus file by name, for example `file("369.xer")`.
pub(crate) fn file(name: &str) -> PathBuf {
    dir().join(name)
}
