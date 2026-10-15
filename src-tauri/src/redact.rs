//! Builds a share-safe "reference" summary of a single entry.
//!
//! The point of this module is that the *user* never has to decide what counts
//! as sensitive. Everything credential-shaped is masked here, deliberately
//! erring toward over-masking: hiding too much is a cosmetic problem, while
//! leaking a real key is a security incident.
//!
//! Labels (name, kind, project, environment, tags) are kept verbatim because a
//! developer needs them to identify the entry. Free-form fields — notes and
//! URL — are redacted, since that is where people actually paste credentials.

use crate::model::SecretEntry;

/// Placeholder substituted for anything that could be a credential.
pub const REDACTED: &str = "<已隐藏>";

/// URL query keys whose value is always masked. Matched as substrings so
/// `access_token`, `apiKey` and `pwd` are all caught.
const SENSITIVE_KEY_FRAGMENTS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "pwd",
    "apikey",
    "api_key",
    "access_key",
    "private",
    "credential",
    "session",
    "auth",
    "signature",
];

/// `key = value` pairs whose value is masked in notes. `authorization` is
/// handled separately because its value runs to the end of the line.
const ASSIGNMENT_KEYS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "pwd",
    "api_key",
    "apikey",
    "access_key",
    "access_token",
    "authorization",
    "auth",
    "client_secret",
    "private_key",
];

/// Longest run of `[A-Za-z0-9_-]` treated as a bare credential.
const BLOB_MIN_LEN: usize = 24;

/// Render an entry as Markdown that is safe to paste into a chat, an issue, or
/// a commit message.
pub fn entry_reference(entry: &SecretEntry) -> String {
    let mut out = String::new();

    out.push_str(&format!("# {}\n\n", entry.name));
    out.push_str(&format!("- 类型：{}\n", entry.kind));
    if !entry.project.is_empty() {
        out.push_str(&format!("- 项目：{}\n", entry.project));
    }
    if !entry.environment.is_empty() {
        out.push_str(&format!("- 环境：{}\n", entry.environment));
    }
    out.push_str(&format!("- 密钥：`{REDACTED}`\n"));
    if !entry.username.is_empty() {
        out.push_str(&format!("- 登录账号：`{REDACTED}`\n"));
    }
    if !entry.url.is_empty() {
        out.push_str(&format!("- 地址：{}\n", redact_url(&entry.url)));
    }
    if !entry.tags.is_empty() {
        out.push_str(&format!("- 标签：{}\n", entry.tags.join("、")));
    }

    if !entry.notes.trim().is_empty() {
        out.push_str("\n## 备注\n\n");
        out.push_str(&redact_notes(&entry.notes));
        out.push('\n');
    }

    out.push_str("\n<!-- 密钥与账号已隐藏。 -->\n");
    out
}

/// Strip sensitive query parameters from a URL and mask credential-shaped
/// material in the remainder.
pub fn redact_url(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return mask_blobs(url);
    };

    let kept: Vec<String> = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, _)) if is_sensitive_key(key) => format!("{key}={REDACTED}"),
            _ => pair.to_owned(),
        })
        .collect();

    if kept.is_empty() {
        mask_blobs(base)
    } else {
        mask_blobs(&format!("{base}?{}", kept.join("&")))
    }
}

/// Mask credential-looking content inside free-form notes while leaving the
/// surrounding documentation intact.
pub fn redact_notes(notes: &str) -> String {
    mask_blobs(&mask_bearer_tokens(&mask_assignments(notes)))
}

fn is_sensitive_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    SENSITIVE_KEY_FRAGMENTS
        .iter()
        .any(|fragment| lowered.contains(fragment))
}

/// Replace `token=xxx`, `api_key: xxx` and similar assignments, keeping the key
/// name so the shape of the documentation survives.
fn mask_assignments(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;

    while index < chars.len() {
        match match_assignment(&chars, index) {
            Some(end) => {
                out.push_str(REDACTED);
                index = end;
            }
            None => {
                out.push(chars[index]);
                index += 1;
            }
        }
    }

    out
}

/// If a sensitive key starts at `index` and is followed by `=`, `:` or, for
/// `authorization`, by anything at all, return the index just past its value.
fn match_assignment(chars: &[char], index: usize) -> Option<usize> {
    for key in ASSIGNMENT_KEYS {
        let key_len = key.chars().count();
        if index + key_len > chars.len() {
            continue;
        }
        let slice: String = chars[index..index + key_len].iter().collect();
        if !slice.eq_ignore_ascii_case(key) {
            continue;
        }

        // The key must stand on its own, not be part of a longer word.
        if index > 0 && is_word_char(chars[index - 1]) {
            continue;
        }
        if chars[index + key_len..]
            .first()
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        {
            continue;
        }

        let mut cursor = index + key_len;
        while matches!(chars.get(cursor), Some(' ') | Some('\t')) {
            cursor += 1;
        }

        // `Authorization: Bearer <token>` — the value extends to end of line,
        // so masking only up to the first space would expose the token.
        let to_end_of_line = key.eq_ignore_ascii_case("authorization");
        if !to_end_of_line {
            if !matches!(chars.get(cursor), Some('=') | Some(':')) {
                continue;
            }
            cursor += 1;
            while matches!(chars.get(cursor), Some(' ') | Some('\t')) {
                cursor += 1;
            }
        } else if !matches!(chars.get(cursor), Some('=') | Some(':') | Some(' ')) {
            continue;
        }

        let value_start = cursor;
        let quoted = matches!(chars.get(cursor), Some('\'') | Some('"'));
        if quoted {
            cursor += 1;
            while cursor < chars.len()
                && chars[cursor] != chars[value_start]
                && chars[cursor] != '\n'
            {
                cursor += 1;
            }
            if cursor < chars.len() {
                cursor += 1;
            }
        } else if to_end_of_line {
            while cursor < chars.len() && chars[cursor] != '\n' {
                cursor += 1;
            }
            while cursor > value_start && chars[cursor - 1].is_whitespace() {
                cursor -= 1;
            }
        } else {
            while cursor < chars.len() && !is_value_terminator(chars[cursor]) {
                cursor += 1;
            }
        }

        if cursor == value_start {
            continue;
        }
        return Some(cursor);
    }

    None
}

/// Mask the value in `Bearer <token>` style headers.
fn mask_bearer_tokens(text: &str) -> String {
    const PREFIX: &str = "Bearer ";
    let lowered = text.to_ascii_lowercase();
    let needle = PREFIX.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut consumed = 0usize;
    let mut search_from = 0usize;

    while let Some(offset) = lowered[search_from..].find(&needle) {
        let position = search_from + offset;
        let after = &text[position + PREFIX.len()..];

        let value_end = after
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(after.len());

        out.push_str(&text[consumed..position]);
        out.push_str(PREFIX);
        out.push_str(REDACTED);
        consumed = position + PREFIX.len() + value_end;
        search_from = consumed;
        if consumed >= text.len() {
            break;
        }
    }

    out.push_str(&text[consumed..]);
    out
}

/// Mask long unbroken `[A-Za-z0-9_-]` runs that look like raw API keys. This is
/// the safety net for credentials pasted without any surrounding `key=value`.
fn mask_blobs(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;

    while index < chars.len() {
        let start = index;
        let mut has_digit = false;
        let mut has_alpha = false;
        while index < chars.len() {
            let c = chars[index];
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                has_digit |= c.is_ascii_digit();
                has_alpha |= c.is_ascii_alphabetic();
                index += 1;
            } else {
                break;
            }
        }

        // Always make progress, otherwise separators would loop forever.
        if index == start {
            out.push(chars[index]);
            index += 1;
            continue;
        }

        let run: String = chars[start..index].iter().collect();
        let looks_like_credential = run.len() >= BLOB_MIN_LEN && has_alpha && has_digit;
        if looks_like_credential {
            out.push_str(REDACTED);
        } else {
            out.push_str(&run);
        }
    }

    out
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_value_terminator(c: char) -> bool {
    c.is_whitespace() || matches!(c, ',' | ';' | '"' | '\'' | '`' | ')' | '}' | ']' | '|')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> SecretEntry {
        SecretEntry {
            id: "id".into(),
            name: "NEWSFEED_API_KEY".into(),
            kind: "api-key".into(),
            project: "market".into(),
            environment: "production".into(),
            username: "ops@example.com".into(),
            secret: "super-secret-value".into(),
            url: String::new(),
            notes: String::new(),
            tags: vec![],
            enabled: true,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn never_leaks_the_secret_or_username() {
        let rendered = entry_reference(&entry());
        assert!(!rendered.contains("super-secret-value"));
        assert!(!rendered.contains("ops@example.com"));
        assert!(rendered.contains(REDACTED));
        assert!(rendered.contains("NEWSFEED_API_KEY"));
        assert!(rendered.contains("market"));
        assert!(rendered.contains("production"));
    }

    #[test]
    fn keeps_contract_but_masks_credentials_in_notes() {
        let mut item = entry();
        item.notes = "新闻聚合 API\nBase URL: https://newsfeed.example.com\n\
                      认证方式: Authorization: Bearer sk-live-abcdef0123456789\n\
                      端点: /api/v1/hot/search\n\
                      返回: { code, data: { list: [] } }"
            .into();
        let rendered = entry_reference(&item);

        assert!(!rendered.contains("sk-live-abcdef0123456789"));
        assert!(rendered.contains("https://newsfeed.example.com"));
        assert!(rendered.contains("/api/v1/hot/search"));
        assert!(rendered.contains("data"));
    }

    #[test]
    fn masks_assigned_secrets_in_notes() {
        let text = "token=abc123secret456\nglobal retry = 3\napi_key: 'zzz999'";
        let masked = redact_notes(text);
        assert!(!masked.contains("abc123secret456"));
        assert!(!masked.contains("zzz999"));
        assert!(masked.contains("retry = 3"));
    }

    #[test]
    fn strips_sensitive_query_parameters() {
        let url =
            redact_url("https://api.example.com/v1?api_key=abcdef123456&limit=20&token=xyz789");
        assert!(!url.contains("abcdef123456"));
        assert!(!url.contains("xyz789"));
        assert!(url.contains("limit=20"));
    }

    #[test]
    fn masks_bare_credential_blobs() {
        let masked = redact_notes("明文密钥 abcdefghij0123456789ABCDEF 已废弃");
        assert!(!masked.contains("abcdefghij0123456789ABCDEF"));
        assert!(masked.contains("已废弃"));
    }

    #[test]
    fn leaves_short_identifiers_and_urls_intact() {
        assert_eq!(mask_blobs("OPENAI_API_KEY"), "OPENAI_API_KEY");
        assert_eq!(
            mask_blobs("https://newsfeed.example.com/api/v1/hot/search"),
            "https://newsfeed.example.com/api/v1/hot/search"
        );
        assert_eq!(mask_blobs("production"), "production");
    }

    #[test]
    fn terminates_on_every_punctuation_shape() {
        for text in [
            "",
            ":",
            "::::",
            "a",
            "a=b",
            "///",
            "-_-_-_",
            "Bearer ",
            "Authorization:",
            "token=",
        ] {
            let _ = redact_notes(text);
            let _ = redact_url(text);
            let _ = mask_blobs(text);
        }
    }
}
