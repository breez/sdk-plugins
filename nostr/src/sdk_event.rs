use crate::model::Payment;

#[sdk_macros::async_trait]
pub trait SdkEventListener: Send + Sync {
    async fn on_payment(payment: Payment);
}
