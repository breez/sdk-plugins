use crate::error::NostrResult;

use super::ZapReceiptsHandler;

impl ZapReceiptsHandler {
    pub async fn on_init(&self) -> NostrResult<()> {
        self.ctx.persister.clean_expired_zaps()?;
        Ok(())
    }
}
