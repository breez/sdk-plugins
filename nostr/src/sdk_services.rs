use crate::model::LightningInvoice;
#[cfg(feature = "nip47")]
use {
    crate::error::NostrResult,
    nostr_sdk::nips::nip47::{
        GetBalanceResponse, GetInfoResponse, ListTransactionsRequest, LookupInvoiceResponse,
        MakeInvoiceRequest, MakeInvoiceResponse, PayInvoiceRequest, PayInvoiceResponse,
    },
};

pub(crate) enum NotificationKind {
    NIP04 = 23196,
    NIP44 = 23197,
}

#[sdk_macros::async_trait]
pub trait SdkEventListener: Send + Sync {
    async fn on_sdk_payment(&self, payment: &crate::model::Payment);
}

#[sdk_macros::async_trait]
pub trait NostrSdkServices: Send + Sync {
    #[cfg(feature = "nip47")]
    fn supported_methods(&self) -> &[&'static str];

    #[cfg(feature = "nip47")]
    async fn make_invoice(&self, req: &MakeInvoiceRequest) -> NostrResult<MakeInvoiceResponse>;

    #[cfg(feature = "nip47")]
    async fn pay_invoice(&self, req: &PayInvoiceRequest) -> NostrResult<PayInvoiceResponse>;

    #[cfg(feature = "nip47")]
    async fn list_transactions(
        &self,
        req: &ListTransactionsRequest,
    ) -> NostrResult<Vec<LookupInvoiceResponse>>;

    #[cfg(feature = "nip47")]
    async fn get_balance(&self) -> NostrResult<GetBalanceResponse>;

    #[cfg(feature = "nip47")]
    async fn get_info(&self) -> NostrResult<GetInfoResponse>;

    #[cfg(feature = "nip47")]
    #[cfg(feature = "nip57")]
    async fn parse_invoice(&self, invoice: &str) -> NostrResult<LightningInvoice>;

    async fn add_event_listener(&self, listener: Box<dyn SdkEventListener>) -> String;

    async fn remove_event_listener(&self, listener_id: String);
}
