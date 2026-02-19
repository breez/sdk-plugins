use anyhow::Result;

use nostr_sdk::{Event, Filter};
use tokio::time::Interval;

use crate::model::Payment;

#[sdk_macros::async_trait]
pub trait HandlerRoutines: Send + Sync {
    async fn on_init(&self) -> Result<()>;
    async fn on_connect(&self) -> Result<()>;
    async fn on_interval(&self) -> Result<()>;
    async fn on_relay_event(&self, event: &Event) -> Result<()>;
    async fn on_sdk_payment(&self, payment: &Payment) -> Result<()>;
    async fn on_resubscribe(&self, maybe_expiry_interval: &mut Option<Interval>) -> Result<()>;
    async fn on_destroy(&self) -> Result<()>;
    fn set_filters(&self, filter: &mut Filter);
}
