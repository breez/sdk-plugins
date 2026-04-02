use std::collections::HashMap;

use crate::NostrManager;
use crate::error::NostrResult;
use crate::nips::nip47::model::AddConnectionResponse;

use super::NostrWalletConnectService;
use super::model::{
    AddConnectionRequest, EditConnectionRequest, EditConnectionResponse, NwcConnection,
};

#[sdk_macros::async_trait]
impl NostrWalletConnectService for NostrManager {
    async fn add_connection(
        &self,
        req: AddConnectionRequest,
    ) -> NostrResult<AddConnectionResponse> {
        self.handlers().await?.nwc.add_connection(req).await
    }

    async fn edit_connection(
        &self,
        req: EditConnectionRequest,
    ) -> NostrResult<EditConnectionResponse> {
        self.handlers().await?.nwc.edit_connection(req).await
    }

    async fn list_connections(&self) -> NostrResult<HashMap<String, NwcConnection>> {
        self.handlers().await?.nwc.list_connections().await
    }

    async fn remove_connection(&self, name: String) -> NostrResult<()> {
        self.handlers().await?.nwc.remove_connection(name).await
    }

    async fn handle_event(&self, event_id: String) -> NostrResult<()> {
        self.handlers().await?.nwc.handle_event(event_id).await
    }
}
