use crate::cli::ListArgs;
use crate::error::{Error, Result};

/// Stub: filled in by Phase B.
///
/// # Errors
///
/// Always returns [`Error::NotImplemented`].
#[expect(
    clippy::unused_async,
    reason = "Phase B implementations will use async I/O"
)]
pub async fn run(_args: ListArgs) -> Result<()> {
    Err(Error::NotImplemented)
}
