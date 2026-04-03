use std::collections::HashMap;

use log::{debug, info};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

#[cfg(feature = "nip47")]
use crate::nips::nip47::event::NwcEventKind;

#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Clone, Debug, PartialEq)]
pub enum NostrEventDetails {
    Connected,
    Disconnected,
    #[cfg(feature = "nip47")]
    Nwc {
        kind: NwcEventKind,
        connection_name: Option<String>,
    },
    #[cfg(feature = "nip57")]
    ZapReceived {
        invoice: String,
    },
}

/// The event emitted when a Nostr operation has been handled
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Clone, Debug, PartialEq)]
pub struct NostrEvent {
    pub event_id: Option<String>,
    pub details: NostrEventDetails,
}

#[cfg_attr(feature = "uniffi", uniffi::export(callback_interface))]
#[sdk_macros::async_trait]
pub trait NostrEventListener: Send + Sync {
    async fn on_event(&self, event: NostrEvent);
}

pub(crate) struct EventManager {
    listeners: RwLock<HashMap<String, Box<dyn NostrEventListener>>>,
    notifier: broadcast::Sender<NostrEvent>,
    is_paused: AtomicBool,
}

impl EventManager {
    pub fn new() -> Self {
        let (notifier, _) = broadcast::channel::<NostrEvent>(100);

        Self {
            listeners: Default::default(),
            notifier,
            is_paused: AtomicBool::new(false),
        }
    }

    pub async fn add(&self, listener: Box<dyn NostrEventListener>) -> String {
        let id = Uuid::new_v4().to_string();
        (*self.listeners.write().await).insert(id.clone(), listener);
        id
    }

    pub async fn remove(&self, id: &str) {
        (*self.listeners.write().await).remove(id);
    }

    pub async fn notify(&self, e: NostrEvent) {
        match self.is_paused.load(Ordering::SeqCst) {
            true => info!("Event notifications are paused, not emitting event {e:?}"),
            false => {
                debug!("Emitting event: {e:?}");
                let _ = self.notifier.send(e.clone());

                for listener in (*self.listeners.read().await).values() {
                    listener.on_event(e.clone()).await;
                }
            }
        }
    }

    pub(crate) fn pause_notifications(&self) {
        info!("Pausing event notifications");
        self.is_paused.store(true, Ordering::SeqCst);
    }

    pub(crate) fn resume_notifications(&self) {
        info!("Resuming event notifications");
        self.is_paused.store(false, Ordering::SeqCst);
    }
}
