use std::{sync::Arc, time::Duration};

use crate::{
    context::{ContextAction, RuntimeContext},
    error::{NostrError, NostrResult},
    event::{EventManager, NostrEventListener},
    handlers::{builder::NostrHandlersBuilder, routines::HandlerRoutines, NostrHandlers},
    model::{NostrConfig, NostrManagerInfo},
};
use breez_plugins::{Plugin, PluginStorage};
use log::{error, info, warn};
use nostr_sdk::{RelayMessage, RelayPoolNotification};
pub use sdk_services::NostrSdkServices;
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
pub mod sdk_services;
pub(crate) mod utils;

pub const DEFAULT_RELAY_URLS: [&str; 4] = [
    "wss://relay.getalbypro.com/breez",
    "wss://nos.lol/",
    "wss://nostr.land/",
    "wss://nostr.wine/",
];

struct Runtime {
    ctx: Arc<RuntimeContext>,
    handlers: Arc<NostrHandlers>,
}

pub struct NostrManager {
    config: NostrConfig,
    event_manager: Arc<EventManager>,
    runtime: Mutex<Option<Runtime>>,
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
            runtime: Default::default(),
            event_manager: Arc::new(EventManager::new()),
        }
    }

    pub async fn add_event_listener(&self, listener: Box<dyn NostrEventListener>) -> String {
        self.event_manager.add(listener).await
    }

    pub async fn remove_event_listener(&self, id: &str) {
        self.event_manager.remove(id).await
    }

    pub async fn get_info(&self) -> Option<NostrManagerInfo> {
        let lock = self.runtime.lock().await;
        let runtime = (*lock).as_ref()?;
        Some(NostrManagerInfo {
            wallet_pubkey: runtime.ctx.our_keys.public_key().to_hex(),
            connected_relays: self.config.relays(),
        })
    }
}

impl NostrManager {
    #[allow(unused, unused_mut)]
    fn build_handlers(
        &self,
        ctx: Arc<RuntimeContext>,
        sdk: Arc<dyn NostrSdkServices>,
    ) -> Arc<NostrHandlers> {
        let mut handlers_builder = NostrHandlersBuilder::new(ctx, self.config.clone());

        #[cfg(feature = "nip47")]
        handlers_builder.nwc(sdk);

        #[cfg(feature = "nip57")]
        handlers_builder.zaps();

        Arc::new(handlers_builder.build())
    }

    pub(crate) async fn handlers(&self) -> NostrResult<Arc<NostrHandlers>> {
        for _ in 0..3 {
            match *self.runtime.lock().await {
                Some(ref runtime) => return Ok(runtime.handlers.clone()),
                None => tokio::time::sleep(Duration::from_millis(500)).await,
            };
        }
        Err(NostrError::generic("Nostr manager is not running."))
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
impl<SdkServices: NostrSdkServices + 'static> Plugin<SdkServices> for NostrManager {
    fn id(&self) -> String {
        "breez-nostr".to_string()
    }

    async fn on_start(&self, sdk: Arc<SdkServices>, storage: PluginStorage) {
        let mut runtime_lock = self.runtime.lock().await;
        if runtime_lock.is_some() {
            warn!("Called on_start when service was already running.");
            return;
        }

        let (action_tx, mut action_rx) = mpsc::channel::<ContextAction>(10);
        let ctx = match RuntimeContext::new(
            sdk.clone(),
            &self.config,
            storage,
            self.event_manager.clone(),
            action_tx,
        )
        .await
        {
            Ok(ctx) => Arc::new(ctx),
            Err(err) => {
                error!("Could not create Nostr runtime context: {err:?}");
                return;
            }
        };
        let handlers = self.build_handlers(ctx.clone(), sdk.clone());
        *runtime_lock = Some(Runtime {
            ctx: ctx.clone(),
            handlers: handlers.clone(),
        });
        drop(runtime_lock);

        if let Err(err) = handlers.on_init().await {
            warn!("Could not execute `on_init` routine: {err}");
            return;
        }

        if let Err(err) = ctx.add_event_listener(handlers.clone()).await {
            warn!("Could not add SDK event listener: {err}");
        };

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
                            ContextAction::Shutdown => return,
                            ContextAction::Resubscribe => break,
                        },
                    }
                }
            }
        });
    }

    async fn on_stop(&self) {
        info!("Shutting down Nostr Manager");
        let mut runtime_lock = self.runtime.lock().await;
        if let Some(ref runtime) = *runtime_lock {
            if let Err(err) = runtime
                .ctx
                .action_trigger
                .send(ContextAction::Shutdown)
                .await
            {
                warn!("Could not send shutdown command: {err}");
                return;
            };
            runtime.ctx.clear().await;
            if let Err(err) = runtime.handlers.on_destroy().await {
                warn!("Could not execute `on_destroy` routine: {err}");
            };
            *runtime_lock = None;
        }
    }
}
