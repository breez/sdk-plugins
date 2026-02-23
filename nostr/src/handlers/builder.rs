use std::sync::Arc;

use super::NostrHandlers;
use crate::{context::RuntimeContext, model::NostrConfig};

#[cfg(feature = "nip47")]
use crate::{nips::nip47::NostrWalletConnectHandler, sdk_services::NostrSdkServices};

#[cfg(feature = "nip57")]
use crate::nips::nip57::ZapReceiptsHandler;

pub struct NostrHandlersBuilder {
    config: NostrConfig,
    ctx: Arc<RuntimeContext>,
    #[cfg(feature = "nip47")]
    nwc: Option<NostrWalletConnectHandler>,
    #[cfg(feature = "nip57")]
    zaps: Option<ZapReceiptsHandler>,
}

impl NostrHandlersBuilder {
    pub(crate) fn new(ctx: Arc<RuntimeContext>, config: NostrConfig) -> Self {
        Self {
            ctx,
            config,
            #[cfg(feature = "nip47")]
            nwc: None,
            #[cfg(feature = "nip57")]
            zaps: None,
        }
    }

    #[cfg(feature = "nip47")]
    pub(crate) fn nwc(&mut self, handler: Arc<dyn NostrSdkServices>) {
        let nwc_handler =
            NostrWalletConnectHandler::new(self.ctx.clone(), handler, self.config.clone());
        self.nwc = Some(nwc_handler);
    }

    #[cfg(feature = "nip57")]
    pub(crate) fn zaps(&mut self) {
        self.zaps = Some(ZapReceiptsHandler::new(self.ctx.clone()));
    }

    pub(crate) fn build(self) -> NostrHandlers {
        NostrHandlers {
            ctx: self.ctx,
            #[cfg(feature = "nip47")]
            nwc: self.nwc.unwrap(),
            #[cfg(feature = "nip57")]
            zaps: self.zaps.unwrap(),
        }
    }
}
