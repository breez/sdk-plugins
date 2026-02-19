pub(crate) mod builder;
pub(crate) mod routines;

use std::sync::Arc;

use crate::{
    context::RuntimeContext,
    handlers::routines::HandlerRoutines,
    model::Payment,
    nips::{nip47::NostrWalletConnectHandler, nip57::ZapReceiptsHandler},
};
use anyhow::Result;
use log::info;
use nostr_sdk::{Event, Filter};
use tokio::time::Interval;

pub struct NostrHandlers {
    ctx: Arc<RuntimeContext>,
    nwc: Option<NostrWalletConnectHandler>,
    zaps: Option<ZapReceiptsHandler>,
}

#[sdk_macros::async_trait]
impl HandlerRoutines for NostrHandlers {
    async fn on_init(&self) -> Result<()> {
        self.ctx.event_manager.resume_notifications();
        if let Some(ref nwc) = self.nwc {
            nwc.on_init().await?;
        }
        if let Some(ref zaps) = self.zaps {
            zaps.on_init().await?;
        }
        Ok(())
    }

    async fn on_connect(&self) -> Result<()> {
        if let Some(ref nwc) = self.nwc {
            nwc.on_connect().await?;
        }
        if let Some(ref zaps) = self.zaps {
            zaps.on_connect().await?;
        }
        Ok(())
    }

    async fn on_interval(&self) -> Result<()> {
        if let Some(ref nwc) = self.nwc {
            nwc.on_interval().await?;
        }
        if let Some(ref zaps) = self.zaps {
            zaps.on_interval().await?;
        }
        Ok(())
    }

    async fn on_relay_event(&self, event: &Event) -> Result<()> {
        if let Some(ref nwc) = self.nwc {
            nwc.on_relay_event(event).await?;
        }
        if let Some(ref zaps) = self.zaps {
            zaps.on_relay_event(event).await?;
        }
        Ok(())
    }

    async fn on_sdk_payment(&self, payment: &Payment) -> Result<()> {
        if let Some(ref nwc) = self.nwc {
            nwc.on_sdk_payment(payment).await?;
        }
        if let Some(ref zaps) = self.zaps {
            zaps.on_sdk_payment(payment).await?;
        }
        Ok(())
    }

    async fn on_resubscribe(&self, maybe_expiry_interval: &mut Option<Interval>) -> Result<()> {
        let mut filters = Filter::new();
        self.set_filters(&mut filters);
        self.ctx.client.subscribe(filters, None).await?;
        info!("Successfully subscribed to events");

        if let Some(ref nwc) = self.nwc {
            nwc.on_resubscribe(maybe_expiry_interval).await?;
        }
        if let Some(ref zaps) = self.zaps {
            zaps.on_resubscribe(maybe_expiry_interval).await?;
        }
        Ok(())
    }

    async fn on_destroy(&self) -> Result<()> {
        if let Some(ref nwc) = self.nwc {
            nwc.on_destroy().await?;
        }
        if let Some(ref zaps) = self.zaps {
            zaps.on_destroy().await?;
        }
        Ok(())
    }

    fn set_filters(&self, filters: &mut Filter) {
        if let Some(ref nwc) = self.nwc {
            nwc.set_filters(filters);
        }
        if let Some(ref zaps) = self.zaps {
            zaps.set_filters(filters);
        }
    }
}
