//! `lvault verify` — 只读校验当前 Vault 完整性。
//!
//! 目的：让人能一眼看出「这个 Vault 还是好的吗」，不用打开应用、不用主密码。
//!
//! 合同（与 README / AGENTS.md 一致）：
//!
//! - **只读**：不写任何文件，不改 Vault，不动 sidecar
//! - **不打印任何密钥值**：最多报条目数量、运行配置数量
//! - **不索要主密码**：口令加密的 sidecar 只查格式，不查内容
//! - 只有显式给了 `--identity`（恢复身份文件）才会真的解密一次，
//!   目的是回答「这份恢复身份到底能不能打开这个 Vault」
//! - 全程本地，不联网、不写日志

use crate::crypto;
use crate::vault::{VaultError, VaultFile};
use std::fs;
use std::path::{Path, PathBuf};
use zeroize::Zeroize;

const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_IDENTITY_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Fail,
    Skip,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Ok => "通过",
            Status::Warn => "注意",
            Status::Fail => "失败",
            Status::Skip => "跳过",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Check {
    pub label: String,
    pub status: Status,
    pub detail: Option<String>,
}

impl Check {
    fn ok(label: &str, detail: Option<String>) -> Self {
        Self {
            label: label.to_owned(),
            status: Status::Ok,
            detail,
        }
    }

    fn warn(label: &str, detail: Option<String>) -> Self {
        Self {
            label: label.to_owned(),
            status: Status::Warn,
            detail,
        }
    }

    fn fail(label: &str, detail: Option<String>) -> Self {
        Self {
            label: label.to_owned(),
            status: Status::Fail,
            detail,
        }
    }

    fn skip(label: &str, detail: Option<String>) -> Self {
        Self {
            label: label.to_owned(),
            status: Status::Skip,
            detail,
        }
    }
}

/// 只统计数量，绝不包含条目名或条目值。
#[derive(Debug, Default, Clone, Copy)]
pub struct VaultFacts {
    pub entries: Option<usize>,
    pub disabled: Option<usize>,
    pub profiles: Option<usize>,
}

#[derive(Debug)]
pub struct VerifyReport {
    pub vault_path: PathBuf,
    pub checks: Vec<Check>,
    pub facts: VaultFacts,
}

impl VerifyReport {
    pub fn has_failure(&self) -> bool {
        self.checks.iter().any(|check| check.status == Status::Fail)
    }
}

/// 校验 Vault 文件的完整性。永远不会修改任何文件。
pub fn verify(vault_path: &Path, identity_path: Option<&Path>) -> VerifyReport {
    let mut checks: Vec<Check> = Vec::new();
    let mut facts = VaultFacts::default();

    let bytes = match read_bounded(vault_path, MAX_FILE_BYTES) {
        Ok(bytes) => bytes,
        Err(problem) => {
            checks.push(Check::fail("Vault 文件可读", Some(problem.describe())));
            return finish(vault_path, checks, facts);
        }
    };
    checks.push(Check::ok(
        "Vault 文件可读",
        Some(format!("{} 字节", bytes.len())),
    ));

    let passphrase_only = match age::Decryptor::new_buffered(&bytes[..]) {
        Ok(decryptor) => decryptor.is_scrypt(),
        Err(_) => {
            checks.push(Check::fail(
                "Vault 文件格式有效",
                Some("不是有效的 age 文件：文件可能已损坏，或不是本工具生成的".to_owned()),
            ));
            return finish(vault_path, checks, facts);
        }
    };

    if passphrase_only {
        checks.push(Check::ok(
            "Vault 文件格式有效",
            Some("age v1，口令加密（旧格式）".to_owned()),
        ));
        checks.push(Check::warn(
            "旧格式没有恢复身份",
            Some("忘记主密码只能重建空 Vault，迁移到身份格式后才可恢复".to_owned()),
        ));
        if VaultFile::identity_path(vault_path).exists() {
            checks.push(Check::warn(
                "口令 sidecar 与 Vault 不匹配",
                Some("存在 sidecar，但 Vault 是口令格式，sidecar 不起作用".to_owned()),
            ));
        }
        checks.push(Check::skip(
            "解密校验未执行",
            Some("口令格式的 Vault 需要主密码才能解开，verify 不索要主密码".to_owned()),
        ));
        return finish(vault_path, checks, facts);
    }

    checks.push(Check::ok(
        "Vault 文件格式有效",
        Some("age v1，X25519 身份加密".to_owned()),
    ));

    let sidecar_path = VaultFile::identity_path(vault_path);
    let mut sidecar_ok = false;
    match read_bounded(&sidecar_path, MAX_FILE_BYTES) {
        Ok(sidecar) => {
            if crypto::is_scrypt_encrypted(&sidecar) {
                sidecar_ok = true;
                checks.push(Check::ok(
                    "口令 sidecar 存在且格式有效",
                    Some(
                        sidecar_path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    ),
                ));
            } else {
                checks.push(Check::fail(
                    "口令 sidecar 格式有效",
                    Some("sidecar 应该是口令加密的文件，现在不是".to_owned()),
                ));
            }
        }
        Err(problem) => {
            let detail = match problem {
                FileProblem::Missing => {
                    "缺少口令 sidecar（vault.identity.age）：没有它就只能靠恢复身份打开 Vault"
                        .to_owned()
                }
                other => format!("{}：没有它就只能靠恢复身份打开 Vault", other.describe()),
            };
            checks.push(Check::fail("口令 sidecar 存在", Some(detail)));
        }
    }

    match identity_path {
        None => {
            checks.push(Check::skip(
                "解密校验未执行",
                Some(
                    "未提供恢复身份文件；加 --identity <恢复身份文件> 可验证「能不能真的打开」"
                        .to_owned(),
                ),
            ));
        }
        Some(path) => match read_identity(path) {
            Err(reason) => {
                checks.push(Check::fail("恢复身份文件可用", Some(reason)));
            }
            Ok(identity) => match VaultFile::load_with_identity(vault_path, &identity) {
                Ok(document) => {
                    let entries = document.entries.len();
                    let disabled = document
                        .entries
                        .iter()
                        .filter(|entry| !entry.enabled)
                        .count();
                    let profiles = document.profiles.len();
                    facts.entries = Some(entries);
                    facts.disabled = Some(disabled);
                    facts.profiles = Some(profiles);
                    checks.push(Check::ok(
                        "用恢复身份打开 Vault",
                        Some(format!(
                            "{entries} 个条目，{disabled} 个已停用，{profiles} 个运行配置"
                        )),
                    ));
                }
                Err(error) => {
                    checks.push(Check::fail(
                        "用恢复身份打开 Vault",
                        Some(match error {
                            VaultError::NotFound => "Vault 文件不存在".to_owned(),
                            VaultError::UnsupportedVersion => {
                                "Vault 版本不受支持，需要升级本工具".to_owned()
                            }
                            VaultError::InvalidData => {
                                "Vault 内容不是有效文档（JSON 解析或字段校验失败）".to_owned()
                            }
                            VaultError::InvalidPassphrase => {
                                "恢复身份与这个 Vault 不匹配，或文件已损坏".to_owned()
                            }
                            VaultError::AlreadyExists => "Vault 文件状态异常".to_owned(),
                            VaultError::Io(_) => "读取失败".to_owned(),
                        }),
                    ));
                }
            },
        },
    }

    if sidecar_ok {
        checks.push(Check::warn(
            "口令 sidecar 的内容未校验",
            Some("解开它需要主密码，verify 不索要主密码".to_owned()),
        ));
    }

    finish(vault_path, checks, facts)
}

fn finish(vault_path: &Path, checks: Vec<Check>, facts: VaultFacts) -> VerifyReport {
    VerifyReport {
        vault_path: vault_path.to_path_buf(),
        checks,
        facts,
    }
}

/// 把报告渲染成人能读、机器也能解析的文本。
///
/// 这里只输出数量和状态，**不输出任何条目名或条目值**。
pub fn render(report: &VerifyReport) -> String {
    let mut text = format!("Vault 校验：{}\n", report.vault_path.display());
    for check in &report.checks {
        text.push_str(&format!("[{}] {}", check.status.label(), check.label));
        if let Some(detail) = &check.detail {
            text.push_str(&format!("：{detail}"));
        }
        text.push('\n');
    }
    text.push_str(&format!("\n结论：{}\n", conclusion(report)));
    text
}

fn conclusion(report: &VerifyReport) -> String {
    if report.has_failure() {
        return "发现问题，请按上面的「失败」项处理。".to_owned();
    }
    if report.facts.entries.is_some() {
        return "Vault 完好，已用恢复身份实际打开验证。".to_owned();
    }
    "文件层面正常，但没做解密校验；加 --identity <恢复身份文件> 可完整验证。".to_owned()
}

/// 读文件失败的原因。分开报，错误信息才说人话。
enum FileProblem {
    Missing,
    NotAFile,
    TooLarge(u64),
    Io(String),
}

impl FileProblem {
    fn describe(&self) -> String {
        match self {
            FileProblem::Missing => "文件不存在".to_owned(),
            FileProblem::NotAFile => "不是一个普通文件".to_owned(),
            FileProblem::TooLarge(limit) => format!("文件超过 {limit} 字节上限"),
            FileProblem::Io(error) => format!("读取失败：{error}"),
        }
    }
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, FileProblem> {
    let metadata = fs::metadata(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => FileProblem::Missing,
        _ => FileProblem::Io(error.to_string()),
    })?;
    if !metadata.is_file() {
        return Err(FileProblem::NotAFile);
    }
    if metadata.len() > limit {
        return Err(FileProblem::TooLarge(limit));
    }
    fs::read(path).map_err(|error| FileProblem::Io(error.to_string()))
}

fn read_identity(path: &Path) -> Result<age::x25519::Identity, String> {
    let bytes = read_bounded(path, MAX_IDENTITY_BYTES).map_err(|problem| problem.describe())?;
    let mut contents =
        String::from_utf8(bytes).map_err(|_| "恢复身份文件不是 UTF-8 文本".to_owned())?;
    let result = VaultFile::parse_recovery_identity(&contents)
        .map_err(|_| "不是有效的恢复身份文件（应包含一行 AGE-SECRET-KEY-1…）".to_owned());
    contents.zeroize();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SecretEntry;
    use crate::vault::VaultFile;
    use age::secrecy::ExposeSecret;
    use tempfile::tempdir;

    fn entry_with_secret(secret: &str) -> SecretEntry {
        SecretEntry {
            id: "entry-id".into(),
            name: "DEMO_KEY".into(),
            kind: "api-key".into(),
            project: "app".into(),
            environment: "development".into(),
            username: String::new(),
            secret: secret.to_owned(),
            url: String::new(),
            notes: String::new(),
            tags: vec![],
            enabled: true,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn missing_vault_is_a_failure() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let report = verify(&path, None);
        assert!(report.has_failure());
        assert!(render(&report).contains("失败"));
    }

    #[test]
    fn garbage_file_is_not_an_age_file() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        fs::write(&path, b"this is not an age file").unwrap();
        let report = verify(&path, None);
        assert!(report.has_failure());
        assert!(render(&report).contains("不是有效的 age 文件"));
    }

    #[test]
    fn legacy_vault_warns_but_does_not_fail() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        VaultFile::create(&path, "passphrase with enough entropy").unwrap();
        let report = verify(&path, None);
        assert!(!report.has_failure());
        let text = render(&report);
        assert!(text.contains("旧格式没有恢复身份"));
        assert!(text.contains("跳过"));
    }

    #[test]
    fn identity_backed_vault_without_identity_only_skips() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let identity = age::x25519::Identity::generate();
        VaultFile::create_with_recovery(&path, "passphrase with enough entropy", &identity)
            .unwrap();

        let report = verify(&path, None);
        assert!(!report.has_failure());
        assert!(report.facts.entries.is_none());
        assert!(render(&report).contains("解密校验未执行"));
    }

    #[test]
    fn missing_sidecar_is_a_failure() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let identity = age::x25519::Identity::generate();
        VaultFile::create_with_recovery(&path, "passphrase with enough entropy", &identity)
            .unwrap();
        fs::remove_file(VaultFile::identity_path(&path)).unwrap();

        let report = verify(&path, None);
        assert!(report.has_failure());
        assert!(render(&report).contains("口令 sidecar 存在"));
    }

    #[test]
    fn recovery_identity_that_opens_the_vault_passes() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let identity = age::x25519::Identity::generate();
        VaultFile::create_with_recovery(&path, "passphrase with enough entropy", &identity)
            .unwrap();

        let identity_path = directory.path().join("recovery.key");
        let secret = identity.to_string();
        VaultFile::write_recovery_identity(&identity_path, secret.expose_secret()).unwrap();

        let report = verify(&path, Some(&identity_path));
        assert!(!report.has_failure());
        assert_eq!(report.facts.entries, Some(0));
        assert_eq!(report.facts.profiles, Some(0));
        assert!(render(&report).contains("用恢复身份打开 Vault"));
    }

    #[test]
    fn wrong_recovery_identity_fails() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let identity = age::x25519::Identity::generate();
        VaultFile::create_with_recovery(&path, "passphrase with enough entropy", &identity)
            .unwrap();

        let other = age::x25519::Identity::generate();
        let identity_path = directory.path().join("recovery.key");
        let secret = other.to_string();
        VaultFile::write_recovery_identity(&identity_path, secret.expose_secret()).unwrap();

        let report = verify(&path, Some(&identity_path));
        assert!(report.has_failure());
        assert!(render(&report).contains("不匹配"));
    }

    #[test]
    fn report_never_contains_entry_names_or_values() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let identity = age::x25519::Identity::generate();
        VaultFile::create_with_recovery(&path, "passphrase with enough entropy", &identity)
            .unwrap();

        let mut document = VaultFile::load_with_identity(&path, &identity).unwrap();
        document
            .entries
            .push(entry_with_secret("super-secret-value"));
        VaultFile::save(&path, "passphrase with enough entropy", &document).unwrap();

        let identity_path = directory.path().join("recovery.key");
        let secret = identity.to_string();
        VaultFile::write_recovery_identity(&identity_path, secret.expose_secret()).unwrap();

        let report = verify(&path, Some(&identity_path));
        let text = render(&report);
        assert!(!text.contains("super-secret-value"), "泄漏了密钥值：{text}");
        assert!(!text.contains("DEMO_KEY"), "泄漏了条目名：{text}");
        assert_eq!(report.facts.entries, Some(1));
    }
}
