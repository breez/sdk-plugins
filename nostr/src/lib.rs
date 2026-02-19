use std::{sync::Arc, time::Duration};

use crate::{
    context::{ContextAction, RuntimeContext},
    error::NostrError,
    event::{EventManager, NostrEvent, NostrEventDetails, NostrEventListener},
    handlers::{builder::NostrHandlersBuilder, routines::HandlerRoutines, NostrHandlers},
    model::{NostrConfig, NostrManagerInfo},
    nips::nip47::handler::RelayMessageHandler,
    sdk_event::SdkEventListener,
};
use breez_sdk_plugins::{Plugin, PluginStorage};
use log::{error, info, warn};
use nostr_sdk::{RelayMessage, RelayPoolNotification};
use tokio::{
    sync::{mpsc, Mutex},
    time::Interval,
};
use tokio_with_wasm::alias as tokio;

pub(crate) mod context;
mod encrypt;
pub mod error;
pub mod event;
pub mod handlers;
pub mod model;
pub mod nips;
mod persist;
pub(crate) mod sdk_event;
pub(crate) mod utils;

pub const DEFAULT_RELAY_URLS: [&str; 4] = [
    "wss://relay.getalbypro.com/breez",
    "wss://nos.lol/",
    "wss://nostr.land/",
    "wss://nostr.wine/",
];

pub struct NostrManager {
    config: NostrConfig,
    event_manager: Arc<EventManager>,
    runtime_ctx: Mutex<Option<Arc<RuntimeContext>>>,
}

impl NostrManager {
    /// Creates a new NostrManager instance.
    ///
    /// Initializes the service with the provided cryptographic keys
    ///
    /// # Arguments
    /// * `config` - Configuration containing the relay URLs and secret key
    ///
    /// # Returns
    /// * `NostrManager` - Successfully initialized service
    pub fn new(config: NostrConfig) -> Self {
        Self {
            config,
            runtime_ctx: Default::default(),
            event_manager: Arc::new(EventManager::new()),
        }
    }

    pub async fn add_event_listener(&self, listener: Box<dyn NostrEventListener>) -> String {
        self.event_manager.add(listener).await
    }

    pub async fn remove_event_listener(&self, id: &str) {
        self.event_manager.remove(id).await
    }

    pub async fn get_info(&self) -> NostrManagerInfo {
        let lock = self.runtime_ctx.lock().await;
        let Some(ref ctx) = *lock else {
            return NostrManagerInfo {
                is_running: false,
                ..Default::default()
            };
        };
        NostrManagerInfo {
            is_running: true,
            wallet_pubkey: Some(ctx.our_keys.public_key().to_hex()),
            connected_relays: Some(self.config.relays()),
        }
    }

    fn build_handlers<SdkServices>(
        &self,
        ctx: &Arc<RuntimeContext>,
        sdk: &Arc<SdkServices>,
    ) -> Arc<NostrHandlers>
    where
        SdkServices: RelayMessageHandler + SdkEventListener + 'static,
    {
        let mut handlers_builder = NostrHandlersBuilder::new(ctx.clone(), self.config.clone());

        handlers_builder.nwc(sdk.clone());
        handlers_builder.zaps();

        Arc::new(handlers_builder.build())
    }

    async fn new_maybe_interval(ctx: &RuntimeContext) -> Option<Interval> {
        ctx.persister
            .get_min_interval()
            .map(|interval| tokio::time::interval(Duration::from_secs(interval)))
    }

    async fn min_refresh_interval(maybe_interval: &mut Option<Interval>) -> Option<()> {
        match maybe_interval {
            Some(interval) => {
                interval.tick().await;
                Some(())
            }
            None => None,
        }
    }
}

#[sdk_macros::async_trait]
impl<SdkServices: RelayMessageHandler + SdkEventListener + 'static> Plugin<SdkServices>
    for NostrManager
{
    fn id(&self) -> String {
        "breez-nostr".to_string()
    }

    async fn on_start(&self, sdk: Arc<SdkServices>, storage: PluginStorage) {
        let mut ctx_lock = self.runtime_ctx.lock().await;
        if ctx_lock.is_some() {
            warn!("Called on_start when service was already running.");
            return;
        }

        let (action_tx, mut action_rx) = mpsc::channel::<ContextAction>(10);
        let ctx =
            match RuntimeContext::new(&self.config, storage, self.event_manager.clone(), action_tx)
                .await
            {
                Ok(ctx) => Arc::new(ctx),
                Err(err) => {
                    error!("Could not create Nostr runtime context: {err:?}");
                    return;
                }
            };
        *ctx_lock = Some(ctx.clone());
        drop(ctx_lock);
        let handlers = self.build_handlers(&ctx, &sdk);

        if let Err(err) = handlers.on_init().await {
            warn!("Could not execute `on_init` routine: {err}");
            return;
        }

        ctx.client.connect().await;
        if let Err(err) = handlers.on_connect().await {
            warn!("Could not execute `on_connect` routine: {err}");
            return;
        };

        if self.config.listen_to_events.is_some_and(|listen| !listen) {
            return;
        }

        let thread_ctx = ctx.clone();
        tokio::spawn(async move {
            thread_ctx
                .event_manager
                .notify(NostrEvent {
                    details: NostrEventDetails::Connected,
                    event_id: None,
                })
                .await;
            info!("Successfully connected Nostr client");

            if let Err(err) = handlers.on_connect().await {
                warn!("Could not execute `on_connect` routine: {err}");
                return;
            }

            let mut maybe_expiry_interval = None;
            loop {
                info!("Subscribing to notifications.");
                if let Err(err) = handlers.on_resubscribe(&mut maybe_expiry_interval).await {
                    warn!("Could not execute `on_resubscribe` routine: {err}");
                    return;
                }

                let mut notifications_listener = thread_ctx.client.notifications();
                loop {
                    tokio::select! {
                        Ok(notification) = notifications_listener.recv() => match notification {
                            RelayPoolNotification::Message { message: RelayMessage::Event { event, .. }, .. } => {
                                    info!("Received event: {event:?}");
                                    let handlers = handlers.clone();
                                    tokio::spawn(async move {
                                        if let Err(err) = handlers.on_relay_event(&event).await {
                                            warn!("Could not handle event {}: {}", event.id, err);
                                        }
                                    });
                            },
                            RelayPoolNotification::Message { message: RelayMessage::EndOfStoredEvents(_), .. } => notifications_listener = notifications_listener.resubscribe(),
                            _ => {},
                        },
                        Some(_) = Self::min_refresh_interval(&mut maybe_expiry_interval) => {
                            info!("Refreshing active connections");
                            if let Err(err) = handlers.on_interval().await {
                                warn!("Could not execute `on_interval` routine: {err}");
                            }
                        }
                        Some(action) = action_rx.recv() => match action {
                            ContextAction::Shutdown => {
                                info!("Shutting down Nostr Manager");
                                if let Err(err) = handlers.on_destroy().await {
                                    warn!("Could not execute `on_destroy` routine: {err}");
                                };
                                ctx.clear().await;
                            },
                            ContextAction::Resubscribe => break,
                        },
                    }
                }
            }
        });
    }

    async fn on_stop(&self) {
        let mut ctx_lock = self.runtime_ctx.lock().await;
        if let Some(ref ctx) = *ctx_lock {
            if let Err(err) = ctx.action_trigger.send(ContextAction::Shutdown).await {
                warn!("Could not send shutdown command: {err}");
                return;
            };
            *ctx_lock = None;
        }
    }
}
