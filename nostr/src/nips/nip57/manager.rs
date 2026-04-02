use super::ZapReceiptsService;
use crate::{NostrManager, error::NostrResult};

#[sdk_macros::async_trait]
impl ZapReceiptsService for NostrManager {
    async fn track_zap(&self, invoice: String, zap_request: String) -> NostrResult<()> {
        self.handlers()
            .await?
            .zaps
            .track_zap(invoice, zap_request)
            .await
    }

    async fn is_zap(&self, invoice: String) -> NostrResult<bool> {
        self.handlers().await?.zaps.is_zap(invoice).await
    }
}
