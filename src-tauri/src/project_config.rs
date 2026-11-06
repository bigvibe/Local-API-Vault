//! `.lvault.toml` — 项目级命令行参数文件。
//!
//! 格式刻意极简：
//!
//! ```toml
//! vars = ["NEWSFEED_API_KEY", "OAK_PRIVATE_JWT"]
//! ```
//!
//! 设计合同（由 README / AGENTS.md 维护）：
//!
//! - **文件里只有环境变量名**——绝无值，绝无密钥本身。
//!   value 过不了 env name 校验，所以 `vars = ["KEY=sk-..."]` 看似未来可能，现在就会报错。
//! - 提交进仓库即随项目走，`lvault run -- <command>` 自动一并注入。
//! - **只有顶层 `vars` 是合法键**；任何 `[section]`、任何其它顶层键都被拒绝。
//!   这样误写普通 TOML 配置不会被静默接受——出错时报错误，而不是半懂不懂。

use std::fs;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE_NAME: &str = ".lvault.toml";
const MAX_CONFIG_BYTES: u64 = 16 * 1024;
const MAX_VARS: usize = 64;
const MAX_NAME_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectConfig {
    pub vars: Vec<String>,
    /// Path the config was loaded from, shown in error messages for context.
    pub source: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectConfigError {
    #[error("当前目录没有 {CONFIG_FILE_NAME}（或读取失败）：{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
}

/// Locate and parse `.lvault.toml` directly inside `dir`.
///
/// Discovery is intentionally shallow (one directory, no walking up); this is
/// the same model as `.env` being found in the project root, and avoids
/// leaking the parent Vault onto a shell opened outside the project.
pub fn load_project_config(dir: &Path) -> Result<ProjectConfig, ProjectConfigError> {
    let path = dir.join(CONFIG_FILE_NAME);
    let metadata =
        fs::metadata(&path).map_err(|error| ProjectConfigError::NotFound(error.to_string()))?;
    if !metadata.is_file() {
        return Err(ProjectConfigError::NotFound("不是普通文件".to_owned()));
    }
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err(ProjectConfigError::Invalid(format!(
            "{CONFIG_FILE_NAME} 超过 {} KB 上限",
            MAX_CONFIG_BYTES / 1024
        )));
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| ProjectConfigError::NotFound(error.to_string()))?;
    parse_config(&contents, &path)
}

/// Parse the config from raw TOML text — used by the loader and heavily tested.
pub fn parse_config(contents: &str, source: &Path) -> Result<ProjectConfig, ProjectConfigError> {
    let parsed: toml::Value = toml::from_str(contents)
        .map_err(|error| ProjectConfigError::Invalid(format!(".lvault.toml 语法错误：{error}")))?;

    let table = parsed
        .as_table()
        .ok_or_else(|| ProjectConfigError::Invalid(".lvault.toml 不是 TOML 表".to_owned()))?;

    // Only one legal top-level key — anything else means this is not our file.
    if let Some(unknown) = table.keys().find(|key| key.as_str() != "vars") {
        return Err(ProjectConfigError::Invalid(format!(
            ".lvault.toml 只允许顶层键 `vars`，发现多余键 `{unknown}`——{}",
            mk_note(unknown)
        )));
    }

    let vars = table
        .get("vars")
        .ok_or_else(|| ProjectConfigError::Invalid(".lvault.toml 缺少顶层键 `vars`".to_owned()))?
        .as_array()
        .ok_or_else(|| ProjectConfigError::Invalid("`vars` 必须是字符串数组".to_owned()))?
        .iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                ProjectConfigError::Invalid("`vars` 的每一项必须是字符串".to_owned())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    if vars.is_empty() {
        return Err(ProjectConfigError::Invalid(
            "`vars` 至少要有一个环境变量名".to_owned(),
        ));
    }
    if vars.len() > MAX_VARS {
        return Err(ProjectConfigError::Invalid(format!(
            "`vars` 最多 {} 个条目",
            MAX_VARS
        )));
    }

    let mut seen = std::collections::HashSet::new();
    let mut names = Vec::with_capacity(vars.len());
    for raw in vars {
        let name = raw.trim();
        if name.is_empty() {
            return Err(ProjectConfigError::Invalid(
                "`vars` 含有空字符串".to_owned(),
            ));
        }
        if name != raw {
            return Err(ProjectConfigError::Invalid(
                "`vars` 有一项前后带空白".to_owned(),
            ));
        }
        if !is_environment_name(name) {
            return Err(ProjectConfigError::Invalid(
                "`vars` 有一项不是合法的环境变量名（只允许字母/数字/下划线，首字符不是数字）。这里只写名字，不要写值。"
                    .to_owned(),
            ));
        }
        if name.len() > MAX_NAME_LEN {
            return Err(ProjectConfigError::Invalid(format!(
                "`vars` 有一项超过 {MAX_NAME_LEN} 字符上限"
            )));
        }
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(ProjectConfigError::Invalid(
                "`vars` 有重复（忽略大小写）".to_owned(),
            ));
        }
        names.push(name.to_owned());
    }

    Ok(ProjectConfig {
        vars: names,
        source: source.to_path_buf(),
    })
}

fn mk_note(key: &str) -> &'static str {
    // An empty section header like [vault] usually means "a vault file,
    // but not ours" — tell the user that, or they will waste time.
    match key {
        "vault" | "secret" | "key" | "token" => "看起来你写的是值/密钥，不是条目名，请删掉",
        _ => "本文件不含任何密钥值，删除该键即可",
    }
}

fn is_environment_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<ProjectConfig, ProjectConfigError> {
        parse_config(text, Path::new(".lvault.toml"))
    }

    #[test]
    fn accepts_a_minimal_config() {
        let cfg = parse("vars = [\"NEWSFEED_API_KEY\", \"OAK_PRIVATE_JWT\"]").unwrap();
        assert_eq!(cfg.vars, vec!["NEWSFEED_API_KEY", "OAK_PRIVATE_JWT"]);
    }

    #[test]
    fn accepts_trailing_comma_and_comments() {
        let text = "# 只存条目名，不放值\nvars = [\n  \"A_KEY\",\n  \"B_TOKEN\",\n]\n";
        let cfg = parse(text).unwrap();
        assert_eq!(cfg.vars, vec!["A_KEY", "B_TOKEN"]);
    }

    #[test]
    fn rejects_an_empty_vars_array() {
        let err = parse("vars = []").unwrap_err();
        assert!(err.to_string().contains("至少"));
    }

    #[test]
    fn rejects_missing_vars() {
        let err = parse("names = [\"A_KEY\"]").unwrap_err();
        assert!(err.to_string().contains("`vars`"));
    }

    #[test]
    fn rejects_unknown_top_level_key() {
        let err = parse("vars = [\"A_KEY\"]\nother = \"x\"").unwrap_err();
        assert!(err.to_string().contains("other"));
    }

    #[test]
    fn rejects_sections() {
        let err = parse("[vault]\nvars = [\"A_KEY\"]").unwrap_err();
        assert!(err.to_string().contains("vault"));
    }

    #[test]
    fn rejects_values_masquerading_as_names() {
        for hostile in [
            "vars = [\"NEWSFEED_API_KEY=sk-live-12345\"]",
            "vars = [\"Authorization: Bearer x\"]",
            "vars = [\"foo bar\"]",
            "vars = [123]",
            "vars = [{name = \"A\"}]",
        ] {
            assert!(parse(hostile).is_err(), "必须拒绝: {hostile}");
        }
    }

    #[test]
    fn rejects_duplicates_case_insensitively() {
        let err = parse("vars = [\"MY_KEY\", \"my_key\"]").unwrap_err();
        assert!(err.to_string().contains("重复"));
    }

    #[test]
    fn rejects_over_the_limit() {
        let names = (0..65)
            .map(|i| format!("\"K{i}\""))
            .collect::<Vec<_>>()
            .join(",");
        let text = format!("vars = [{names}]");
        let err = parse(&text).unwrap_err();
        assert!(err.to_string().contains("64"));
    }

    #[test]
    fn rejects_whitespace_inside_names() {
        assert!(parse("vars = [\"MY_KEY \"]").is_err());
    }

    #[test]
    fn error_messages_never_echo_secret_data() {
        let err = parse("vars = [\"super-secret-value\"]").unwrap_err();
        assert!(
            !err.to_string().contains("super-secret-value"),
            "错误信息泄漏了输入内容：{err}"
        );
    }

    #[test]
    fn accepts_64_entries_exactly() {
        let names = (0..64)
            .map(|i| format!("\"K{i}\""))
            .collect::<Vec<_>>()
            .join(",");
        let text = format!("vars = [{names}]");
        assert!(parse(&text).is_ok(), "64 项应当被接受：{text}");
    }
}
