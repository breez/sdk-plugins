use std::collections::BTreeMap;

use super::model::TrackedZap;
use crate::{error::NostrResult, persist::Persister, utils};

const KEY_TRACKED_ZAPS: &str = "nostr_zaps";
const ZAP_TRACKING_EXPIRY_SEC: u32 = 60 * 60;

type TrackedZaps = BTreeMap<String, TrackedZap>;

impl Persister {
    fn set_tracked_zaps_safe<F, R>(&self, f: F) -> NostrResult<R>
    where
        F: Fn(&mut TrackedZaps) -> NostrResult<(bool, R)>,
    {
        self.set_storage_safe(KEY_TRACKED_ZAPS, Self::list_tracked_zaps, f)
    }

    pub(crate) fn list_tracked_zaps(&self) -> NostrResult<TrackedZaps> {
        let tracked_zaps = self
            .storage
            .get_item(KEY_TRACKED_ZAPS)?
            .unwrap_or("{}".to_string());
        let tracked_zaps = serde_json::from_str(&tracked_zaps)?;
        Ok(tracked_zaps)
    }

    pub(crate) fn add_tracked_zap(&self, invoice: String, zap_request: String) -> NostrResult<()> {
        self.set_tracked_zaps_safe(|tracked_zaps| {
            tracked_zaps.insert(
                invoice.clone(),
                TrackedZap {
                    zap_request: zap_request.clone(),
                    expires_at: utils::now() + ZAP_TRACKING_EXPIRY_SEC,
                },
            );
            Ok((true, ()))
        })
    }

    pub(crate) fn get_tracked_zap_raw(&self, invoice: &str) -> NostrResult<Option<String>> {
        self.set_tracked_zaps_safe(|tracked_zaps| {
            let result = tracked_zaps.get(invoice).map(|zap| zap.zap_request.clone());
            Ok((false, result))
        })
    }

    pub(crate) fn remove_tracked_zap(
        &self,
        invoice: &str,
    ) -> NostrResult<Option<nostr_sdk::Event>> {
        self.set_tracked_zaps_safe(|tracked_zaps| {
            let tracked_zap = tracked_zaps.remove(invoice);
            let zap_request =
                tracked_zap.and_then(|zap| serde_json::from_str(&zap.zap_request).ok());
            Ok((zap_request.is_some(), zap_request))
        })
    }

    pub(crate) fn clean_expired_zaps(&self) -> NostrResult<()> {
        let now = utils::now();
        self.set_tracked_zaps_safe(|tracked_zaps| {
            let mut expired = vec![];
            for (invoice, TrackedZap { expires_at, .. }) in tracked_zaps.iter() {
                if now >= *expires_at {
                    expired.push(invoice.clone());
                }
            }
            for invoice in expired.iter() {
                tracked_zaps.remove(invoice);
            }
            Ok((!expired.is_empty(), ()))
        })
    }
}
