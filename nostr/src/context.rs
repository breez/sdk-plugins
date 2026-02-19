use std::{collections::HashSet, sync::Arc};

use crate::{
    event::{EventManager, NostrEvent, NostrEventDetails},
    model::NostrConfig,
    persist::Persister,
};
use anyhow::Result;
use breez_sdk_plugins::PluginStorage;
use log::{info, warn};
use nostr_sdk::{Client as NostrClient, EventBuilder, Keys};
use tokio::sync::{mpsc, Mutex, OnceCell};
use tokio::task::JoinHandle;
use tokio_with_wasm::alias as tokio;

pub(crate) struct RuntimeContext {
    pub client: NostrClient,
    pub our_keys: Keys,
    pub persister: Persister,
    pub event_manager: Arc<EventManager>,
    pub resubscription_trigger: mpsc::Sender<()>,
    pub event_loop_handle: OnceCell<JoinHandle<()>>,
    pub replied_event_ids: Mutex<HashSet<String>>,
}

impl RuntimeContext {
    pub(crate) async fn new(
        config: &NostrConfig,
        storage: PluginStorage,
        event_manager: Arc<EventManager>,
        resub_tx: mpsc::Sender<()>,
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
            client,
            our_keys,
            persister,
            resubscription_trigger: resub_tx,
            event_loop_handle: OnceCell::new(),
            event_manager,
            replied_event_ids: Mutex::new(HashSet::new()),
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
        let _ = self.resubscription_trigger.send(()).await;
    }

    pub async fn clear(&self) {
        if let Some(handle) = self.event_loop_handle.get() {
            handle.abort();
        }
        self.client.disconnect().await;
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
        info!("Broadcasting Nostr event: {event:?}");
        self.client.send_event(&event).await?;
        Ok(())
    }

    /// Returns true when we have replied to the event, and false otherwise (and inserts it)
    pub async fn check_replied_event(&self, event_id: String) -> bool {
        !self.replied_event_ids.lock().await.insert(event_id)
    }
}
