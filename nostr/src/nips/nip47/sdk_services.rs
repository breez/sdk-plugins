use std::sync::Arc;

use super::ActiveConnections;
use log::{info, warn};
use nostr_sdk::{
    EventBuilder, Keys, Kind, Tag, Timestamp,
    nips::nip47::{
        Notification, NotificationResult, NotificationType, PaymentNotification, TransactionType,
    },
};
use tokio::sync::RwLock;

use crate::{
    context::RuntimeContext,
    encrypt::EncryptionHandler,
    model::{Payment, PaymentState, PaymentType},
    nips::nip47::NostrWalletConnectHandler,
    sdk_services::{NotificationKind, SdkEventListener},
};

pub(crate) struct NwcEventHandler {
    pub active_connections: Arc<RwLock<ActiveConnections>>,
}

impl NwcEventHandler {
    pub(crate) async fn handle_notif_to_relay(&self, ctx: &RuntimeContext, payment: &Payment) {
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
            let encryption_handler =
                EncryptionHandler::new(ctx.our_keys.secret_key(), &nwc_client_keypair.public_key);

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
                    .tags([Tag::public_key(nwc_client_keypair.public_key)]);

                if let Err(e) = ctx.send_event(event_builder).await {
                    warn!("Could not send notification event to relay: {e:?}");
                } else {
                    info!("Sent payment notification to relay");
                }
            }
        }
    }
}

#[sdk_macros::async_trait]
impl SdkEventListener for NostrWalletConnectHandler {
    async fn on_sdk_payment(&self, payment: &Payment) {
        if payment.payment_state == PaymentState::Complete {
            self.event_handler
                .handle_notif_to_relay(&self.ctx, payment)
                .await;
        }
    }
}
