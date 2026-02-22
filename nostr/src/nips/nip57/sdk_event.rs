use crate::{model::Payment, sdk_event::SdkEventListener};

use super::ZapReceiptsHandler;

#[sdk_macros::async_trait]
impl SdkEventListener for ZapReceiptsHandler {
    async fn on_sdk_payment(&self, _payment: &Payment) {}
}
