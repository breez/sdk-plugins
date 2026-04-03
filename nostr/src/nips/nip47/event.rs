#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Clone, Debug, PartialEq)]
pub enum NwcEventKind {
    PayInvoice {
        success: bool,
        preimage: Option<String>,
        fees_sat: Option<u64>,
        error: Option<String>,
    },
    MakeInvoice,
    ListTransactions,
    GetBalance,
    GetInfo,
    ConnectionExpired,
    ConnectionRefreshed,
}
