mod error;
pub mod event;
mod forward;
pub mod model;
mod persist;
pub(crate) mod routines;
mod sdk_services;

use log::{debug, info};
use model::{
    ActiveConnection, AddConnectionRequest, AddConnectionResponse, EditConnectionRequest,
    EditConnectionResponse, NwcConnection, NwcConnectionInner, PeriodicBudgetInner,
};
use std::{collections::HashMap, str::FromStr as _, sync::Arc};
use tokio::sync::RwLock;

use crate::{
    context::RuntimeContext,
    encrypt::EncryptionHandler,
    error::{NostrError, NostrResult},
    event::{NostrEvent, NostrEventDetails},
    model::{LightningInvoice, NostrConfig},
    sdk_services::NostrSdkServices,
    utils,
};
use event::NwcEventKind;
use sdk_services::NwcEventHandler;

use nostr_sdk::{
    Event, EventBuilder, Keys, Kind, RelayUrl, Tag, Timestamp,
    nips::nip47::{NostrWalletConnectURI, Request, RequestParams, Response, ResponseResult},
};

pub const MIN_REFRESH_INTERVAL_SEC: u64 = 60; // 1 minute
pub const DEFAULT_PERIODIC_BUDGET_TIME_SEC: u32 = 60 * 60 * 24 * 30; // 30 days
pub const DEFAULT_EVENT_HANDLING_INTERVAL_SEC: u64 = 10;
pub const NWC_SUPPORTED_METHODS: [&str; 5] = [
    "make_invoice",
    "pay_invoice",
    "list_transactions",
    "get_balance",
    "get_info",
];
pub const NWC_SUPPORTED_NOTIFICATIONS: [&str; 2] = ["payment_received", "payment_sent"];

pub(crate) type ActiveConnections = HashMap<String, ActiveConnection>;

#[sdk_macros::async_trait]
pub trait NostrWalletConnectService: Send + Sync {
    /// Creates a Nostr Wallet Connect connection for this service.
    ///
    /// Generates a unique connection URI that external applications can use
    /// to connect to this wallet service. The URI includes the wallet's public key,
    /// relay information, and a randomly generated secret for secure communication.
    ///
    /// # Arguments
    /// * `req` - The [add connection request](AddConnectionRequest), including:
    ///     * `name` - the **unique** identifier of the connection
    ///     * `expiry_time_min` - the expiry time of the connection string. If None, it will **not**
    ///     expire
    ///     * `periodic_budget_req` - the periodic budget paremeters of the connection if any.
    ///     You can specify the [maximum amount \(in satoshi\) per period](crate::model::PeriodicBudgetRequest::max_budget_sat)
    ///     and the [period renewal time \(in minutes\)](crate::model::PeriodicBudgetRequest::renewal_time_mins)
    ///
    /// # Returns
    /// * `res` - The [AddConnectionResponse], including:
    ///     * `connection` - the generated NWC connection
    async fn add_connection(&self, req: AddConnectionRequest)
    -> NostrResult<AddConnectionResponse>;

    /// Modifies a Nostr Wallet Connect connection for this service.
    ///
    /// # Arguments
    /// * `req` - The [edit connection request](EditConnectionRequest), including:
    ///     * `name` - the already existing identifier of the connection
    ///     * `expiry_time_min` - the expiry time of the connection string. If None, it will **not**
    ///     expire
    ///     * `periodic_budget_req` - the periodic budget paremeters of the connection if any.
    ///     You can specify the [maximum amount \(in satoshi\) per period](crate::model::PeriodicBudgetRequest::max_budget_sat)
    ///     and the [period renewal time \(in minutes\)](crate::model::PeriodicBudgetRequest::renewal_time_mins)
    ///
    /// # Returns
    /// * `res` - The [EditConnectionResponse], including:
    ///     * `connection` - the modified NWC connection
    async fn edit_connection(
        &self,
        req: EditConnectionRequest,
    ) -> NostrResult<EditConnectionResponse>;

    /// Lists the active Nostr Wallet Connect connections for this service.
    async fn list_connections(&self) -> NostrResult<HashMap<String, NwcConnection>>;

    /// Removes a Nostr Wallet Connect connection string
    ///
    /// Removes a previously set connection string. Returns error if unset.
    ///
    /// # Arguments
    /// * `name` - The unique identifier for the connection string
    async fn remove_connection(&self, name: String) -> NostrResult<()>;

    /// Handles a Nostr WalletRequest event
    ///
    /// # Arguments
    /// * `raw_event` - the raw, JSON-serialized Nostr event
    async fn handle_event(&self, raw_event: String) -> NostrResult<()>;
}

pub(crate) struct NostrWalletConnectHandler {
    pub config: NostrConfig,
    pub ctx: Arc<RuntimeContext>,
    pub message_handler: Arc<dyn NostrSdkServices>,
    pub event_handler: NwcEventHandler,
    pub active_connections: Arc<RwLock<ActiveConnections>>,
}

#[sdk_macros::async_trait]
impl NostrWalletConnectService for NostrWalletConnectHandler {
    async fn add_connection(
        &self,
        req: AddConnectionRequest,
    ) -> NostrResult<AddConnectionResponse> {
        let random_secret_key = nostr_sdk::SecretKey::generate();
        let relays = self
            .config
            .relays()
            .into_iter()
            .filter_map(|r| RelayUrl::from_str(&r).ok())
            .collect();

        let now = utils::now();
        let connection = NwcConnectionInner {
            connection_string: NostrWalletConnectURI::new(
                self.ctx.our_keys.public_key,
                relays,
                random_secret_key,
                None,
            )
            .to_string(),
            created_at: now,
            expiry_time_sec: req.expiry_time_mins.map(utils::mins_to_seconds),
            receive_only: req.receive_only.unwrap_or(false),
            paid_amount_sat: 0,
            periodic_budget: req
                .periodic_budget_req
                .map(|req| PeriodicBudgetInner::from_budget_request(req, now)),
        };
        self.ctx
            .persister
            .add_nwc_connection(req.name.clone(), connection.clone())
            .await?;
        self.ctx.trigger_resubscription().await;
        Ok(AddConnectionResponse {
            connection: connection.into(),
        })
    }

    async fn edit_connection(
        &self,
        req: EditConnectionRequest,
    ) -> NostrResult<EditConnectionResponse> {
        let connection = self.ctx.persister.edit_nwc_connection(req).await?;
        self.ctx.trigger_resubscription().await;
        Ok(EditConnectionResponse {
            connection: connection.into(),
        })
    }

    async fn list_connections(&self) -> NostrResult<HashMap<String, NwcConnection>> {
        let connections = self.ctx.persister.list_nwc_connections().await?;
        Ok(connections
            .into_iter()
            .map(|(k, v)| (k, v.into()))
            .collect())
    }

    async fn remove_connection(&self, name: String) -> NostrResult<()> {
        self.ctx.persister.remove_nwc_connection(name).await?;
        self.ctx.trigger_resubscription().await;
        Ok(())
    }

    async fn handle_event(&self, raw_event: String) -> NostrResult<()> {
        self.ctx.client.connect().await;
        let event = serde_json::from_str(&raw_event)?;
        self.handle_event_inner(&event).await?;
        Ok(())
    }
}

impl NostrWalletConnectHandler {
    pub fn new(
        ctx: Arc<RuntimeContext>,
        message_handler: Arc<dyn NostrSdkServices>,
        config: NostrConfig,
    ) -> Self {
        let active_connections: Arc<RwLock<ActiveConnections>> = Default::default();
        let event_handler = NwcEventHandler {
            active_connections: active_connections.clone(),
        };
        Self {
            config,
            ctx,
            message_handler,
            event_handler,
            active_connections,
        }
    }

    pub async fn fetch_active_connections(&self) -> NostrResult<ActiveConnections> {
        Ok(self
            .ctx
            .persister
            .list_nwc_connections()
            .await?
            .into_iter()
            .filter_map(|(name, connection)| {
                NostrWalletConnectURI::from_str(&connection.connection_string)
                    .map(|uri| {
                        (
                            name,
                            ActiveConnection {
                                pubkey: Keys::new(uri.secret.clone()).public_key,
                                uri,
                                connection,
                            },
                        )
                    })
                    .ok()
            })
            .collect())
    }

    pub async fn send_info_event(&self) -> NostrResult<()> {
        let content = NWC_SUPPORTED_METHODS.join(" ").to_string();
        self.ctx
            .send_event(
                EventBuilder::new(Kind::WalletConnectInfo, content).tag(Tag::custom(
                    "encryption".into(),
                    ["nip44_v2 nip04".to_string()],
                )),
            )
            .await?;
        Ok(())
    }

    async fn handle_event_inner(&self, event: &Event) -> NostrResult<()> {
        let event_id = event.id.to_string();
        let client_pubkey = event.pubkey;

        let (connection_name, mut client) = self
            .active_connections
            .read()
            .await
            .iter()
            .find(|(_, con)| con.pubkey == client_pubkey)
            .map(|(name, con)| (name.clone(), con.clone()))
            .ok_or(NostrError::PubkeyNotFound {
                pubkey: client_pubkey.to_string(),
            })?;

        // Verify the event has not expired
        if event
            .tags
            .expiration()
            .is_some_and(|t| *t < Timestamp::now())
        {
            return Err(NostrError::EventExpired);
        }

        // Verify the event signature and event id
        event.verify().map_err(|err| NostrError::InvalidSignature {
            err: err.to_string(),
        })?;

        // Decrypt the event content
        let encryption_handler =
            EncryptionHandler::new(self.ctx.our_keys.secret_key(), &client_pubkey);
        let decrypted_content = encryption_handler.decrypt(event)?;
        info!("Decrypted NWC notification");

        // Build response
        let req = serde_json::from_str::<Request>(&decrypted_content)?;
        let mut compute_result = async || -> NostrResult<ResponseResult> {
            if client.connection.receive_only
                && !matches!(req.params, RequestParams::MakeInvoice(_))
            {
                return Err(NostrError::generic("Connection is receive-only."));
            }

            match &req.params {
                RequestParams::PayInvoice(req) => {
                    let Ok(LightningInvoice {
                        bolt11,
                        amount_msat,
                        ..
                    }) = self.ctx.sdk.parse_invoice(&req.invoice).await
                    else {
                        return Err(NostrError::generic(format!(
                            "Could not parse pay_invoice invoice: {}",
                            req.invoice
                        )));
                    };
                    let Some(req_amount_sat) = req
                        .amount
                        .or(amount_msat)
                        .map(|amount| amount.div_ceil(1000))
                    else {
                        return Err(NostrError::InvoiceWithoutAmount);
                    };

                    if let Some(ref mut periodic_budget) = client.connection.periodic_budget {
                        if periodic_budget.used_budget_sat + req_amount_sat
                            > periodic_budget.max_budget_sat
                        {
                            return Err(NostrError::MaxBudgetExceeded);
                        }
                        // We modify the connection's budget before executing the payment to avoid any race
                        // conditions
                        if let Err(err) = self
                            .ctx
                            .persister
                            .update_budget(&connection_name, req_amount_sat as i64)
                            .await
                        {
                            return Err(NostrError::generic(format!(
                                "Cannot pay invoice: could not update periodic budget on connection \"{connection_name}\": {err}"
                            )));
                        }
                    }
                    match self.message_handler.pay_invoice(req).await {
                        Ok(res) => {
                            self.ctx
                                .persister
                                .add_nwc_paid_invoice(&connection_name, bolt11)
                                .await
                                .map_err(|err| {
                                    NostrError::persist(format!(
                                        "Could not persist paid invoice: {err}"
                                    ))
                                })?;
                            Ok(ResponseResult::PayInvoice(res))
                        }
                        Err(e) => {
                            // In case of payment failure, we want to undo the periodic budget changes
                            if client.connection.periodic_budget.is_some()
                                && let Err(err) = self
                                    .ctx
                                    .persister
                                    .update_budget(&connection_name, -(req_amount_sat as i64))
                                    .await
                            {
                                return Err(NostrError::generic(format!(
                                    "Cannot pay invoice: could not update periodic budget on connection \"{connection_name}\": {err}."
                                )));
                            }
                            Err(e)
                        }
                    }
                }
                RequestParams::MakeInvoice(req) => self
                    .message_handler
                    .make_invoice(req)
                    .await
                    .map(ResponseResult::MakeInvoice),
                RequestParams::ListTransactions(req) => self
                    .message_handler
                    .list_transactions(req)
                    .await
                    .map(ResponseResult::ListTransactions),
                RequestParams::GetBalance => self
                    .message_handler
                    .get_balance()
                    .await
                    .map(ResponseResult::GetBalance),
                RequestParams::GetInfo => {
                    let mut res = self.message_handler.get_info().await;
                    if let Ok(res) = &mut res {
                        res.methods = NWC_SUPPORTED_METHODS.map(String::from).to_vec();
                        res.notifications = NWC_SUPPORTED_NOTIFICATIONS.map(String::from).to_vec();
                    }
                    res.map(ResponseResult::GetInfo)
                }
                _ => Err(NostrError::generic(format!(
                    "Received unhandled request: {req:?}"
                ))),
            }
        };
        let res = compute_result().await;
        debug!("Got result {res:?} for event {event_id}");

        // Notify SDK
        self.forward_nwc_to_sdk(connection_name.to_string(), &res, &event_id)
            .await;

        // Serialize and encrypt the response
        let content = serde_json::to_string(&Response {
            result_type: req.method,
            result: res.as_ref().ok().cloned(),
            error: res.as_ref().err().cloned().map(Into::into),
        })
        .map_err(|err| {
            NostrError::generic(format!("Could not serialize Nostr response: {err:?}"))
        })?;

        let encrypted_content = encryption_handler.encrypt(event, &content)?;
        info!("Encrypted NWC response");
        let event_builder = EventBuilder::new(Kind::WalletConnectResponse, encrypted_content)
            .tags([Tag::event(event.id), Tag::public_key(client_pubkey)]);

        // Broadcast the response
        self.ctx
            .send_event(event_builder)
            .await
            .map_err(|err| NostrError::Network {
                err: err.to_string(),
            })?;
        info!("Sent encrypted NWC response");

        Ok(())
    }

    async fn forward_nwc_to_sdk(
        &self,
        connection_name: String,
        result: &NostrResult<ResponseResult>,
        event_id: &str,
    ) {
        debug!("Handling notification: {result:?}");
        let kind = match result {
            Ok(ResponseResult::PayInvoice(response)) => NwcEventKind::PayInvoice {
                success: true,
                preimage: Some(response.preimage.clone()),
                fees_sat: response.fees_paid.map(|f| f / 1000),
                error: None,
            },
            Err(
                err @ NostrError::InvoiceExpired
                | err @ NostrError::InvoiceWithoutAmount
                | err @ NostrError::MaxBudgetExceeded,
            ) => NwcEventKind::PayInvoice {
                success: false,
                preimage: None,
                fees_sat: None,
                error: Some(err.to_string()),
            },
            Ok(ResponseResult::MakeInvoice(_)) => NwcEventKind::MakeInvoice,
            Ok(ResponseResult::ListTransactions(_)) => NwcEventKind::ListTransactions,
            Ok(ResponseResult::GetBalance(_)) => NwcEventKind::ListTransactions,
            Ok(ResponseResult::GetInfo(_)) => NwcEventKind::GetInfo,
            _ => {
                return;
            }
        };
        let event = NostrEvent {
            details: NostrEventDetails::Nwc {
                kind,
                connection_name: Some(connection_name),
            },
            event_id: Some(event_id.to_string()),
        };
        info!("Sending event: {event:?}");
        self.ctx.event_manager.notify(event).await;
    }
}
