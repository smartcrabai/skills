use crate::cli::UpdateArgs;
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
pub async fn run(_args: UpdateArgs) -> Result<()> {
    Err(Error::NotImplemented)
}
