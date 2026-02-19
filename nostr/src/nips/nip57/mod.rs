mod persist;
pub(crate) mod routines;

use std::sync::Arc;

use crate::{context::RuntimeContext, error::NostrResult};

use log::info;
use nostr_sdk::Event;

pub(crate) mod model;

pub(crate) struct ZapReceiptsHandler {
    pub ctx: Arc<RuntimeContext>,
}

impl ZapReceiptsHandler {
    pub(crate) fn new(ctx: Arc<RuntimeContext>) -> Self {
        Self { ctx }
    }
}

impl ZapReceiptsHandler {
    async fn track_zap(&self, invoice: String, zap_request: String) -> NostrResult<()> {
        let zap_request = urlencoding::decode(&zap_request)?.into_owned();
        let zap_request_event: Event = serde_json::from_str(&zap_request)?;
        zap_request_event.verify()?;
        self.ctx
            .persister
            .add_tracked_zap(invoice.clone(), zap_request)?;
        info!("Successfully added zap tracking for invoice {invoice}");
        Ok(())
    }
}
