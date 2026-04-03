mod forward;
mod persist;
mod routines;
mod sdk_services;

use std::sync::Arc;

use crate::{
    context::RuntimeContext, error::NostrResult, nips::nip57::sdk_services::ZapEventHandler,
};

use log::info;
use nostr_sdk::Event;

pub(crate) mod model;

#[sdk_macros::async_trait]
pub trait ZapReceiptsService {
    /// Tracks an incoming zap until the payment is complete, and broadcasts the associated
    /// zap_receipt
    ///
    /// # Arguments
    ///
    /// * `invoice` - The invoice related to the zap request
    /// * `zap_request` - the URL- and JSON-encoded zap request
    async fn track_zap(&self, invoice: String, zap_request: String) -> NostrResult<()>;

    /// Whether or not an invoice was registered to track a zap
    ///
    /// # Arguments
    ///
    /// * `invoice` - The invoice related to the zap
    async fn is_zap(&self, invoice: String) -> NostrResult<bool>;
}

pub(crate) struct ZapReceiptsHandler {
    pub ctx: Arc<RuntimeContext>,
    pub event_handler: ZapEventHandler,
}

impl ZapReceiptsHandler {
    pub fn new(ctx: Arc<RuntimeContext>) -> Self {
        Self {
            ctx,
            event_handler: ZapEventHandler {},
        }
    }
}

#[sdk_macros::async_trait]
impl ZapReceiptsService for ZapReceiptsHandler {
    async fn track_zap(&self, invoice: String, zap_request: String) -> NostrResult<()> {
        let zap_request = urlencoding::decode(&zap_request)?.into_owned();
        let zap_request_event: Event = serde_json::from_str(&zap_request)?;
        zap_request_event.verify()?;
        self.ctx
            .persister
            .add_tracked_zap(invoice.clone(), zap_request)
            .await?;
        info!("Successfully added zap tracking for invoice {invoice}");
        Ok(())
    }

    async fn is_zap(&self, invoice: String) -> NostrResult<bool> {
        Ok(self
            .ctx
            .persister
            .get_tracked_zap_raw(&invoice)
            .await?
            .is_some())
    }
}
