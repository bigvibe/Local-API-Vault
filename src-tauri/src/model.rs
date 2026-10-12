use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

pub const VAULT_VERSION: u32 = 1;

fn default_enabled() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretEntry {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub project: String,
    pub environment: String,
    pub username: String,
    pub secret: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Zeroize for SecretEntry {
    fn zeroize(&mut self) {
        self.id.zeroize();
        self.name.zeroize();
        self.kind.zeroize();
        self.project.zeroize();
        self.environment.zeroize();
        self.username.zeroize();
        self.secret.zeroize();
        self.url.zeroize();
        self.notes.zeroize();
        self.tags.zeroize();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProfile {
    pub id: String,
    pub name: String,
    pub working_directory: String,
    pub command: String,
    pub secrets: Vec<String>,
    pub project: String,
    pub environment: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Zeroize for RunProfile {
    fn zeroize(&mut self) {
        self.id.zeroize();
        self.name.zeroize();
        self.working_directory.zeroize();
        self.command.zeroize();
        self.secrets.zeroize();
        self.project.zeroize();
        self.environment.zeroize();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultDocument {
    pub version: u32,
    pub entries: Vec<SecretEntry>,
    #[serde(default)]
    pub profiles: Vec<RunProfile>,
}

impl VaultDocument {
    pub fn new() -> Self {
        Self {
            version: VAULT_VERSION,
            entries: Vec::new(),
            profiles: Vec::new(),
        }
    }
}

impl Default for VaultDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl Zeroize for VaultDocument {
    fn zeroize(&mut self) {
        self.entries.zeroize();
        self.profiles.zeroize();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryInput {
    pub id: Option<String>,
    pub name: String,
    pub kind: String,
    pub project: String,
    pub environment: String,
    pub username: String,
    pub secret: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl Zeroize for EntryInput {
    fn zeroize(&mut self) {
        self.id.zeroize();
        self.name.zeroize();
        self.kind.zeroize();
        self.project.zeroize();
        self.environment.zeroize();
        self.username.zeroize();
        self.secret.zeroize();
        self.url.zeroize();
        self.notes.zeroize();
        self.tags.zeroize();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProfileInput {
    pub id: Option<String>,
    pub name: String,
    pub working_directory: String,
    pub command: String,
    pub secrets: Vec<String>,
    pub project: String,
    pub environment: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub project: String,
    pub environment: String,
    pub username: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&SecretEntry> for EntrySummary {
    fn from(entry: &SecretEntry) -> Self {
        Self {
            id: entry.id.clone(),
            name: entry.name.clone(),
            kind: entry.kind.clone(),
            project: entry.project.clone(),
            environment: entry.environment.clone(),
            username: entry.username.clone(),
            url: entry.url.clone(),
            notes: entry.notes.clone(),
            tags: entry.tags.clone(),
            enabled: entry.enabled,
            created_at: entry.created_at,
            updated_at: entry.updated_at,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealedSecret {
    pub secret: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub locked: bool,
    pub path: Option<String>,
    pub entry_count: usize,
    pub recovery_enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVaultResult {
    #[serde(flatten)]
    pub status: VaultStatus,
    pub recovery_key: Option<String>,
    pub recovery_public_key: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildResult {
    #[serde(flatten)]
    pub status: VaultStatus,
    pub backup_path: String,
    pub recovery_key: Option<String>,
    pub recovery_public_key: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported_count: usize,
    pub skipped_count: usize,
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_entries_default_to_enabled() {
        let entry: SecretEntry = serde_json::from_str(
            r#"{"id":"id","name":"Example","kind":"api-key","project":"","environment":"","username":"","secret":"value","url":"","notes":"","tags":[],"createdAt":0,"updatedAt":0}"#,
        )
        .unwrap();
        assert!(entry.enabled);
    }

    #[test]
    fn legacy_documents_default_to_no_profiles() {
        let document: VaultDocument =
            serde_json::from_str(r#"{"version":1,"entries":[]}"#).unwrap();
        assert!(document.profiles.is_empty());
    }
}
