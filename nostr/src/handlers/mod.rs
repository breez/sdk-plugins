pub(crate) mod builder;
pub(crate) mod routines;

use std::sync::Arc;

use crate::{
    context::RuntimeContext,
    event::{NostrEvent, NostrEventDetails},
    handlers::routines::HandlerRoutines,
    model::Payment,
    sdk_event::SdkEventListener,
};
use anyhow::Result;
use log::info;
use nostr_sdk::{Event, Filter};
use tokio::time::Interval;

#[cfg(feature = "nip47")]
use crate::nips::nip47::NostrWalletConnectHandler;

#[cfg(feature = "nip57")]
use crate::nips::nip57::ZapReceiptsHandler;

pub(crate) struct NostrHandlers {
    pub ctx: Arc<RuntimeContext>,
    #[cfg(feature = "nip47")]
    pub nwc: NostrWalletConnectHandler,
    #[cfg(feature = "nip57")]
    pub zaps: ZapReceiptsHandler,
}

#[sdk_macros::async_trait]
impl HandlerRoutines for NostrHandlers {
    async fn on_init(&self) -> Result<()> {
        self.ctx.event_manager.resume_notifications();
        #[cfg(feature = "nip47")]
        self.nwc.on_init().await?;
        #[cfg(feature = "nip57")]
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

        #[cfg(feature = "nip47")]
        self.nwc.on_connect().await?;
        #[cfg(feature = "nip57")]
        self.zaps.on_connect().await?;
        Ok(())
    }

    async fn on_interval(&self) -> Result<()> {
        #[cfg(feature = "nip47")]
        self.nwc.on_interval().await?;
        #[cfg(feature = "nip57")]
        self.zaps.on_interval().await?;
        Ok(())
    }

    async fn on_relay_event(&self, event: &Event) -> Result<()> {
        #[cfg(feature = "nip47")]
        self.nwc.on_relay_event(event).await?;
        #[cfg(feature = "nip57")]
        self.zaps.on_relay_event(event).await?;
        Ok(())
    }

    async fn on_resubscribe(&self, maybe_expiry_interval: &mut Option<Interval>) -> Result<()> {
        let mut filters = Filter::new();
        self.set_filters(&mut filters);
        self.ctx.client.subscribe(filters, None).await?;
        info!("Successfully subscribed to events");

        #[cfg(feature = "nip47")]
        self.nwc.on_resubscribe(maybe_expiry_interval).await?;
        #[cfg(feature = "nip57")]
        self.zaps.on_resubscribe(maybe_expiry_interval).await?;
        Ok(())
    }

    async fn on_destroy(&self) -> Result<()> {
        #[cfg(feature = "nip47")]
        self.nwc.on_destroy().await?;
        #[cfg(feature = "nip57")]
        self.zaps.on_destroy().await?;
        Ok(())
    }

    fn set_filters(&self, filters: &mut Filter) {
        #[cfg(feature = "nip47")]
        self.nwc.set_filters(filters);
        #[cfg(feature = "nip57")]
        self.zaps.set_filters(filters);
    }
}

#[sdk_macros::async_trait]
impl SdkEventListener for NostrHandlers {
    async fn on_sdk_payment(&self, payment: &Payment) {
        #[cfg(feature = "nip47")]
        self.nwc.on_sdk_payment(payment).await;
        #[cfg(feature = "nip57")]
        self.zaps.on_sdk_payment(payment).await;
    }
}

#[sdk_macros::async_trait]
impl SdkEventListener for Arc<NostrHandlers> {
    async fn on_sdk_payment(&self, payment: &Payment) {
        Self::on_sdk_payment(self, payment).await;
    }
}
