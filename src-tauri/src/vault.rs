use crate::crypto;
use crate::model::{VaultDocument, VAULT_VERSION};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use thiserror::Error;
use zeroize::Zeroize;

const MAX_VAULT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("Vault 文件已存在")]
    AlreadyExists,
    #[error("Vault 文件不存在")]
    NotFound,
    #[error("无法读取或写入 Vault")]
    Io(#[from] std::io::Error),
    #[error("主密码错误或 Vault 文件已损坏")]
    InvalidPassphrase,
    #[error("Vault 文件格式无效")]
    InvalidData,
    #[error("Vault 版本不受支持")]
    UnsupportedVersion,
}

pub struct VaultFile;

impl VaultFile {
    pub fn identity_path(path: &Path) -> PathBuf {
        path.with_extension("identity.age")
    }

    pub fn has_identity_sidecar(path: &Path) -> bool {
        if !path.is_file() || !Self::identity_path(path).is_file() {
            return false;
        }
        read_bounded(path)
            .map(|encrypted| !crypto::is_scrypt_encrypted(&encrypted))
            .unwrap_or(false)
    }

    pub fn create(path: &Path, passphrase: &str) -> Result<VaultDocument, VaultError> {
        if path.exists() || Self::identity_path(path).exists() {
            return Err(VaultError::AlreadyExists);
        }
        let document = VaultDocument::new();
        write_legacy(path, passphrase, &document)?;
        Ok(document)
    }

    pub fn create_with_recovery(
        path: &Path,
        passphrase: &str,
        identity: &age::x25519::Identity,
    ) -> Result<VaultDocument, VaultError> {
        if path.exists() || Self::identity_path(path).exists() {
            return Err(VaultError::AlreadyExists);
        }
        let document = VaultDocument::new();
        write_new_pair(path, passphrase, identity, &document)?;
        Ok(document)
    }

    pub fn rebuild(path: &Path, passphrase: &str) -> Result<(VaultDocument, PathBuf), VaultError> {
        Self::rebuild_with_recovery(path, passphrase, None)
    }

    pub fn rebuild_with_recovery(
        path: &Path,
        passphrase: &str,
        identity: Option<&age::x25519::Identity>,
    ) -> Result<(VaultDocument, PathBuf), VaultError> {
        if !path.exists() {
            return Err(VaultError::NotFound);
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let backup = PathBuf::from(format!(
            "{}.before-rebuild-{}",
            path.to_string_lossy(),
            stamp
        ));
        let old_identity_path = Self::identity_path(path);
        let old_identity_exists = old_identity_path.exists();
        fs::rename(path, &backup)?;
        if old_identity_exists {
            if let Err(error) = fs::rename(&old_identity_path, Self::identity_path(&backup)) {
                let _ = fs::rename(&backup, path);
                return Err(VaultError::Io(error));
            }
        }

        let result = match identity {
            Some(identity) => Self::create_with_recovery(path, passphrase, identity),
            None => Self::create(path, passphrase),
        };
        match result {
            Ok(document) => Ok((document, backup)),
            Err(error) => {
                let _ = fs::remove_file(path);
                let _ = fs::remove_file(Self::identity_path(path));
                let _ = fs::rename(&backup, path);
                if old_identity_exists {
                    let _ = fs::rename(Self::identity_path(&backup), &old_identity_path);
                }
                Err(error)
            }
        }
    }

    pub fn load(path: &Path, passphrase: &str) -> Result<VaultDocument, VaultError> {
        let encrypted = read_bounded(path)?;
        let identity_path = Self::identity_path(path);
        if identity_path.exists() && !crypto::is_scrypt_encrypted(&encrypted) {
            let sidecar = read_bounded(&identity_path)?;
            let identity = crypto::decrypt_identity_sidecar(&sidecar, passphrase)
                .map_err(|_| VaultError::InvalidPassphrase)?;
            return Self::load_with_identity(path, &identity);
        }

        let mut plaintext =
            crypto::decrypt(&encrypted, passphrase).map_err(|_| VaultError::InvalidPassphrase)?;
        decode_document(&mut plaintext)
    }

    pub fn load_with_identity(
        path: &Path,
        identity: &age::x25519::Identity,
    ) -> Result<VaultDocument, VaultError> {
        let encrypted = read_bounded(path)?;
        let mut plaintext = crypto::decrypt_with_identity(&encrypted, identity)
            .map_err(|_| VaultError::InvalidPassphrase)?;
        decode_document(&mut plaintext)
    }

    pub fn recover(
        path: &Path,
        identity: &age::x25519::Identity,
        new_passphrase: &str,
    ) -> Result<VaultDocument, VaultError> {
        let document = Self::load_with_identity(path, identity)?;
        write_recovery_pair(path, new_passphrase, identity, &document)?;
        Ok(document)
    }

    pub fn save(path: &Path, passphrase: &str, document: &VaultDocument) -> Result<(), VaultError> {
        let identity_path = Self::identity_path(path);
        let identity_backed = identity_path.exists()
            && read_bounded(path)
                .map(|encrypted| !crypto::is_scrypt_encrypted(&encrypted))
                .unwrap_or(false);
        if identity_backed {
            let sidecar = read_bounded(&identity_path)?;
            let identity = crypto::decrypt_identity_sidecar(&sidecar, passphrase)
                .map_err(|_| VaultError::InvalidPassphrase)?;
            return save_with_identity(path, &identity, document);
        }
        write_legacy(path, passphrase, document)
    }

    pub fn change_passphrase(
        path: &Path,
        old_passphrase: &str,
        new_passphrase: &str,
        document: &VaultDocument,
    ) -> Result<(), VaultError> {
        let identity_path = Self::identity_path(path);
        let identity_backed = identity_path.exists()
            && read_bounded(path)
                .map(|encrypted| !crypto::is_scrypt_encrypted(&encrypted))
                .unwrap_or(false);
        if identity_backed {
            let sidecar = read_bounded(&identity_path)?;
            let identity = crypto::decrypt_identity_sidecar(&sidecar, old_passphrase)
                .map_err(|_| VaultError::InvalidPassphrase)?;
            let encrypted_sidecar = crypto::encrypt_identity_sidecar(&identity, new_passphrase)
                .map_err(|_| VaultError::InvalidData)?;
            write_atomic(&identity_path, &encrypted_sidecar)?;
            return Ok(());
        }
        write_legacy(path, new_passphrase, document)
    }

    pub fn migrate_to_identity(
        path: &Path,
        passphrase: &str,
        identity: &age::x25519::Identity,
        document: &VaultDocument,
    ) -> Result<(), VaultError> {
        if Self::has_identity_sidecar(path) {
            return Err(VaultError::AlreadyExists);
        }
        write_new_pair(path, passphrase, identity, document)
    }

    pub fn copy_encrypted(source: &Path, destination: &Path) -> Result<(), VaultError> {
        if source == destination {
            return Ok(());
        }
        let recovery_source = Self::identity_path(source);
        let recovery_destination = Self::identity_path(destination);
        if !recovery_source.exists() && recovery_destination.exists() {
            return Err(VaultError::InvalidData);
        }
        copy_file_atomic(source, destination)?;
        if recovery_source.exists() {
            copy_file_atomic(&recovery_source, &recovery_destination)?;
        }
        Ok(())
    }

    pub fn parse_recovery_identity(contents: &str) -> Result<age::x25519::Identity, VaultError> {
        let mut identity = None;
        for line in contents.lines() {
            let line = line.trim();
            if line.starts_with("AGE-SECRET-KEY-") {
                if identity.is_some() {
                    return Err(VaultError::InvalidData);
                }
                identity = Some(
                    line.parse::<age::x25519::Identity>()
                        .map_err(|_| VaultError::InvalidData)?,
                );
            }
        }
        identity.ok_or(VaultError::InvalidData)
    }

    pub fn write_recovery_identity(path: &Path, identity: &str) -> Result<(), VaultError> {
        let identity = identity.trim();
        identity
            .parse::<age::x25519::Identity>()
            .map_err(|_| VaultError::InvalidData)?;
        let mut contents = format!(
            "# Local API Vault recovery identity\n# Keep this file offline and private.\n{}\n",
            identity.to_ascii_uppercase()
        );
        let result = write_atomic(path, contents.as_bytes());
        contents.zeroize();
        result
    }
}

fn decode_document(plaintext: &mut Vec<u8>) -> Result<VaultDocument, VaultError> {
    let result =
        serde_json::from_slice::<VaultDocument>(plaintext).map_err(|_| VaultError::InvalidData);
    plaintext.zeroize();
    let document = result?;
    if document.version != VAULT_VERSION {
        return Err(VaultError::UnsupportedVersion);
    }
    validate_document(&document)?;
    Ok(document)
}

fn validate_document(document: &VaultDocument) -> Result<(), VaultError> {
    if document.entries.len() > 10_000 {
        return Err(VaultError::InvalidData);
    }
    let mut ids = HashSet::with_capacity(document.entries.len());
    if document.profiles.len() > 100 {
        return Err(VaultError::InvalidData);
    }
    let mut profile_ids = HashSet::with_capacity(document.profiles.len());
    let mut profile_names = HashSet::with_capacity(document.profiles.len());
    for profile in &document.profiles {
        if profile.id.is_empty()
            || profile.name.trim().is_empty()
            || profile.name.chars().count() > 100
            || profile.working_directory.chars().count() > 2000
            || profile.command.chars().count() > 4000
            || profile.secrets.len() > 50
            || profile
                .secrets
                .iter()
                .any(|secret| !valid_environment_name(secret))
            || !profile_ids.insert(&profile.id)
            || !profile_names.insert(profile.name.to_lowercase())
        {
            return Err(VaultError::InvalidData);
        }
    }
    for entry in &document.entries {
        if entry.id.is_empty()
            || entry.name.trim().is_empty()
            || entry.secret.len() > 4 * 1024 * 1024
            || entry.tags.len() > 20
            || !ids.insert(&entry.id)
        {
            return Err(VaultError::InvalidData);
        }
    }
    Ok(())
}

fn valid_environment_name(name: &str) -> bool {
    let mut characters = name.chars();
    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, VaultError> {
    if !path.exists() {
        return Err(VaultError::NotFound);
    }
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_VAULT_BYTES {
        return Err(VaultError::InvalidData);
    }
    Ok(fs::read(path)?)
}

fn write_legacy(path: &Path, passphrase: &str, document: &VaultDocument) -> Result<(), VaultError> {
    let mut plaintext = serde_json::to_vec(document).map_err(|_| VaultError::InvalidData)?;
    let encrypted = match crypto::encrypt(&plaintext, passphrase) {
        Ok(value) => {
            plaintext.zeroize();
            value
        }
        Err(_) => {
            plaintext.zeroize();
            return Err(VaultError::InvalidData);
        }
    };
    write_atomic(path, &encrypted)
}

fn write_new_pair(
    path: &Path,
    passphrase: &str,
    identity: &age::x25519::Identity,
    document: &VaultDocument,
) -> Result<(), VaultError> {
    let mut plaintext = serde_json::to_vec(document).map_err(|_| VaultError::InvalidData)?;
    let encrypted = match crypto::encrypt_to_identity(&plaintext, identity) {
        Ok(value) => value,
        Err(_) => {
            plaintext.zeroize();
            return Err(VaultError::InvalidData);
        }
    };
    let sidecar = match crypto::encrypt_identity_sidecar(identity, passphrase) {
        Ok(value) => value,
        Err(_) => {
            plaintext.zeroize();
            return Err(VaultError::InvalidData);
        }
    };
    plaintext.zeroize();

    let identity_path = VaultFile::identity_path(path);
    let previous_sidecar = if identity_path.exists() {
        Some(read_bounded(&identity_path)?)
    } else {
        None
    };
    write_atomic(&identity_path, &sidecar)?;
    if let Err(error) = write_atomic(path, &encrypted) {
        if let Some(previous_sidecar) = previous_sidecar {
            let _ = write_atomic(&identity_path, &previous_sidecar);
        } else {
            let _ = fs::remove_file(&identity_path);
        }
        return Err(error);
    }
    Ok(())
}

fn write_recovery_pair(
    path: &Path,
    passphrase: &str,
    identity: &age::x25519::Identity,
    document: &VaultDocument,
) -> Result<(), VaultError> {
    let mut plaintext = serde_json::to_vec(document).map_err(|_| VaultError::InvalidData)?;
    let encrypted = match crypto::encrypt_to_identity(&plaintext, identity) {
        Ok(value) => value,
        Err(_) => {
            plaintext.zeroize();
            return Err(VaultError::InvalidData);
        }
    };
    plaintext.zeroize();
    write_atomic(path, &encrypted)?;
    let sidecar = crypto::encrypt_identity_sidecar(identity, passphrase)
        .map_err(|_| VaultError::InvalidData)?;
    write_atomic(&VaultFile::identity_path(path), &sidecar)
}

fn save_with_identity(
    path: &Path,
    identity: &age::x25519::Identity,
    document: &VaultDocument,
) -> Result<(), VaultError> {
    let mut plaintext = serde_json::to_vec(document).map_err(|_| VaultError::InvalidData)?;
    let encrypted = match crypto::encrypt_to_identity(&plaintext, identity) {
        Ok(value) => value,
        Err(_) => {
            plaintext.zeroize();
            return Err(VaultError::InvalidData);
        }
    };
    plaintext.zeroize();
    write_atomic(path, &encrypted)
}

fn copy_file_atomic(source: &Path, destination: &Path) -> Result<(), VaultError> {
    if source == destination {
        return Ok(());
    }
    let metadata = fs::metadata(source)?;
    if metadata.len() > MAX_VAULT_BYTES {
        return Err(VaultError::InvalidData);
    }
    let parent = destination
        .parent()
        .filter(|candidate| !candidate.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let mut input = fs::File::open(source)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    std::io::copy(&mut input, temporary.as_file_mut())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(destination)
        .map_err(|error| VaultError::Io(error.error))?;
    Ok(())
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), VaultError> {
    let parent = path
        .parent()
        .filter(|candidate| !candidate.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| VaultError::Io(error.error))?;
    Ok(())
}

pub fn default_vault_path() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    home.join(".local-api-vault").join("vault.age")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_load_and_save_vault() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let passphrase = "test passphrase with enough entropy";

        let created = VaultFile::create(&path, passphrase).unwrap();
        assert!(created.entries.is_empty());

        let mut document = VaultFile::load(&path, passphrase).unwrap();
        document.entries.push(crate::model::SecretEntry {
            id: "entry-id".into(),
            name: "Example".into(),
            kind: "api-key".into(),
            project: "app".into(),
            environment: "development".into(),
            username: String::new(),
            secret: "secret".into(),
            url: String::new(),
            notes: String::new(),
            tags: vec![],
            enabled: true,
            created_at: 1,
            updated_at: 1,
        });
        VaultFile::save(&path, passphrase, &document).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("secret"));

        let backup = directory.path().join("backup.age");
        VaultFile::copy_encrypted(&path, &backup).unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), bytes);

        let loaded = VaultFile::load(&path, passphrase).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].secret, "secret");
    }

    #[test]
    fn migrates_legacy_vault_to_identity_backed_format() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let passphrase = "legacy passphrase with enough entropy";
        let identity = age::x25519::Identity::generate();
        VaultFile::create(&path, passphrase).unwrap();
        let document = VaultFile::load(&path, passphrase).unwrap();

        VaultFile::migrate_to_identity(&path, passphrase, &identity, &document).unwrap();
        assert!(VaultFile::identity_path(&path).exists());
        assert!(VaultFile::load(&path, passphrase).is_ok());
        assert!(VaultFile::load_with_identity(&path, &identity).is_ok());
    }

    #[test]
    fn recovery_sidecar_can_recover_without_passphrase() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let passphrase = "original passphrase with enough entropy";
        let new_passphrase = "replacement passphrase with enough entropy";
        let identity = age::x25519::Identity::generate();
        VaultFile::create_with_recovery(&path, passphrase, &identity).unwrap();
        assert!(VaultFile::identity_path(&path).exists());

        let backup = directory.path().join("backup.age");
        VaultFile::copy_encrypted(&path, &backup).unwrap();
        assert!(VaultFile::identity_path(&backup).exists());

        VaultFile::recover(&path, &identity, new_passphrase).unwrap();
        assert!(VaultFile::load(&path, passphrase).is_err());
        assert!(VaultFile::load(&path, new_passphrase).is_ok());
    }

    #[test]
    fn rebuild_preserves_previous_vault() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let old_passphrase = "old passphrase with enough entropy";
        let new_passphrase = "new passphrase with enough entropy";
        VaultFile::create(&path, old_passphrase).unwrap();

        let (document, backup) = VaultFile::rebuild(&path, new_passphrase).unwrap();
        assert!(document.entries.is_empty());
        assert!(backup.exists());
        assert!(VaultFile::load(&path, new_passphrase).is_ok());
        assert!(VaultFile::load(&backup, old_passphrase).is_ok());
    }

    #[test]
    fn wrong_passphrase_is_sanitized() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        VaultFile::create(&path, "right password").unwrap();
        let error = VaultFile::load(&path, "wrong password").unwrap_err();
        assert!(matches!(error, VaultError::InvalidPassphrase));
    }

    /// Forward-compatibility guard.
    ///
    /// Writes a vault file exactly the way an older release would have: the
    /// passphrase-only age format, and a JSON document that predates both the
    /// `enabled` flag and the run-profile collection. Today must still be able
    /// to open it, apply defaults, rewrite it, and read it back unchanged.
    ///
    /// If this test ever needs editing to pass, the change is not backward
    /// compatible. Prefer adding an optional field with `#[serde(default)]`
    /// over bumping `VAULT_VERSION` or dropping an old read path.
    #[test]
    fn opens_and_rewrites_a_pre_profiles_vault() {
        const LEGACY_JSON: &str = r#"{
          "version": 1,
          "entries": [
            {
              "id": "legacy-entry",
              "name": "LEGACY_KEY",
              "kind": "api-key",
              "project": "app",
              "environment": "development",
              "username": "",
              "secret": "legacy-secret",
              "url": "",
              "notes": "",
              "tags": ["keep"],
              "createdAt": 100,
              "updatedAt": 100
            }
          ]
        }"#;

        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let passphrase = "passphrase from the old release";

        let encrypted = crypto::encrypt(LEGACY_JSON.as_bytes(), passphrase).unwrap();
        std::fs::write(&path, &encrypted).unwrap();

        // A pre-profile vault loads, and missing fields fall back to defaults.
        let mut loaded = VaultFile::load(&path, passphrase).unwrap();
        assert_eq!(loaded.version, VAULT_VERSION);
        assert!(loaded.profiles.is_empty());
        assert_eq!(loaded.entries.len(), 1);
        assert!(loaded.entries[0].enabled);
        assert_eq!(loaded.entries[0].secret, "legacy-secret");
        assert_eq!(loaded.entries[0].tags, vec!["keep".to_owned()]);

        // Rewriting through today's code must preserve every legacy field.
        loaded.entries[0].notes = "touched by a newer release".into();
        VaultFile::save(&path, passphrase, &loaded).unwrap();

        let reloaded = VaultFile::load(&path, passphrase).unwrap();
        assert!(reloaded.profiles.is_empty());
        assert_eq!(reloaded.entries.len(), 1);
        assert!(reloaded.entries[0].enabled);
        assert_eq!(reloaded.entries[0].name, "LEGACY_KEY");
        assert_eq!(reloaded.entries[0].project, "app");
        assert_eq!(reloaded.entries[0].environment, "development");
        assert_eq!(reloaded.entries[0].secret, "legacy-secret");
        assert_eq!(reloaded.entries[0].notes, "touched by a newer release");
        assert_eq!(reloaded.entries[0].tags, vec!["keep".to_owned()]);

        // Fields added after the upgrade are accepted by that same file.
        let mut upgraded = reloaded;
        upgraded.profiles.push(crate::model::RunProfile {
            id: "profile-id".into(),
            name: "legacy-dev".into(),
            working_directory: directory.path().to_string_lossy().into_owned(),
            command: "npm run dev".into(),
            secrets: vec!["LEGACY_KEY".into()],
            project: "app".into(),
            environment: "development".into(),
            created_at: 1,
            updated_at: 1,
        });
        upgraded.entries[0].enabled = false;
        VaultFile::save(&path, passphrase, &upgraded).unwrap();

        let final_state = VaultFile::load(&path, passphrase).unwrap();
        assert_eq!(final_state.profiles.len(), 1);
        assert_eq!(final_state.profiles[0].name, "legacy-dev");
        assert!(!final_state.entries[0].enabled);
        assert_eq!(final_state.entries[0].secret, "legacy-secret");
    }
}
