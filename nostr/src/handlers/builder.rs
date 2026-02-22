use std::sync::Arc;

use super::NostrHandlers;
use crate::{
    context::RuntimeContext,
    model::NostrConfig,
    nips::{
        nip47::{handler::RelayMessageHandler, NostrWalletConnectHandler},
        nip57::ZapReceiptsHandler,
    },
};

pub struct NostrHandlersBuilder {
    config: NostrConfig,
    ctx: Arc<RuntimeContext>,
    nwc: Option<NostrWalletConnectHandler>,
    zaps: Option<ZapReceiptsHandler>,
}

impl NostrHandlersBuilder {
    pub(crate) fn new(ctx: Arc<RuntimeContext>, config: NostrConfig) -> Self {
        Self {
            ctx,
            config,
            nwc: None,
            zaps: None,
        }
    }

    pub(crate) fn nwc(&mut self, handler: Arc<dyn RelayMessageHandler>) {
        let nwc_handler =
            NostrWalletConnectHandler::new(self.ctx.clone(), handler, self.config.clone());
        self.nwc = Some(nwc_handler);
    }

    pub(crate) fn zaps(&mut self) {
        self.zaps = Some(ZapReceiptsHandler::new(self.ctx.clone()));
    }

    pub(crate) fn build(self) -> NostrHandlers {
        NostrHandlers {
            ctx: self.ctx,
            nwc: self.nwc.unwrap(),
            zaps: self.zaps.unwrap(),
        }
    }
}
