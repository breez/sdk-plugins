mod storage;

use std::sync::Arc;

pub use storage::*;

#[sdk_macros::async_trait]
pub trait Plugin<SdkServices> {
    fn id(&self) -> String;
    async fn on_start(&self, plugin_sdk: Arc<SdkServices>, storage: PluginStorage);
    async fn on_stop(&self);
}
