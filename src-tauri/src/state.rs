use age::secrecy::SecretString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use crate::model::{EntrySummary, VaultDocument, VaultStatus};
use crate::vault::VaultFile;
use zeroize::Zeroize;

pub struct UnlockedVault {
    pub path: PathBuf,
    pub passphrase: SecretString,
    pub document: VaultDocument,
}

impl UnlockedVault {
    pub fn from_parts(path: PathBuf, passphrase: String, document: VaultDocument) -> Self {
        Self {
            path,
            passphrase: SecretString::from(passphrase),
            document,
        }
    }

    pub fn open(path: &Path, mut passphrase: String) -> Result<Self, String> {
        let result = VaultFile::load(path, &passphrase).map_err(|error| error.to_string());
        if result.is_err() {
            passphrase.zeroize();
        }
        Ok(Self::from_parts(path.to_path_buf(), passphrase, result?))
    }

    pub fn status(&self) -> VaultStatus {
        VaultStatus {
            locked: false,
            path: Some(self.path.to_string_lossy().into_owned()),
            entry_count: self.document.entries.len(),
            recovery_enabled: VaultFile::has_identity_sidecar(&self.path),
        }
    }

    pub fn summaries(&self) -> Vec<EntrySummary> {
        self.document
            .entries
            .iter()
            .map(EntrySummary::from)
            .collect()
    }

    pub fn change_passphrase(
        &mut self,
        old_passphrase: &str,
        new_passphrase: &str,
    ) -> Result<(), String> {
        VaultFile::load(&self.path, old_passphrase).map_err(|error| error.to_string())?;
        VaultFile::change_passphrase(&self.path, old_passphrase, new_passphrase, &self.document)
            .map_err(|error| error.to_string())?;
        self.passphrase = SecretString::from(new_passphrase.to_owned());
        Ok(())
    }
}

impl Zeroize for UnlockedVault {
    fn zeroize(&mut self) {
        self.passphrase.zeroize();
        self.document.zeroize();
    }
}

pub struct AppState {
    inner: Mutex<Option<UnlockedVault>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl AppState {
    fn guard(&self) -> Result<MutexGuard<'_, Option<UnlockedVault>>, String> {
        self.inner
            .lock()
            .map_err(|_| "Vault 状态锁已损坏，请重新启动应用。".to_owned())
    }

    pub fn replace(&self, replacement: UnlockedVault) -> Result<(), String> {
        let mut guard = self.guard()?;
        if let Some(mut previous) = guard.take() {
            previous.zeroize();
        }
        *guard = Some(replacement);
        Ok(())
    }

    pub fn with_mut<T>(
        &self,
        operation: impl FnOnce(&mut UnlockedVault) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self.guard()?;
        let vault = guard
            .as_mut()
            .ok_or_else(|| "Vault 已锁定，请先解锁。".to_owned())?;
        operation(vault)
    }

    pub fn status(&self) -> Result<VaultStatus, String> {
        let guard = self.guard()?;
        Ok(guard
            .as_ref()
            .map(UnlockedVault::status)
            .unwrap_or(VaultStatus {
                locked: true,
                path: None,
                entry_count: 0,
                recovery_enabled: false,
            }))
    }

    pub fn lock(&self) -> Result<(), String> {
        let mut guard = self.guard()?;
        if let Some(mut vault) = guard.take() {
            vault.zeroize();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn changes_passphrase_for_identity_backed_vault() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let old_passphrase = "old passphrase with enough entropy";
        let new_passphrase = "new passphrase with enough entropy";
        let identity = age::x25519::Identity::generate();
        VaultFile::create_with_recovery(&path, old_passphrase, &identity).unwrap();
        let document = VaultFile::load(&path, old_passphrase).unwrap();
        let mut vault =
            UnlockedVault::from_parts(path.clone(), old_passphrase.to_owned(), document);

        vault
            .change_passphrase(old_passphrase, new_passphrase)
            .unwrap();
        assert!(VaultFile::load(&path, old_passphrase).is_err());
        assert!(VaultFile::load(&path, new_passphrase).is_ok());
        assert!(VaultFile::load_with_identity(&path, &identity).is_ok());
    }

    #[test]
    fn changes_passphrase_and_reencrypts_vault() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let old_passphrase = "old passphrase with enough entropy";
        let new_passphrase = "new passphrase with enough entropy";
        VaultFile::create(&path, old_passphrase).unwrap();
        let document = VaultFile::load(&path, old_passphrase).unwrap();
        let mut vault =
            UnlockedVault::from_parts(path.clone(), old_passphrase.to_owned(), document);

        vault
            .change_passphrase(old_passphrase, new_passphrase)
            .unwrap();
        assert!(VaultFile::load(&path, old_passphrase).is_err());
        assert!(VaultFile::load(&path, new_passphrase).is_ok());
    }
}
