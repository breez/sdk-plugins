use crate::{
    context::RuntimeContext,
    event::{NostrEvent, NostrEventDetails},
    model::{Payment, PaymentState},
    sdk_services::SdkEventListener,
};
use log::{info, warn};
use nostr_sdk::{EventBuilder, Kind, Tag, TagKind, TagStandard};
use sdk_common::prelude::{parse, InputType};

use super::ZapReceiptsHandler;

pub(crate) struct ZapEventHandler {}

impl ZapEventHandler {
    async fn handle_zap_receipt(&self, ctx: &RuntimeContext, payment: &Payment) {
        let Payment {
            invoice, preimage, ..
        } = payment;

        let Ok(Some(zap_request)) = ctx.persister.get_tracked_zap(invoice) else {
            return;
        };

        info!("Constructing zap receipt for invoice {invoice}");

        let Ok(InputType::Bolt11 { invoice }) = parse(invoice, None).await else {
            warn!("Could not parse bolt11 invoice for tracked zap");
            return;
        };

        let mut eb = EventBuilder::new(Kind::ZapReceipt, "");

        // Verify zap_request
        // https://github.com/nostr-protocol/nips/blob/master/57.md#appendix-e-zap-receipt-event

        // Insert `p` tag
        let Some(p_tag) = zap_request.tags.find(TagKind::p()) else {
            warn!("No `p` tag found for zap request. Aborting receipt.");
            return;
        };
        eb = eb.tag(p_tag.clone());

        // Insert e, a
        for tag_kind in [TagKind::a(), TagKind::e()] {
            if let Some(tag) = zap_request.tags.find(tag_kind) {
                eb = eb.tag(tag.clone());
            }
        }
        // Insert P tag
        eb = eb.tag(
            TagStandard::PublicKey {
                public_key: zap_request.pubkey,
                relay_url: None,
                alias: None,
                uppercase: true,
            }
            .into(),
        );

        // Insert bolt11 tag
        eb = eb.tag(TagStandard::Bolt11(invoice.bolt11.clone()).into());
        // Insert description tag
        let Ok(zap_request_json) = serde_json::to_string(&zap_request) else {
            warn!("Could not encode zap request in JSON");
            return;
        };
        eb = eb.tag(TagStandard::Description(zap_request_json).into());
        // Insert preimage tag
        if let Some(preimage) = preimage {
            eb = eb.tag(Tag::from_standardized(TagStandard::Preimage(
                preimage.clone(),
            )));
        }

        // Send event
        if let Err(err) = ctx.send_event(eb).await {
            warn!("Could not broadcast zap receipt: {err}");
            return;
        }
        info!(
            "Successfully sent zap receipt for invoice {}",
            invoice.bolt11
        );
        if let Err(err) = ctx.persister.remove_tracked_zap(&invoice.bolt11) {
            warn!("Could not remove tracked zap: {err}");
        };
        ctx.event_manager
            .notify(NostrEvent {
                details: NostrEventDetails::ZapReceived {
                    invoice: invoice.bolt11,
                },
                event_id: Some(zap_request.id.to_string()),
            })
            .await;
    }
}

#[sdk_macros::async_trait]
impl SdkEventListener for ZapReceiptsHandler {
    async fn on_sdk_payment(&self, payment: &Payment) {
        match payment.payment_state {
            PaymentState::Pending => {
                self.event_handler
                    .handle_zap_receipt(&self.ctx, payment)
                    .await;
            }
            _ => {}
        }
    }
}
