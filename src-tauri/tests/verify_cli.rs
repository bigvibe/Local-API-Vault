//! `verify` 的端到端测试：真的跑一次 CLI，检查退出码和输出。
//!
//! 重点验证两件事：
//! 1. 退出码可用（0 = 没问题，1 = 有问题）
//! 2. 输出里永远不会出现条目名或密钥值

use age::secrecy::ExposeSecret;
use local_api_vault_lib::model::SecretEntry;
use local_api_vault_lib::vault::VaultFile;
use std::process::Command;
use tempfile::tempdir;

const PASSPHRASE: &str = "passphrase with enough entropy";

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_local-api-vault-cli"))
}

fn secret_entry(secret: &str) -> SecretEntry {
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
fn a_healthy_legacy_vault_exits_zero() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("vault.age");
    VaultFile::create(&path, PASSPHRASE).unwrap();

    let output = cli()
        .arg("verify")
        .arg("--vault")
        .arg(&path)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "应当以 0 退出：{stdout}");
    assert!(stdout.contains("通过"), "{stdout}");
    assert!(stdout.contains("旧格式没有恢复身份"), "{stdout}");
    assert!(!stdout.contains(PASSPHRASE), "输出里出现了主密码：{stdout}");
}

#[test]
fn a_missing_vault_exits_one() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("nowhere.age");

    let output = cli()
        .arg("verify")
        .arg("--vault")
        .arg(&path)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "应当以非 0 退出：{stdout}");
    assert!(stdout.contains("失败"), "{stdout}");
}

#[test]
fn an_identity_backed_vault_never_prints_values() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("vault.age");
    let identity = age::x25519::Identity::generate();
    VaultFile::create_with_recovery(&path, PASSPHRASE, &identity).unwrap();

    let mut document = VaultFile::load_with_identity(&path, &identity).unwrap();
    document.entries.push(secret_entry("super-secret-value"));
    VaultFile::save(&path, PASSPHRASE, &document).unwrap();

    let identity_path = directory.path().join("recovery.key");
    let secret = identity.to_string();
    VaultFile::write_recovery_identity(&identity_path, secret.expose_secret()).unwrap();

    let output = cli()
        .arg("verify")
        .arg("--vault")
        .arg(&path)
        .arg("--identity")
        .arg(&identity_path)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "应当以 0 退出：{stdout}");
    assert!(stdout.contains("用恢复身份打开 Vault"), "{stdout}");
    assert!(stdout.contains("1 个条目"), "{stdout}");
    assert!(
        !stdout.contains("super-secret-value"),
        "泄漏了密钥值：{stdout}"
    );
    assert!(!stdout.contains("DEMO_KEY"), "泄漏了条目名：{stdout}");
}
