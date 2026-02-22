use std::{collections::BTreeSet, time::Duration};

use super::{event::NwcEventKind, NostrWalletConnectHandler};
use crate::{
    event::{NostrEvent, NostrEventDetails},
    handlers::routines::HandlerRoutines,
};
use anyhow::Result;
use nostr_sdk::{Alphabet, Event, Filter, Kind, SingleLetterTag};
use tokio::time::Interval;

#[sdk_macros::async_trait]
impl HandlerRoutines for NostrWalletConnectHandler {
    async fn on_init(&self) -> Result<()> {
        Ok(())
    }

    async fn on_connect(&self) -> Result<()> {
        self.send_info_event().await?;
        Ok(())
    }

    async fn on_interval(&self) -> Result<()> {
        let result = self.ctx.persister.refresh_connections()?;
        for connection_name in result.deleted {
            self.ctx
                .event_manager
                .notify(NostrEvent {
                    event_id: None,
                    details: NostrEventDetails::Nwc {
                        kind: NwcEventKind::ConnectionExpired,
                        connection_name: Some(connection_name),
                    },
                })
                .await;
        }
        for connection_name in result.refreshed {
            self.ctx
                .event_manager
                .notify(NostrEvent {
                    event_id: None,
                    details: NostrEventDetails::Nwc {
                        kind: NwcEventKind::ConnectionRefreshed,
                        connection_name: Some(connection_name),
                    },
                })
                .await;
        }
        Ok(())
    }

    async fn on_relay_event(&self, event: &Event) -> Result<()> {
        if self.active_connections.read().await.is_empty() {
            return Ok(());
        }
        self.handle_event_inner(event).await?;
        Ok(())
    }

    async fn on_resubscribe(&self, maybe_expiry_interval: &mut Option<Interval>) -> Result<()> {
        *maybe_expiry_interval = self
            .ctx
            .persister
            .get_min_interval()
            .map(|interval| tokio::time::interval(Duration::from_secs(interval)));

        if let Some(ref mut interval) = maybe_expiry_interval {
            // First time ticks instantly
            interval.tick().await;
        }
        let mut active_connections = self.active_connections.write().await;
        *active_connections = self.fetch_active_connections().await?;
        Ok(())
    }

    async fn on_destroy(&self) -> Result<()> {
        Ok(())
    }

    fn set_filters(&self, filter: &mut Filter) {
        filter.generic_tags.insert(
            SingleLetterTag {
                character: Alphabet::P,
                uppercase: false,
            },
            BTreeSet::from([self.ctx.our_keys.public_key.to_string()]),
        );
        let kinds = filter.kinds.get_or_insert(BTreeSet::from([]));
        kinds.insert(Kind::WalletConnectRequest);
    }
}
