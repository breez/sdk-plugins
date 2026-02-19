use super::ZapReceiptsHandler;
use crate::{handlers::routines::HandlerRoutines, model::Payment};
use anyhow::Result;
use nostr_sdk::{Event, Filter};
use tokio::time::Interval;

#[sdk_macros::async_trait]
impl HandlerRoutines for ZapReceiptsHandler {
    async fn on_init(&self) -> Result<()> {
        self.ctx.persister.clean_expired_zaps()?;
        Ok(())
    }

    async fn on_connect(&self) -> Result<()> {
        Ok(())
    }

    async fn on_interval(&self) -> Result<()> {
        Ok(())
    }

    async fn on_relay_event(&self, _event: &Event) -> Result<()> {
        Ok(())
    }

    async fn on_sdk_payment(&self, _payment: &Payment) -> Result<()> {
        Ok(())
    }

    async fn on_resubscribe(&self, _maybe_expiry_interval: &mut Option<Interval>) -> Result<()> {
        Ok(())
    }

    async fn on_destroy(&self) -> Result<()> {
        Ok(())
    }

    fn set_filters(&self, _filters: &mut Filter) {}
}
