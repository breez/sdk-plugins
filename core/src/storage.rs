use aes::cipher::generic_array::GenericArray;
use aes_gcm::{
    aead::{Aead, OsRng},
    AeadCore as _, Aes256Gcm, KeyInit as _, Nonce,
};
use anyhow::{bail, Result};

#[derive(Debug, thiserror::Error)]
pub enum PluginStorageError {
    #[error("Could not write to storage: value has changed since last read.")]
    DataTooOld,

    #[error("Could not encrypt storage data: {err}")]
    Encryption { err: String },

    #[error("Plugin storage operation failed: {err}")]
    Generic { err: String },
}

impl From<aes_gcm::Error> for PluginStorageError {
    fn from(value: aes_gcm::Error) -> Self {
        Self::Encryption {
            err: value.to_string(),
        }
    }
}

type StorageResult<T> = Result<T, PluginStorageError>;

pub trait Transaction {
    fn get_item(&self, key: &str) -> StorageResult<Option<String>>;
    fn set_item(&self, key: String, value: String) -> StorageResult<()>;
    fn remove_item(&self, key: &str) -> StorageResult<()>;
    fn commit(self: Box<Self>) -> StorageResult<()>;
}

pub trait PluginStorageController: Send + Sync {
    fn begin_tx(&self) -> Box<dyn Transaction>;
}

pub struct PluginStorage {
    plugin_id: String,
    cipher: Aes256Gcm,
    controller: Box<dyn PluginStorageController>,
}

impl From<anyhow::Error> for PluginStorageError {
    fn from(value: anyhow::Error) -> Self {
        Self::Generic {
            err: value.to_string(),
        }
    }
}

impl PluginStorage {
    pub fn new(
        controller: Box<dyn PluginStorageController>,
        passphrase: &[u8],
        plugin_id: String,
    ) -> Result<Self> {
        if plugin_id.is_empty() {
            log::error!("Plugin ID cannot be an empty string!");
            bail!("Plugin ID cannot be an empty string!");
        }
        let passphrase = GenericArray::clone_from_slice(passphrase);
        let cipher = Aes256Gcm::new(&passphrase);

        Ok(Self {
            cipher,
            controller,
            plugin_id,
        })
    }

    fn encrypt(&self, data: String) -> Result<String, PluginStorageError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let encrypted = self.cipher.encrypt(&nonce, data.as_bytes())?;
        let mut payload = nonce.to_vec();
        payload.extend_from_slice(&encrypted);
        Ok(hex::encode(payload))
    }

    fn decrypt(&self, data: String) -> Result<String, PluginStorageError> {
        let decoded = hex::decode(data).map_err(|err| PluginStorageError::Encryption {
            err: err.to_string(),
        })?;
        let (nonce, data) = decoded.split_at(12);
        let nonce = Nonce::from_slice(nonce);
        let decrypted = self.cipher.decrypt(nonce, data)?;
        let result =
            String::from_utf8(decrypted).map_err(|err| PluginStorageError::Encryption {
                err: err.to_string(),
            })?;
        Ok(result)
    }

    pub(crate) fn scoped_key(&self, key: &str) -> String {
        format!("{}-{}", self.plugin_id, key)
    }

    /// Writes/updates a value in the database
    ///
    /// # Arguments
    ///   - key: The name of the database key to write into
    ///   - value: The value to write
    ///   - old_value (optional): The previous value of that field (if any). When provided, it
    ///     will ensure that the value that's being written has not been modified, throwing a
    ///     [PluginStorageError::DataTooOld] error otherwise
    pub fn set_item(
        &self,
        key: &str,
        value: String,
        old_value: Option<String>,
    ) -> StorageResult<()> {
        let tx = self.controller.begin_tx();
        let scoped_key = self.scoped_key(&key);
        if let Some(old_value) = old_value {
            if let Some(current_value) = tx.get_item(&scoped_key)? {
                let current_value = self.decrypt(current_value)?;
                if old_value != current_value {
                    return Err(PluginStorageError::DataTooOld);
                }
            }
        }

        tx.set_item(scoped_key, self.encrypt(value)?)?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_item(&self, key: &str) -> StorageResult<Option<String>> {
        let scoped_key = self.scoped_key(key);
        let tx = self.controller.begin_tx();
        let value = tx.get_item(&scoped_key)?;
        tx.commit()?;
        if let Some(value) = value {
            return Ok(Some(self.decrypt(value)?));
        }
        Ok(None)
    }

    pub fn remove_item(&self, key: &str) -> StorageResult<()> {
        let scoped_key = self.scoped_key(key);
        let tx = self.controller.begin_tx();
        tx.remove_item(&scoped_key)?;
        tx.commit()?;
        Ok(())
    }
}
