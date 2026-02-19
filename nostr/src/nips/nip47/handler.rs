use nostr_sdk::nips::nip47::{
    GetBalanceResponse, GetInfoResponse, ListTransactionsRequest, LookupInvoiceResponse,
    MakeInvoiceRequest, MakeInvoiceResponse, PayInvoiceRequest, PayInvoiceResponse,
};

use crate::error::NostrResult;

#[sdk_macros::async_trait]
pub trait RelayMessageHandler: Send + Sync {
    fn supported_methods(&self) -> &[&'static str];
    async fn make_invoice(&self, req: &MakeInvoiceRequest) -> NostrResult<MakeInvoiceResponse>;
    async fn pay_invoice(&self, req: &PayInvoiceRequest) -> NostrResult<PayInvoiceResponse>;
    async fn list_transactions(
        &self,
        req: &ListTransactionsRequest,
    ) -> NostrResult<Vec<LookupInvoiceResponse>>;
    async fn get_balance(&self) -> NostrResult<GetBalanceResponse>;
    async fn get_info(&self) -> NostrResult<GetInfoResponse>;
}
