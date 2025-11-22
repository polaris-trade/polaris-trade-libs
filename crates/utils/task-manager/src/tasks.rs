use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::TaskResult;

/// Basic trait that all tasks must implement, either directly or via a wrapper.
#[async_trait]
pub trait RunnableTask: Send + Sync + 'static {
    fn name(&self) -> &str;

    /// Main execution - receives a cancellation token for shutdown coordination.
    ///
    /// The CancellationToken provides:
    /// - `token.cancelled().await` - Wait for shutdown signal
    /// - `token.is_cancelled()` - Check if shutdown was requested
    /// - `token.cancel()` - Trigger shutdown
    async fn run(&self, token: CancellationToken) -> TaskResult<()>;

    /// Optional cleanup after shutdown.
    async fn on_shutdown(&self) -> TaskResult<()> {
        Ok(())
    }

    /// Optional initialization before run.
    async fn init(&self) -> TaskResult<()> {
        Ok(())
    }

    /// Optional readiness check.
    async fn ready(&self) -> TaskResult<()> {
        Ok(())
    }

    /// Optional metrics reporting.
    async fn metrics(&self) -> TaskResult<()> {
        Ok(())
    }
}
