use breez_plugins::{PluginStorage, PluginStorageError};
use serde::Serialize;

use crate::error::{NostrError, NostrResult};

const MAX_SAFE_WRITE_RETRIES: u64 = 3;

const KEY_NOSTR_SECKEY: &str = "nostr_seckey";

pub(crate) struct Persister {
    pub(crate) storage: PluginStorage,
}

impl Persister {
    pub(crate) fn new(storage: PluginStorage) -> Self {
        Self { storage }
    }

    pub(crate) fn set_seckey(&self, key: String) -> NostrResult<()> {
        self.storage
            .set_item(KEY_NOSTR_SECKEY, key, None)
            .map_err(Into::into)
    }

    pub(crate) fn get_seckey(&self) -> NostrResult<Option<String>> {
        self.storage.get_item(KEY_NOSTR_SECKEY).map_err(Into::into)
    }

    pub(crate) fn set_storage_safe<T, Getter, Setter, Res>(
        &self,
        storage_key: &'static str,
        get_data: Getter,
        set_data: Setter,
    ) -> NostrResult<Res>
    where
        T: Clone + Serialize,
        Getter: Fn(&Self) -> NostrResult<T>,
        Setter: Fn(&mut T) -> NostrResult<(bool, Res)>,
    {
        for _ in 0..MAX_SAFE_WRITE_RETRIES {
            let old_data = get_data(self)?;
            let mut new_data = old_data.clone();
            let (changed, result) = set_data(&mut new_data)?;
            if changed {
                let set_result = self.storage.set_item(
                    storage_key,
                    serde_json::to_string(&new_data)?,
                    Some(serde_json::to_string(&old_data)?),
                );
                match set_result {
                    Ok(_) => return Ok(result),
                    Err(PluginStorageError::DataTooOld) => continue,
                    Err(err) => return Err(err.into()),
                }
            }
            return Ok(result);
        }
        Err(NostrError::persist("Maximum write attempts reached"))
    }
}
