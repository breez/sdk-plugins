use crate::persist::Persister;

use std::collections::{BTreeMap, BTreeSet};

use super::{
    model::{EditConnectionRequest, NwcConnectionInner, PeriodicBudgetInner, RefreshResult},
    MIN_REFRESH_INTERVAL_SEC,
};
use crate::{
    error::{NostrError, NostrResult},
    utils,
};

const REFRESH_INTERVAL_GRACE_PERIOD: u64 = 3;
const KEY_NWC_CONNECTIONS: &str = "nwc_connections";
const KEY_NWC_PAID_INVOICES: &str = "nwc_paid_invoices";

type NwcConnections = BTreeMap<String, NwcConnectionInner>;
type PaidInvoices = BTreeMap<String, BTreeSet<String>>;

impl Persister {
    fn set_connections_safe<F, R>(&self, f: F) -> NostrResult<R>
    where
        F: Fn(&mut NwcConnections) -> NostrResult<(bool, R)>,
    {
        self.set_storage_safe(KEY_NWC_CONNECTIONS, Self::list_nwc_connections, f)
    }

    fn set_paid_invoices_safe<F, R>(&self, f: F) -> NostrResult<R>
    where
        F: Fn(&mut PaidInvoices) -> NostrResult<(bool, R)>,
    {
        self.set_storage_safe(KEY_NWC_PAID_INVOICES, Self::list_nwc_paid_invoices, f)
    }

    pub(crate) fn add_nwc_connection(
        &self,
        name: String,
        connection: NwcConnectionInner,
    ) -> NostrResult<()> {
        self.set_connections_safe(|connections| {
            if connections.contains_key(&name) {
                return Err(NostrError::ConnectionExists);
            }
            connections.insert(name.clone(), connection.clone());
            Ok((true, ()))
        })
    }

    // Helper method to update a connection's used budget directly
    pub(crate) fn update_budget(&self, name: &str, delta_sat: i64) -> NostrResult<()> {
        self.set_connections_safe(|connections| {
            let Some(connection) = connections.get_mut(name) else {
                return Err(NostrError::ConnectionNotFound);
            };
            connection.paid_amount_sat =
                connection.paid_amount_sat.saturating_add_signed(delta_sat);
            if let Some(ref mut periodic_budget) = &mut connection.periodic_budget {
                periodic_budget.used_budget_sat = periodic_budget
                    .used_budget_sat
                    .saturating_add_signed(delta_sat);
            }
            Ok((true, ()))
        })
    }

    pub(crate) fn edit_nwc_connection(
        &self,
        req: EditConnectionRequest,
    ) -> NostrResult<NwcConnectionInner> {
        self.set_connections_safe(|connections| {
            let Some(connection) = connections.get_mut(&req.name) else {
                return Err(NostrError::ConnectionNotFound );
            };
            if let Some(new_receive_only) = req.receive_only {
                connection.receive_only = new_receive_only;
            }
            match (req.expiry_time_mins, req.remove_expiry) {
                (Some(_), Some(_)) => {
                    return Err(NostrError::generic(
                        "`expiry_time_mins` and `remove_expiry` cannot both be set at once",
                    ));
                }
                (Some(new_expiry_mins), None) => {
                    connection.expiry_time_sec = Some(utils::mins_to_seconds(new_expiry_mins))
                }
                (None, Some(true)) => connection.expiry_time_sec = None,
                _ => {}
            }
            match (req.periodic_budget_req.clone(), req.remove_periodic_budget) {
                (Some(_), Some(_)) => {
                    return Err(NostrError::generic(
                        "`periodic_budget_req` and `remove_periodic_budget` cannot both be set at once",
                    ));
                }
                (Some(periodic_budget_req), None) => {
                    connection.periodic_budget = Some(PeriodicBudgetInner::from_budget_request(periodic_budget_req, utils::now()));
                }
                (None, Some(true)) => connection.periodic_budget = None,
                _ => {}
            }
            Ok((true, connection.clone()))
        })
    }

    pub(crate) fn get_min_interval(&self) -> Option<u64> {
        let get_min_connection_interval = || -> NostrResult<Option<u64>> {
            let connections = self.list_nwc_connections()?;
            if connections.is_empty() {
                return Ok(None);
            }
            let mut min_interval = None;
            for connection in connections.into_values() {
                if let Some(expiry_time_sec) = connection.expiry_time_sec {
                    if expiry_time_sec < min_interval.unwrap_or(u32::MAX) {
                        min_interval = Some(expiry_time_sec);
                    }
                }
                if let Some(renewal_time_sec) =
                    connection.periodic_budget.and_then(|b| b.renewal_time_sec)
                {
                    if renewal_time_sec < min_interval.unwrap_or(u32::MAX) {
                        min_interval = Some(renewal_time_sec);
                    }
                }
            }
            Ok(min_interval.map(u64::from))
        };
        match get_min_connection_interval() {
            Ok(Some(mut interval)) => {
                // We set a minimum of MIN_REFRESH_INTERVAL_SEC to avoid breaking the service by
                // refreshing too frequently
                if interval < MIN_REFRESH_INTERVAL_SEC {
                    interval = MIN_REFRESH_INTERVAL_SEC;
                }
                // We add a grace period to avoid races between the interval tick and the connection
                // expiry/budget refresh
                Some(interval + REFRESH_INTERVAL_GRACE_PERIOD)
            }
            _ => None,
        }
    }

    /// Refreshes the active connections (expiry and budget) returning two arrays of the
    /// corresponding connection names
    pub(crate) fn refresh_connections(&self) -> NostrResult<RefreshResult> {
        self.set_connections_safe(|connections| {
            let now = utils::now();
            let mut result = RefreshResult::default();
            for (name, connection) in connections.iter_mut() {
                // If the connection has expired, mark it for deletion
                if let Some(expiry) = connection.expiry_time_sec {
                    if now >= connection.created_at + expiry {
                        result.deleted.push(name.clone());
                        continue;
                    }
                }
                // If the connection's periodic budget has to be updated
                if let Some(ref mut budget) = connection.periodic_budget {
                    if let Some(renewal_time_sec) = budget.renewal_time_sec {
                        if now >= budget.updated_at + renewal_time_sec {
                            budget.used_budget_sat = 0;
                            budget.updated_at = now;
                            result.refreshed.push(name.clone());
                        }
                    }
                }
            }
            for name in &result.deleted {
                connections.remove(name);
            }
            Ok((
                !result.refreshed.is_empty() || !result.deleted.is_empty(),
                result,
            ))
        })
    }

    pub(crate) fn list_nwc_connections(&self) -> NostrResult<NwcConnections> {
        let connections = self
            .storage
            .get_item(KEY_NWC_CONNECTIONS)?
            .unwrap_or("{}".to_string());
        let connections = serde_json::from_str(&connections)?;
        Ok(connections)
    }

    pub(crate) fn remove_nwc_connection(&self, name: String) -> NostrResult<()> {
        self.set_connections_safe(|connections| {
            if connections.remove(&name).is_none() {
                return Err(NostrError::ConnectionNotFound);
            }
            Ok((true, ()))
        })?;
        self.set_paid_invoices_safe(|paid_invoices| {
            paid_invoices.remove(&name);
            Ok((true, ()))
        })?;
        Ok(())
    }

    pub(crate) fn list_nwc_paid_invoices(&self) -> NostrResult<PaidInvoices> {
        let paid_invoices = self
            .storage
            .get_item(KEY_NWC_PAID_INVOICES)?
            .unwrap_or("{}".to_string());
        let paid_invoices = serde_json::from_str(&paid_invoices)?;
        Ok(paid_invoices)
    }

    pub(crate) fn add_nwc_paid_invoice(
        &self,
        connection: &str,
        invoice: String,
    ) -> NostrResult<()> {
        self.set_paid_invoices_safe(|paid_invoices| {
            let invoices = paid_invoices
                .entry(connection.to_string())
                .or_insert_with(BTreeSet::new);
            invoices.insert(invoice.clone());
            Ok((true, ()))
        })
    }
}
