use serde::{Deserialize, Serialize};

use crate::DEFAULT_RELAY_URLS;
pub use nostr_sdk::Timestamp;

#[derive(Clone, Serialize, Deserialize)]
pub struct NostrConfig {
    /// A list of default relay urls to add per connection
    pub relay_urls: Option<Vec<String>>,
    /// Custom Nostr secret key (hex-encoded) for the wallet node
    pub secret_key_hex: Option<String>,
    /// Whether or not to start the notification listener event loop. True by default.
    /// Recommended to set to `Some(false)` if you only need event handling
    pub listen_to_events: Option<bool>,
}

impl NostrConfig {
    pub fn relays(&self) -> Vec<String> {
        self.relay_urls
            .clone()
            .unwrap_or(DEFAULT_RELAY_URLS.iter().map(|s| s.to_string()).collect())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct NostrManagerInfo {
    pub wallet_pubkey: String,
    pub connected_relays: Vec<String>,
}

#[derive(PartialEq)]
pub enum PaymentType {
    Incoming,
    Outgoing,
}

#[derive(PartialEq)]
pub enum PaymentState {
    Pending,
    Failed,
    Complete,
}

pub struct Payment {
    pub invoice: String,
    pub amount_sat: u64,
    pub fees_sat: u64,
    pub timestamp: u32,
    pub payment_type: PaymentType,
    pub payment_state: PaymentState,
    pub payment_hash: Option<String>,
    pub preimage: Option<String>,
    pub description: Option<String>,
    pub description_hash: Option<String>,
}

pub struct LightningInvoice {
    pub bolt11: String,
    pub payment_hash: String,
    pub description: Option<String>,
    pub amount_msat: Option<u64>,
}
