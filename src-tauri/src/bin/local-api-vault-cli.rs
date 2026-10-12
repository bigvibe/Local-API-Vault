use local_api_vault_lib::{
    bridge::{self, ClientError},
    vault::{default_vault_path, VaultFile},
};
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use zeroize::Zeroize;

/// Turn a bridge failure into something a person can act on.
///
/// The wire `message` is English and stable (it is part of the client contract).
/// Here we localise it and, more importantly, say what to DO next — a bare
/// "Vault is locked" reads as "the software is broken" to someone who does not
/// know the vault has to be unlocked first.
fn explain(error: &ClientError) -> String {
    let mut text = match error.code.as_str() {
        "vault_locked" => {
            "Vault 已锁定。请点击系统托盘中的 Local API Vault 图标，打开窗口后输入主密码解锁，再重试。".to_owned()
        }
        "bridge_unavailable" => {
            "无法连接到 Local API Vault。请确认应用正在运行（托盘中有图标）。".to_owned()
        }
        "disabled" => {
            "该密钥已被停用，无法读取。如需使用，请打开应用，在条目上点击「启用」。".to_owned()
        }
        "not_found" => "没有找到这个密钥条目。".to_owned(),
        "invalid_request" => "请求格式有误。".to_owned(),
        _ => error.message.clone(),
    };
    if !error.suggestions.is_empty() {
        text.push_str(&format!(
            "\n  你是指这些条目吗：{}",
            error.suggestions.join("、")
        ));
        text.push_str("\n  用 lvault 或应用里的确切名称重试。");
    }
    text
}

fn usage() -> &'static str {
    "用法:\n  local-api-vault-cli get <name> --vault <path>\n  local-api-vault-cli list --vault <path>\n  local-api-vault-cli pipe-get <name> [--project <name>] [--environment <name>]\n  local-api-vault-cli run <profile>\n  local-api-vault-cli run [--cwd <path>] --secret <NAME> [--project <name>] [--environment <name>] -- <command>\n\n主密码只通过安全提示读取，不会作为命令行参数传入。\n停用条目不会出现在 list 中，也不能通过 get 或 pipe-get 读取。\nrun 通过已解锁的 Windows Named Pipe 注入环境变量，不生成 .env。"
}

fn main() {
    if let Err(error) = run() {
        eprintln!("错误: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(|| usage().to_owned())?;
    if command == "--help" || command == "-h" {
        println!("{}", usage());
        return Ok(());
    }

    if command == "run" {
        return run_with_pipe(&mut args);
    }

    let mut vault = None;
    let mut name = None;
    let mut project = None;
    let mut environment = None;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--vault" => vault = args.next(),
            "--project" => project = args.next(),
            "--environment" => environment = args.next(),
            value if name.is_none() && !value.starts_with("--") => name = Some(value.to_owned()),
            _ => return Err(format!("未知参数: {argument}\n\n{}", usage())),
        }
    }

    if command == "pipe-get" {
        let name = name.ok_or_else(|| "pipe-get 命令需要一个密钥名称。".to_owned())?;
        let value = bridge::request_secret(&name, project.as_deref(), environment.as_deref())
            .map_err(|error| explain(&error))?;
        print!("{value}");
        io::stdout()
            .flush()
            .map_err(|_| "无法写入标准输出。".to_owned())?;
        return Ok(());
    }

    let vault =
        PathBuf::from(vault.unwrap_or_else(|| default_vault_path().to_string_lossy().into_owned()));

    let mut passphrase =
        rpassword::prompt_password("主密码: ").map_err(|_| "无法读取主密码。".to_owned())?;
    let loaded = VaultFile::load(&vault, &passphrase);
    passphrase.zeroize();
    let document = loaded.map_err(|error| error.to_string())?;

    match command.as_str() {
        "list" => {
            for entry in &document.entries {
                if !entry.enabled {
                    continue;
                }
                let project = if entry.project.is_empty() {
                    "-"
                } else {
                    entry.project.as_str()
                };
                let environment = if entry.environment.is_empty() {
                    "-"
                } else {
                    entry.environment.as_str()
                };
                println!("{}\t{}\t{}", entry.name, project, environment);
            }
        }
        "get" => {
            let name = name.ok_or_else(|| "get 命令需要一个密钥名称。".to_owned())?;
            let entry = document
                .entries
                .iter()
                .find(|entry| entry.name == name.as_str())
                .ok_or_else(|| format!("找不到密钥: {name}"))?;
            if !entry.enabled {
                return Err(format!("密钥已停用: {name}"));
            }
            print!("{}", entry.secret);
            io::stdout()
                .flush()
                .map_err(|_| "无法写入标准输出。".to_owned())?;
        }
        _ => return Err(format!("未知命令: {command}\n\n{}", usage())),
    }
    Ok(())
}

fn run_with_pipe<I>(args: &mut I) -> Result<(), String>
where
    I: Iterator<Item = String>,
{
    let arguments: Vec<String> = args.by_ref().collect();
    if let Some(first) = arguments.first() {
        if !first.starts_with('-') {
            if arguments.len() > 1 {
                return Err("保存的运行配置后不需要额外参数。".to_owned());
            }
            return run_saved_profile(first);
        }
    }

    let mut args = arguments.into_iter();
    let mut working_directory = None;
    let mut project = None;
    let mut environment = None;
    let mut secret_names = Vec::new();
    let mut command_parts = Vec::new();
    let mut after_separator = false;

    while let Some(argument) = args.next() {
        if after_separator {
            command_parts.push(argument);
            continue;
        }
        match argument.as_str() {
            "--" => after_separator = true,
            "--cwd" => working_directory = Some(next_value(&mut args, "--cwd")?),
            "--project" => project = Some(next_value(&mut args, "--project")?),
            "--environment" => environment = Some(next_value(&mut args, "--environment")?),
            "--secret" | "--env" => {
                let name = next_value(&mut args, "--secret")?;
                if !is_environment_name(&name) {
                    return Err(format!("不是合法的环境变量名: {name}"));
                }
                if secret_names.iter().any(|existing| existing == &name) {
                    return Err(format!("重复的密钥名称: {name}"));
                }
                secret_names.push(name);
            }
            _ => return Err(format!("未知参数: {argument}\n\n{}", usage())),
        }
    }

    if !after_separator || command_parts.is_empty() {
        return Err("run 需要 -- <command>，例如 -- npm run dev".to_owned());
    }
    if secret_names.is_empty() {
        return Err("run 至少需要一个 --secret <NAME>。".to_owned());
    }

    let working_directory = match working_directory {
        Some(path) => path,
        None => std::env::current_dir()
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| error.to_string())?,
    };
    let command_line = command_parts.join(" ");
    execute_run(
        &working_directory,
        &command_line,
        &secret_names,
        project.as_deref(),
        environment.as_deref(),
    )
}

fn run_saved_profile(name: &str) -> Result<(), String> {
    let profile = bridge::request_profile(name).map_err(|error| explain(&error))?;
    execute_run(
        &profile.working_directory,
        &profile.command,
        &profile.secrets,
        Some(&profile.project),
        Some(&profile.environment),
    )
}

fn execute_run(
    working_directory: &str,
    command_line: &str,
    secret_names: &[String],
    project: Option<&str>,
    environment: Option<&str>,
) -> Result<(), String> {
    if command_line.trim().is_empty() || secret_names.is_empty() {
        return Err("运行配置缺少命令或密钥。".to_owned());
    }
    let working_directory = PathBuf::from(working_directory);
    let mut resolved = Vec::with_capacity(secret_names.len());
    for name in secret_names {
        let value =
            bridge::request_secret(name, project, environment).map_err(|error| explain(&error))?;
        resolved.push((name.clone(), value));
    }

    let mut process = if cfg!(windows) {
        let mut process = Command::new("cmd");
        process.args(["/C", command_line]);
        process
    } else {
        let mut process = Command::new("sh");
        process.args(["-c", command_line]);
        process
    };
    process.current_dir(&working_directory);
    for (name, value) in &resolved {
        process.env(name, value);
    }

    let result = process.status().map_err(|error| error.to_string());
    for (_, value) in &mut resolved {
        value.zeroize();
    }
    let status = result?;
    if !status.success() {
        return Err(format!("命令退出码: {}", status.code().unwrap_or(-1)));
    }
    Ok(())
}

fn next_value<I>(args: &mut I, option: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    args.next().ok_or_else(|| format!("{option} 缺少参数。"))
}

fn is_environment_name(name: &str) -> bool {
    let mut characters = name.chars();
    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}
