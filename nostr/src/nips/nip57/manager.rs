use super::ZapReceiptsService;
use crate::{error::NostrResult, NostrManager};

#[sdk_macros::async_trait]
impl ZapReceiptsService for NostrManager {
    async fn track_zap(&self, invoice: String, zap_request: String) -> NostrResult<()> {
        self.handlers()
            .await?
            .zaps
            .track_zap(invoice, zap_request)
            .await
    }
}
