pub(crate) mod builder;
pub(crate) mod routines;

use std::sync::Arc;

use crate::{
    context::RuntimeContext,
    event::{NostrEvent, NostrEventDetails},
    handlers::routines::HandlerRoutines,
    model::Payment,
    nips::{nip47::NostrWalletConnectHandler, nip57::ZapReceiptsHandler},
    sdk_event::SdkEventListener,
};
use anyhow::Result;
use log::info;
use nostr_sdk::{Event, Filter};
use tokio::time::Interval;

pub(crate) struct NostrHandlers {
    pub ctx: Arc<RuntimeContext>,
    pub nwc: NostrWalletConnectHandler,
    pub zaps: ZapReceiptsHandler,
}

#[sdk_macros::async_trait]
impl HandlerRoutines for NostrHandlers {
    async fn on_init(&self) -> Result<()> {
        self.ctx.event_manager.resume_notifications();
        self.nwc.on_init().await?;
        self.zaps.on_init().await?;
        Ok(())
    }

    async fn on_connect(&self) -> Result<()> {
        info!("Successfully connected Nostr client");
        self.ctx
            .event_manager
            .notify(NostrEvent {
                details: NostrEventDetails::Connected,
                event_id: None,
            })
            .await;

        self.nwc.on_connect().await?;
        self.zaps.on_connect().await?;
        Ok(())
    }

    async fn on_interval(&self) -> Result<()> {
        self.nwc.on_interval().await?;
        self.zaps.on_interval().await?;
        Ok(())
    }

    async fn on_relay_event(&self, event: &Event) -> Result<()> {
        self.nwc.on_relay_event(event).await?;
        self.zaps.on_relay_event(event).await?;
        Ok(())
    }

    async fn on_resubscribe(&self, maybe_expiry_interval: &mut Option<Interval>) -> Result<()> {
        let mut filters = Filter::new();
        self.set_filters(&mut filters);
        self.ctx.client.subscribe(filters, None).await?;
        info!("Successfully subscribed to events");

        self.nwc.on_resubscribe(maybe_expiry_interval).await?;
        self.zaps.on_resubscribe(maybe_expiry_interval).await?;
        Ok(())
    }

    async fn on_destroy(&self) -> Result<()> {
        self.nwc.on_destroy().await?;
        self.zaps.on_destroy().await?;
        Ok(())
    }

    fn set_filters(&self, filters: &mut Filter) {
        self.nwc.set_filters(filters);
        self.zaps.set_filters(filters);
    }
}

#[sdk_macros::async_trait]
impl SdkEventListener for NostrHandlers {
    async fn on_sdk_payment(&self, payment: &Payment) {
        self.nwc.on_sdk_payment(payment).await;
        self.zaps.on_sdk_payment(payment).await;
    }
}

#[sdk_macros::async_trait]
impl SdkEventListener for Arc<NostrHandlers> {
    async fn on_sdk_payment(&self, payment: &Payment) {
        Self::on_sdk_payment(self, payment).await;
    }
}
