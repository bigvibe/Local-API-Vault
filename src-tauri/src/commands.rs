use crate::model::{
    now_millis, CreateVaultResult, EntryInput, EntrySummary, ImportResult, RebuildResult,
    RevealedSecret, RunProfile, RunProfileInput, SecretEntry, VaultStatus,
};
use crate::state::{AppState, UnlockedVault};
use crate::vault::{default_vault_path as default_path, VaultFile};
use age::secrecy::ExposeSecret;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

const MAX_ENV_BYTES: u64 = 4 * 1024 * 1024;
const MAX_RECOVERY_IDENTITY_BYTES: u64 = 64 * 1024;

fn recovery_material() -> (age::x25519::Identity, String, String) {
    let identity = age::x25519::Identity::generate();
    let public_key = identity.to_public().to_string();
    let private_key = identity.to_string().expose_secret().to_owned();
    (identity, private_key, public_key)
}

#[tauri::command]
pub fn default_vault_path() -> String {
    default_path().to_string_lossy().into_owned()
}

#[tauri::command]
pub fn autostart_status() -> Result<bool, String> {
    crate::autostart::is_enabled()
}

#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    crate::autostart::set_enabled(enabled)?;
    crate::autostart::sync_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn create_vault(
    state: State<'_, Arc<AppState>>,
    path: String,
    passphrase: String,
) -> Result<CreateVaultResult, String> {
    validate_passphrase(&passphrase)?;
    let path = PathBuf::from(path.trim());
    if path.as_os_str().is_empty() {
        return Err("请填写 Vault 文件路径。".to_owned());
    }

    let (identity, recovery_key, recovery_public_key) = recovery_material();
    let document = VaultFile::create_with_recovery(&path, &passphrase, &identity)
        .map_err(|error| error.to_string())?;
    let vault = UnlockedVault::from_parts(path, passphrase, document);
    state.replace(vault)?;
    Ok(CreateVaultResult {
        status: state.status()?,
        recovery_key: Some(recovery_key),
        recovery_public_key: Some(recovery_public_key),
    })
}

#[tauri::command]
pub fn rebuild_vault(
    state: State<'_, Arc<AppState>>,
    path: String,
    new_passphrase: String,
) -> Result<RebuildResult, String> {
    validate_passphrase(&new_passphrase)?;
    let path = PathBuf::from(path.trim());
    if path.as_os_str().is_empty() {
        return Err("请填写 Vault 文件路径。".to_owned());
    }
    if !state.status()?.locked {
        return Err("请先锁定当前 Vault，再执行重建。".to_owned());
    }

    let (identity, recovery_key, recovery_public_key) = recovery_material();
    let (document, backup) =
        VaultFile::rebuild_with_recovery(&path, &new_passphrase, Some(&identity))
            .map_err(|error| error.to_string())?;
    let vault = UnlockedVault::from_parts(path, new_passphrase, document);
    state.replace(vault)?;
    let status = state.status()?;
    Ok(RebuildResult {
        status,
        backup_path: backup.to_string_lossy().into_owned(),
        recovery_key: Some(recovery_key),
        recovery_public_key: Some(recovery_public_key),
    })
}

#[tauri::command]
pub fn open_vault(
    state: State<'_, Arc<AppState>>,
    path: String,
    passphrase: String,
) -> Result<VaultStatus, String> {
    validate_passphrase(&passphrase)?;
    let path = PathBuf::from(path.trim());
    if path.as_os_str().is_empty() {
        return Err("请填写 Vault 文件路径。".to_owned());
    }

    let vault = UnlockedVault::open(&path, passphrase)?;
    state.replace(vault)?;
    state.status()
}

#[tauri::command]
pub fn vault_status(state: State<'_, Arc<AppState>>) -> Result<VaultStatus, String> {
    state.status()
}

#[tauri::command]
pub fn list_entries(state: State<'_, Arc<AppState>>) -> Result<Vec<EntrySummary>, String> {
    state.with_mut(|vault| Ok(vault.summaries()))
}

#[tauri::command]
pub fn save_entry(
    state: State<'_, Arc<AppState>>,
    mut input: EntryInput,
) -> Result<EntrySummary, String> {
    if let Err(error) = validate_entry_input(&input) {
        input.zeroize();
        return Err(error);
    }

    let result = state.with_mut(|vault| {
        let mut next = vault.document.clone();
        let now = now_millis();
        let index = if let Some(id) = input.id.as_deref() {
            let index = next
                .entries
                .iter()
                .position(|entry| entry.id == id)
                .ok_or_else(|| "密钥条目不存在或已发生变化。".to_owned())?;
            let created_at = next.entries[index].created_at;
            next.entries[index] = SecretEntry {
                id: id.to_owned(),
                name: input.name.trim().to_owned(),
                kind: normalize_kind(&input.kind),
                project: input.project.trim().to_owned(),
                environment: input.environment.trim().to_owned(),
                username: input.username.trim().to_owned(),
                secret: input.secret.clone(),
                url: input.url.trim().to_owned(),
                notes: input.notes.trim().to_owned(),
                tags: normalize_tags(input.tags.clone()),
                enabled: input.enabled,
                created_at,
                updated_at: now,
            };
            index
        } else {
            let entry = SecretEntry {
                id: Uuid::new_v4().to_string(),
                name: input.name.trim().to_owned(),
                kind: normalize_kind(&input.kind),
                project: input.project.trim().to_owned(),
                environment: input.environment.trim().to_owned(),
                username: input.username.trim().to_owned(),
                secret: input.secret.clone(),
                url: input.url.trim().to_owned(),
                notes: input.notes.trim().to_owned(),
                tags: normalize_tags(input.tags.clone()),
                enabled: input.enabled,
                created_at: now,
                updated_at: now,
            };
            next.entries.push(entry);
            next.entries.len() - 1
        };

        if let Err(error) = vault_save(&vault.path, &vault.passphrase, &next) {
            next.zeroize();
            return Err(error.to_string());
        }

        let summary = EntrySummary::from(&next.entries[index]);
        let mut previous = std::mem::replace(&mut vault.document, next);
        previous.zeroize();
        Ok(summary)
    });
    input.zeroize();
    result
}

#[tauri::command]
pub fn reveal_entry(state: State<'_, Arc<AppState>>, id: String) -> Result<RevealedSecret, String> {
    state.with_mut(|vault| {
        vault
            .document
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| RevealedSecret {
                secret: entry.secret.clone(),
            })
            .ok_or_else(|| "密钥条目不存在。".to_owned())
    })
}

#[tauri::command]
pub fn entry_reference(state: State<'_, Arc<AppState>>, id: String) -> Result<String, String> {
    state.with_mut(|vault| {
        let entry = vault
            .document
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| "密钥条目不存在。".to_owned())?;
        Ok(crate::redact::entry_reference(entry))
    })
}

#[tauri::command]
pub fn get_secret(state: State<'_, Arc<AppState>>, id: String) -> Result<RevealedSecret, String> {
    state.with_mut(|vault| {
        vault
            .document
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| "密钥条目不存在。".to_owned())
            .and_then(|entry| {
                if entry.enabled {
                    Ok(RevealedSecret {
                        secret: entry.secret.clone(),
                    })
                } else {
                    Err("密钥已停用。".to_owned())
                }
            })
    })
}

#[tauri::command]
pub fn list_run_profiles(state: State<'_, Arc<AppState>>) -> Result<Vec<RunProfile>, String> {
    state.with_mut(|vault| Ok(vault.document.profiles.clone()))
}

#[tauri::command]
pub fn save_run_profile(
    state: State<'_, Arc<AppState>>,
    input: RunProfileInput,
) -> Result<RunProfile, String> {
    validate_profile_input(&input)?;
    state.with_mut(|vault| {
        let mut next = vault.document.clone();
        let now = now_millis();
        let index = if let Some(id) = input.id.as_deref() {
            Some(
                next.profiles
                    .iter()
                    .position(|profile| profile.id == id)
                    .ok_or_else(|| "运行配置不存在。".to_owned())?,
            )
        } else {
            None
        };
        if next.profiles.iter().enumerate().any(|(position, profile)| {
            Some(position) != index && profile.name.eq_ignore_ascii_case(input.name.trim())
        }) {
            return Err("运行配置名称已存在。".to_owned());
        }
        let created_at = index
            .and_then(|position| {
                next.profiles
                    .get(position)
                    .map(|profile| profile.created_at)
            })
            .unwrap_or(now);
        let profile = RunProfile {
            id: index
                .map(|position| next.profiles[position].id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            name: input.name.trim().to_owned(),
            working_directory: input.working_directory.trim().to_owned(),
            command: input.command.trim().to_owned(),
            secrets: normalize_secret_names(input.secrets),
            project: input.project.trim().to_owned(),
            environment: input.environment.trim().to_owned(),
            created_at,
            updated_at: now,
        };
        if let Some(position) = index {
            next.profiles[position] = profile.clone();
        } else {
            next.profiles.push(profile.clone());
        }
        if let Err(error) = vault_save(&vault.path, &vault.passphrase, &next) {
            next.zeroize();
            return Err(error.to_string());
        }
        let mut previous = std::mem::replace(&mut vault.document, next);
        previous.zeroize();
        Ok(profile)
    })
}

#[tauri::command]
pub fn delete_run_profile(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    state.with_mut(|vault| {
        let mut next = vault.document.clone();
        let Some(index) = next.profiles.iter().position(|profile| profile.id == id) else {
            return Err("运行配置不存在。".to_owned());
        };
        next.profiles.remove(index);
        if let Err(error) = vault_save(&vault.path, &vault.passphrase, &next) {
            next.zeroize();
            return Err(error.to_string());
        }
        let mut previous = std::mem::replace(&mut vault.document, next);
        previous.zeroize();
        Ok(())
    })
}

#[tauri::command]
pub fn delete_entry(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    state.with_mut(|vault| {
        let mut next = vault.document.clone();
        let Some(index) = next.entries.iter().position(|entry| entry.id == id) else {
            return Err("密钥条目不存在。".to_owned());
        };
        next.entries.remove(index);
        if let Err(error) = vault_save(&vault.path, &vault.passphrase, &next) {
            next.zeroize();
            return Err(error.to_string());
        }
        let mut previous = std::mem::replace(&mut vault.document, next);
        previous.zeroize();
        Ok(())
    })
}

#[tauri::command]
pub fn import_env(state: State<'_, Arc<AppState>>, path: String) -> Result<ImportResult, String> {
    let source = PathBuf::from(path.trim());
    if source.as_os_str().is_empty() {
        return Err("请填写 .env 文件路径。".to_owned());
    }
    let metadata = fs::metadata(&source)
        .map_err(|_| "无法读取 .env 文件，请检查路径和文件权限。".to_owned())?;
    if metadata.len() > MAX_ENV_BYTES {
        return Err(".env 文件过大，最多支持 4 MB。".to_owned());
    }
    let mut contents = fs::read_to_string(&source)
        .map_err(|_| "无法读取 .env 文件，请检查路径和文件权限。".to_owned())?;
    let project = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "imported".to_owned());
    let parsed = Zeroizing::new({
        let parsed = parse_env(&contents, &project);
        contents.zeroize();
        parsed
    });

    state.with_mut(|vault| {
        let mut next = vault.document.clone();
        let mut imported_count = 0usize;
        let mut skipped_count = 0usize;
        let now = now_millis();
        for item in parsed.iter() {
            if next
                .entries
                .iter()
                .any(|entry| entry.name == item.name && entry.project == item.project)
            {
                skipped_count += 1;
                continue;
            }
            next.entries.push(SecretEntry {
                id: Uuid::new_v4().to_string(),
                name: item.name.clone(),
                kind: "environment".to_owned(),
                project: item.project.clone(),
                environment: "development".to_owned(),
                username: String::new(),
                secret: item.secret.clone(),
                url: String::new(),
                notes: "从 .env 文件导入".to_owned(),
                tags: vec!["imported".to_owned()],
                enabled: true,
                created_at: now,
                updated_at: now,
            });
            imported_count += 1;
        }

        if let Err(error) = vault_save(&vault.path, &vault.passphrase, &next) {
            next.zeroize();
            return Err(error.to_string());
        }
        let mut previous = std::mem::replace(&mut vault.document, next);
        previous.zeroize();
        Ok(ImportResult {
            imported_count,
            skipped_count,
        })
    })
}

#[tauri::command]
pub fn export_backup(state: State<'_, Arc<AppState>>, destination: String) -> Result<(), String> {
    let destination = PathBuf::from(destination.trim());
    if destination.as_os_str().is_empty() {
        return Err("请选择备份文件路径。".to_owned());
    }
    state.with_mut(|vault| {
        if destination == vault.path {
            return Err("备份路径不能与当前 Vault 相同。".to_owned());
        }
        VaultFile::copy_encrypted(&vault.path, &destination).map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn change_passphrase(
    state: State<'_, Arc<AppState>>,
    mut old_passphrase: String,
    mut new_passphrase: String,
) -> Result<(), String> {
    let result = (|| {
        validate_passphrase(&old_passphrase).map_err(|error| format!("当前{error}"))?;
        validate_passphrase(&new_passphrase)?;
        if old_passphrase == new_passphrase {
            return Err("新主密码不能与当前主密码相同。".to_owned());
        }
        state.with_mut(|vault| vault.change_passphrase(&old_passphrase, &new_passphrase))
    })();
    old_passphrase.zeroize();
    new_passphrase.zeroize();
    result
}

#[tauri::command]
pub fn enable_recovery(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    state.with_mut(|vault| {
        if VaultFile::has_identity_sidecar(&vault.path) {
            return Err("当前 Vault 已经启用了恢复密钥。".to_owned());
        }
        let (identity, recovery_key, _) = recovery_material();
        VaultFile::migrate_to_identity(
            &vault.path,
            vault.passphrase.expose_secret(),
            &identity,
            &vault.document,
        )
        .map_err(|error| error.to_string())?;
        Ok(recovery_key)
    })
}

#[tauri::command]
pub fn save_recovery_identity(path: String, mut identity: String) -> Result<(), String> {
    let path = PathBuf::from(path.trim());
    if path.as_os_str().is_empty() {
        identity.zeroize();
        return Err("请选择恢复文件保存位置。".to_owned());
    }
    let result =
        VaultFile::write_recovery_identity(&path, &identity).map_err(|error| error.to_string());
    identity.zeroize();
    result
}

#[tauri::command]
pub fn recover_vault(
    state: State<'_, Arc<AppState>>,
    path: String,
    identity_path: String,
    mut new_passphrase: String,
) -> Result<VaultStatus, String> {
    validate_passphrase(&new_passphrase)?;
    let path = PathBuf::from(path.trim());
    let identity_path = PathBuf::from(identity_path.trim());
    if path.as_os_str().is_empty() || identity_path.as_os_str().is_empty() {
        return Err("请选择 Vault 和恢复身份文件。".to_owned());
    }
    if !state.status()?.locked {
        return Err("请先锁定当前 Vault，再执行恢复。".to_owned());
    }
    let identity_metadata =
        fs::metadata(&identity_path).map_err(|_| "无法读取恢复身份文件。".to_owned())?;
    if identity_metadata.len() > MAX_RECOVERY_IDENTITY_BYTES {
        return Err("恢复身份文件过大。".to_owned());
    }
    let mut identity_contents =
        fs::read_to_string(&identity_path).map_err(|_| "无法读取恢复身份文件。".to_owned())?;
    let identity_result = VaultFile::parse_recovery_identity(&identity_contents)
        .map_err(|_| "恢复身份文件格式无效。".to_owned());
    identity_contents.zeroize();
    let identity = identity_result?;
    let result =
        VaultFile::recover(&path, &identity, &new_passphrase).map_err(|error| error.to_string());
    if result.is_err() {
        new_passphrase.zeroize();
    }
    let document = result?;
    let vault = UnlockedVault::from_parts(path, new_passphrase, document);
    state.replace(vault)?;
    state.status()
}

#[tauri::command]
pub fn lock_vault(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.lock()
}

fn validate_passphrase(passphrase: &str) -> Result<(), String> {
    if passphrase.is_empty() {
        return Err("请输入主密码。".to_owned());
    }
    if passphrase.len() < 12 {
        return Err("主密码至少需要 12 个字符。".to_owned());
    }
    Ok(())
}

fn validate_profile_input(input: &RunProfileInput) -> Result<(), String> {
    if input.name.trim().is_empty() || input.name.chars().count() > 100 {
        return Err("运行配置名称不能为空且不能超过 100 个字符。".to_owned());
    }
    if input.working_directory.trim().is_empty() {
        return Err("请填写项目目录。".to_owned());
    }
    if input.command.trim().is_empty() || input.command.chars().count() > 4000 {
        return Err("请填写启动命令，且命令不能超过 4000 个字符。".to_owned());
    }
    if input.secrets.len() > 50
        || input
            .secrets
            .iter()
            .any(|secret| !valid_environment_name(secret.trim()))
    {
        return Err("请选择合法的环境变量名称。".to_owned());
    }
    if input.project.chars().count() > 200 || input.environment.chars().count() > 80 {
        return Err("项目或环境名称过长。".to_owned());
    }
    Ok(())
}

fn normalize_secret_names(names: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for name in names {
        let name = name.trim().to_owned();
        if !name.is_empty() && !result.contains(&name) {
            result.push(name);
        }
    }
    result
}

fn valid_environment_name(name: &str) -> bool {
    let mut characters = name.chars();
    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn validate_entry_input(input: &EntryInput) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("名称不能为空。".to_owned());
    }
    if input.name.chars().count() > 200 {
        return Err("名称不能超过 200 个字符。".to_owned());
    }
    if input.secret.is_empty() {
        return Err("密钥不能为空。".to_owned());
    }
    if input.secret.len() > 4 * 1024 * 1024 {
        return Err("单个密钥过大，请检查导入内容。".to_owned());
    }
    let fields = [
        ("项目", input.project.as_str(), 200),
        ("环境", input.environment.as_str(), 80),
        ("用户名", input.username.as_str(), 512),
        ("服务地址", input.url.as_str(), 2048),
        ("备注", input.notes.as_str(), 10_000),
    ];
    for (label, value, max_length) in fields {
        if value.chars().count() > max_length {
            return Err(format!("{label}内容过长。"));
        }
    }
    if input.tags.len() > 20 || input.tags.iter().any(|tag| tag.chars().count() > 64) {
        return Err("标签数量或长度过多。".to_owned());
    }
    Ok(())
}

fn normalize_kind(kind: &str) -> String {
    match kind {
        "api-key" | "token" | "environment" | "private-key" => kind.to_owned(),
        _ => "other".to_owned(),
    }
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if !tag.is_empty() && !result.iter().any(|existing| existing == tag) {
            result.push(tag.to_owned());
        }
    }
    result.truncate(20);
    result
}

fn vault_save(
    path: &std::path::Path,
    passphrase: &age::secrecy::SecretString,
    document: &crate::model::VaultDocument,
) -> Result<(), crate::vault::VaultError> {
    use age::secrecy::ExposeSecret;
    VaultFile::save(path, passphrase.expose_secret(), document)
}

fn parse_env(contents: &str, project: &str) -> Vec<EntryInput> {
    let mut result = Vec::new();
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !valid_env_key(key) {
            continue;
        }
        let secret = unquote(raw_value.trim());
        if secret.is_empty() {
            continue;
        }
        result.push(EntryInput {
            id: None,
            name: key.to_owned(),
            kind: "environment".to_owned(),
            project: project.to_owned(),
            environment: "development".to_owned(),
            username: String::new(),
            secret,
            url: String::new(),
            notes: "从 .env 文件导入".to_owned(),
            tags: vec!["imported".to_owned()],
            enabled: true,
        });
    }
    result
}

fn valid_env_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn unquote(value: &str) -> String {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[value.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return value[1..value.len() - 1].to_owned();
        }
    }
    value.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_env_entries() {
        let entries = parse_env(
            "# comment\nexport API_KEY=\"secret value\"\nEMPTY=\nINVALID-KEY=no\nURL=https://example.com\n",
            "demo",
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "API_KEY");
        assert_eq!(entries[0].secret, "secret value");
        assert_eq!(entries[1].name, "URL");
        assert_eq!(entries[1].project, "demo");
    }

    #[test]
    fn unquote_preserves_unquoted_values() {
        assert_eq!(unquote("plain"), "plain");
        assert_eq!(unquote("'single'"), "single");
        assert_eq!(unquote("\"double\""), "double");
    }
}
