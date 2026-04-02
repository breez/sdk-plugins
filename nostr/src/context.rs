use std::{collections::HashSet, sync::Arc};

use crate::{
    NostrSdkServices,
    event::{EventManager, NostrEvent, NostrEventDetails},
    handlers::NostrHandlers,
    model::NostrConfig,
    persist::Persister,
};
use anyhow::{Result, bail};
use breez_plugins::PluginStorage;
use log::{debug, warn};
use nostr_sdk::{Client as NostrClient, EventBuilder, Keys};
use tokio::sync::{Mutex, OnceCell, mpsc};
use tokio_with_wasm::alias as tokio;

pub(crate) enum ContextAction {
    Shutdown,
    Resubscribe,
}

pub(crate) struct RuntimeContext {
    pub sdk: Arc<dyn NostrSdkServices>,
    pub client: NostrClient,
    pub our_keys: Keys,
    pub persister: Persister,
    pub event_manager: Arc<EventManager>,
    pub action_trigger: mpsc::Sender<ContextAction>,
    pub replied_event_ids: Mutex<HashSet<String>>,
    pub sdk_listener_id: OnceCell<String>,
}

impl RuntimeContext {
    pub(crate) async fn new(
        sdk: Arc<dyn NostrSdkServices>,
        config: &NostrConfig,
        storage: PluginStorage,
        event_manager: Arc<EventManager>,
        action_tx: mpsc::Sender<ContextAction>,
    ) -> Result<Self> {
        let persister = Persister::new(storage);
        let client = NostrClient::default();
        for relay in config.relays() {
            if let Err(err) = client.add_relay(&relay).await {
                warn!("Could not add relay {relay}: {err:?}");
            }
        }
        let our_keys = Self::get_or_create_keypair(config, &persister).await?;
        let ctx = Self {
            sdk,
            client,
            our_keys,
            persister,
            event_manager,
            action_trigger: action_tx,
            replied_event_ids: Mutex::new(HashSet::new()),
            sdk_listener_id: OnceCell::new(),
        };
        Ok(ctx)
    }

    pub(crate) async fn get_or_create_keypair(
        config: &NostrConfig,
        persister: &Persister,
    ) -> Result<Keys> {
        let get_secret_key = async || -> Result<String> {
            // If we have a key from the configuration, use it
            if let Some(key) = &config.secret_key_hex {
                return Ok(key.clone());
            }

            // Otherwise, try restoring it from the previous session
            if let Ok(Some(key)) = persister.get_seckey() {
                return Ok(key);
            }

            // If none exists, generate a new one
            let key = nostr_sdk::key::SecretKey::generate().to_secret_hex();
            persister.set_seckey(key.clone())?;
            Ok(key)
        };
        let secret_key = get_secret_key().await?;
        Ok(Keys::parse(&secret_key)?)
    }

    pub(crate) async fn trigger_resubscription(&self) {
        let _ = self.action_trigger.send(ContextAction::Resubscribe).await;
    }

    pub async fn clear(&self) {
        self.client.disconnect().await;
        if let Some(listener_id) = self.sdk_listener_id.get() {
            self.sdk
                .remove_event_listener(listener_id.to_string())
                .await;
        }
        self.event_manager
            .notify(NostrEvent {
                event_id: None,
                details: NostrEventDetails::Disconnected,
            })
            .await;
        self.event_manager.pause_notifications();
    }

    pub async fn send_event(&self, event_builder: EventBuilder) -> Result<()> {
        let event = event_builder.sign_with_keys(&self.our_keys)?;
        debug!(
            "Broadcasting Nostr event: {} to {:?}",
            serde_json::to_string(&event)?,
            self.client
                .relays()
                .await
                .keys()
                .map(nostr_sdk::RelayUrl::to_string)
                .collect::<Vec<String>>()
        );
        self.client.send_event(&event).await?;
        Ok(())
    }

    /// Returns true when we have replied to the event, and false otherwise (and inserts it)
    pub async fn check_replied_event(&self, event_id: String) -> bool {
        !self.replied_event_ids.lock().await.insert(event_id)
    }

    pub async fn add_event_listener(&self, handlers: Arc<NostrHandlers>) -> Result<()> {
        if self.sdk_listener_id.initialized() {
            bail!("SDK event listener was already initialized")
        }
        let listener_id = self.sdk.add_event_listener(Box::new(handlers)).await;
        self.sdk_listener_id.set(listener_id)?;
        Ok(())
    }
}
