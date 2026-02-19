use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct TrackedZap {
    pub zap_request: String,
    pub expires_at: u32,
}
