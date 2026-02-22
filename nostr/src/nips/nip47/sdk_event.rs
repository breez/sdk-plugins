use std::sync::Arc;

use super::ActiveConnections;
use log::{info, warn};
use nostr_sdk::{
    nips::nip47::{
        Notification, NotificationResult, NotificationType, PaymentNotification, TransactionType,
    },
    EventBuilder, Keys, Kind, Tag, TagKind, TagStandard, Timestamp,
};
use sdk_common::prelude::{parse, InputType};
use tokio::sync::RwLock;

use crate::{
    context::RuntimeContext,
    encrypt::EncryptionHandler,
    event::{NostrEvent, NostrEventDetails},
    model::{Payment, PaymentState, PaymentType},
    nips::nip47::NostrWalletConnectHandler,
    sdk_event::SdkEventListener,
};

pub(crate) struct NwcEventHandler {
    pub ctx: Arc<RuntimeContext>,
    pub active_connections: Arc<RwLock<ActiveConnections>>,
}

enum NotificationKind {
    NIP04 = 23196,
    NIP44 = 23197,
}

impl NwcEventHandler {
    pub(crate) async fn handle_notif_to_relay(&self, payment: &Payment) {
        let Payment {
            invoice,
            description,
            preimage,
            payment_hash,
            amount_sat,
            fees_sat,
            timestamp,
            description_hash,
            ..
        } = payment;

        let payment_notification = PaymentNotification {
            transaction_type: Some(if payment.payment_type == PaymentType::Outgoing {
                TransactionType::Outgoing
            } else {
                TransactionType::Incoming
            }),
            invoice: invoice.clone(),
            description: description.clone(),
            description_hash: description_hash.clone(),
            amount: amount_sat * 1000,
            fees_paid: fees_sat * 1000,
            created_at: Timestamp::from_secs(*timestamp as u64),
            settled_at: Timestamp::from_secs(*timestamp as u64),
            preimage: preimage.clone().unwrap_or_default(),
            payment_hash: payment_hash.clone().unwrap_or_default(),
            expires_at: None,
            metadata: None,
        };

        let notification = if payment.payment_type == PaymentType::Outgoing {
            Notification {
                notification_type: NotificationType::PaymentSent,
                notification: NotificationResult::PaymentSent(payment_notification),
            }
        } else {
            Notification {
                notification_type: NotificationType::PaymentReceived,
                notification: NotificationResult::PaymentReceived(payment_notification),
            }
        };

        let notification_content = match serde_json::to_string(&notification) {
            Ok(content) => content,
            Err(e) => {
                warn!("Could not serialize notification: {e:?}");
                return;
            }
        };

        for (_, con) in self.active_connections.read().await.iter() {
            let nwc_client_keypair = Keys::new(con.uri.secret.clone());
            let encryption_handler = EncryptionHandler::new(
                self.ctx.our_keys.secret_key(),
                &nwc_client_keypair.public_key,
            );

            for kind in [NotificationKind::NIP04, NotificationKind::NIP44] {
                let enc = match kind {
                    NotificationKind::NIP04 => EncryptionHandler::nip04_encrypt,
                    NotificationKind::NIP44 => EncryptionHandler::nip44_encrypt,
                };
                let encrypted_content = match enc(&encryption_handler, &notification_content) {
                    Ok(encrypted) => encrypted,
                    Err(e) => {
                        warn!("Could not encrypt notification content: {e:?}");
                        continue;
                    }
                };

                let event_builder = EventBuilder::new(Kind::Custom(kind as u16), encrypted_content)
                    .tags([Tag::public_key(con.uri.public_key)]);

                if let Err(e) = self.ctx.send_event(event_builder).await {
                    warn!("Could not send notification event to relay: {e:?}");
                } else {
                    info!("Sent payment notification to relay");
                }
            }
        }
    }

    pub(crate) async fn handle_zap_receipt(&self, payment: &Payment) {
        let Payment {
            invoice, preimage, ..
        } = payment;

        let Ok(Some(zap_request)) = self.ctx.persister.remove_tracked_zap(invoice) else {
            return;
        };

        info!("Constructing zap receipt for invoice {invoice}");

        let Ok(InputType::Bolt11 { invoice }) = parse(invoice, None).await else {
            warn!("Could not parse bolt11 invoice for tracked zap");
            return;
        };

        let mut eb = EventBuilder::new(Kind::ZapReceipt, "")
            .custom_created_at(Timestamp::from_secs(invoice.timestamp));

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
        if let Err(err) = self.ctx.send_event(eb).await {
            warn!("Coult not broadcast zap receipt: {err}");
            return;
        }
        info!(
            "Successfully sent zap receipt for invoice {}",
            invoice.bolt11
        );
        self.ctx
            .event_manager
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
impl SdkEventListener for NostrWalletConnectHandler {
    async fn on_sdk_payment(&self, payment: &Payment) {
        match payment.payment_state {
            PaymentState::Pending => {
                self.event_handler.handle_zap_receipt(payment).await;
            }
            PaymentState::Complete => {
                self.event_handler.handle_notif_to_relay(payment).await;
            }
            _ => {}
        }
    }
}
