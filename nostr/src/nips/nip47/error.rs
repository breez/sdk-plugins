use nostr_sdk::nips::nip47::{ErrorCode, NIP47Error};

use crate::error::NostrError;

#[allow(clippy::from_over_into)]
impl Into<NIP47Error> for NostrError {
    fn into(self) -> NIP47Error {
        let code = match &self {
            Self::PubkeyNotFound { .. } | Self::EventNotFound => ErrorCode::NotFound,
            Self::MaxBudgetExceeded => ErrorCode::QuotaExceeded,
            _ => ErrorCode::PaymentFailed,
        };
        NIP47Error {
            code,
            message: self.to_string(),
        }
    }
}
