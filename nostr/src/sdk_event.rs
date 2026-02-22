use crate::model::Payment;

#[sdk_macros::async_trait]
pub trait SdkEventListener {
    async fn on_sdk_payment(&self, payment: &Payment);
}

#[sdk_macros::async_trait]
pub trait EventEmitter: Send + Sync {
    async fn add_event_listener(&self, listener: Box<dyn SdkEventListener>) -> String;
    async fn remove_event_listener(&self, listener_id: String);
}
