//! Tauri commands: the only entry points from the UI into Rust.
//!
//! Every argument crossing the boundary is validated here (ranges, lengths,
//! paths) before it reaches a collector or the database. Commands that block
//! (OS queries, database reads, dialogs) are `async` and run off the UI thread.

pub mod export;
pub mod gaming;
pub mod hardware;
pub mod history;
pub mod network;
pub mod processes;
pub mod settings;
pub mod stability;
pub mod system;

/// Command result with a user-presentable error message.
pub type CmdResult<T> = Result<T, String>;

pub(crate) const MAX_RANGE_MS: i64 = 400 * 24 * 3_600_000;

/// Validates a `[from, to]` time range.
pub(crate) fn check_range(from_ms: i64, to_ms: i64) -> CmdResult<()> {
    if to_ms <= from_ms || to_ms - from_ms > MAX_RANGE_MS || from_ms < 0 {
        return Err("Invalid time range".into());
    }
    Ok(())
}

pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("Background task failed: {e}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn range_validation() {
        assert!(super::check_range(0, 1).is_ok());
        assert!(super::check_range(5, 5).is_err());
        assert!(super::check_range(-1, 5).is_err());
        assert!(super::check_range(0, super::MAX_RANGE_MS + 1).is_err());
    }
}
