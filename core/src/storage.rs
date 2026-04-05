use aes::cipher::generic_array::GenericArray;
use aes_gcm::{
    AeadCore as _, Aes256Gcm, KeyInit as _, Nonce,
    aead::{Aead, OsRng},
};
use anyhow::{Result, bail};

#[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
#[derive(Debug, thiserror::Error)]
pub enum PluginStorageError {
    #[error("Could not write to storage: value has changed since last read.")]
    DataTooOld,

    #[error("Could not encrypt storage data: {err}")]
    Encryption { err: String },

    #[error("Plugin storage operation failed: {err}")]
    Generic { err: String },
}

impl PluginStorageError {
    pub fn generic<T: ToString>(err: T) -> Self {
        Self::Generic {
            err: err.to_string(),
        }
    }
}

impl From<aes_gcm::Error> for PluginStorageError {
    fn from(value: aes_gcm::Error) -> Self {
        Self::Encryption {
            err: value.to_string(),
        }
    }
}

pub type StorageResult<T> = Result<T, PluginStorageError>;

#[async_trait::async_trait]
pub trait PluginStorageController: Send + Sync {
    async fn get_item(&self, key: String) -> StorageResult<Option<String>>;
    async fn set_item(&self, key: String, value: String) -> StorageResult<()>;
    async fn set_item_safe(
        &self,
        key: String,
        value: String,
        old_value: String,
    ) -> StorageResult<()>;
    async fn remove_item(&self, key: String) -> StorageResult<()>;
}

#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
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
}

#[cfg_attr(feature = "uniffi", uniffi::export(async_runtime = "tokio"))]
impl PluginStorage {
    /// Writes/updates a value in the database
    ///
    /// # Arguments
    ///   - key: The name of the database key to write into
    ///   - value: The value to write
    pub async fn set_item(&self, key: &str, value: String) -> StorageResult<()> {
        let scoped_key = self.scoped_key(key);
        self.controller
            .set_item(scoped_key, self.encrypt(value)?)
            .await
    }

    /// Writes/updates a value in the database, doing so in a thread-safe manner
    ///
    /// # Arguments
    ///   - key: The name of the database key to write into
    ///   - value: The value to write
    ///   - old_value: The previous value of that field (if any). It will ensure that the value that's being written has not been modified, throwing a [PluginStorageError::DataTooOld] error otherwise
    pub async fn set_item_safe(
        &self,
        key: &str,
        value: String,
        old_value: String,
    ) -> StorageResult<()> {
        let scoped_key = self.scoped_key(key);
        self.controller
            .set_item_safe(scoped_key, self.encrypt(value)?, self.encrypt(old_value)?)
            .await
    }

    pub async fn get_item(&self, key: &str) -> StorageResult<Option<String>> {
        let scoped_key = self.scoped_key(key);
        let value = self.controller.get_item(scoped_key).await?;
        if let Some(value) = value {
            return Ok(Some(self.decrypt(value)?));
        }
        Ok(None)
    }

    pub async fn remove_item(&self, key: &str) -> StorageResult<()> {
        let scoped_key = self.scoped_key(key);
        self.controller.remove_item(scoped_key).await
    }
}
