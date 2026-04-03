mod storage;

use std::sync::Arc;

pub use storage::*;

#[sdk_macros::async_trait]
pub trait Plugin<Sdk>: Send + Sync {
    async fn attach(&self, sdk: Arc<Sdk>, storage: PluginStorage);
}
