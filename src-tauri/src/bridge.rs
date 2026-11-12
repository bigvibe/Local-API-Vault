use crate::audit::AuditEvent;
use crate::model::RunProfile;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Arc;

pub const PIPE_NAME: &str = "LocalApiVault-v1";
pub const PROTOCOL_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: u64 = 64 * 1024;
/// 一次 `audit` 请求最多带回多少条记录。
const MAX_AUDIT_EVENTS: usize = 200;
/// 客户端自报的程序标识。**自报，仅作线索**——真正可信的是 pid 与 process。
const CLIENT_NAME: &str = "lvault";

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientRequest {
    pub version: u32,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub environment: Option<String>,
    /// 调用方自报的工作目录。只作线索，服务端不验证。
    #[serde(default)]
    pub cwd: Option<String>,
    /// 调用方自报的程序标识（例如 `lvault`）。只作线索，服务端不验证。
    #[serde(default)]
    pub client: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ClientResponse {
    pub version: u32,
    pub ok: bool,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<RunProfile>,
    // Similar entry NAMES for a "not_found" response, so a caller that guessed
    // the wrong name gets a hint instead of a dead end. Names only — never
    // values — and only ever produced while the vault is unlocked, so this does
    // not widen what a locked vault can reveal. Additive and optional: an
    // older client ignores it.
    //
    // default is REQUIRED, not optional: the field is skipped when empty, so a
    // peer built before this existed (or any server that omits it) would
    // otherwise fail to deserialise every single response.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    // The access log, returned by the `audit` operation. Additive and optional:
    // an older client ignores it, and a server that omits it deserialises as an
    // empty list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<AuditEvent>,
}

impl ClientResponse {
    fn error(code: &str, message: &str) -> Self {
        Self::error_with(code, message, Vec::new())
    }

    fn error_with(code: &str, message: &str, suggestions: Vec<String>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            ok: false,
            code: code.to_owned(),
            message: message.to_owned(),
            value: None,
            entry_version: None,
            profile: None,
            suggestions,
            events: Vec::new(),
        }
    }

    fn success(value: String, entry_version: i64) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            ok: true,
            code: "ok".to_owned(),
            message: "ok".to_owned(),
            value: Some(value),
            entry_version: Some(entry_version),
            profile: None,
            suggestions: Vec::new(),
            events: Vec::new(),
        }
    }

    fn profile(profile: RunProfile) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            ok: true,
            code: "ok".to_owned(),
            message: "ok".to_owned(),
            value: None,
            entry_version: None,
            profile: Some(profile),
            suggestions: Vec::new(),
            events: Vec::new(),
        }
    }

    fn events(events: Vec<AuditEvent>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            ok: true,
            code: "ok".to_owned(),
            message: "ok".to_owned(),
            value: None,
            entry_version: None,
            profile: None,
            suggestions: Vec::new(),
            events,
        }
    }
}

/// 调用方身份。`pid` 与 `process` 来自**服务端**（管道句柄问内核要），
/// 所以调用方改不了；只有请求里的项目/目录是它自己说的。
#[derive(Debug, Clone, Default)]
pub struct Caller {
    pub pid: Option<u32>,
    pub process: Option<String>,
}

pub fn start(state: Arc<AppState>) -> Result<(), String> {
    start_with_pipe(state, PIPE_NAME)
}

#[cfg(windows)]
fn start_with_pipe(state: Arc<AppState>, pipe_name: &str) -> Result<(), String> {
    let listener = create_listener(pipe_name)?;
    std::thread::Builder::new()
        .name("local-api-vault-bridge".to_owned())
        .spawn(move || serve(listener, state))
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(not(windows))]
fn start_with_pipe(state: Arc<AppState>, _pipe_name: &str) -> Result<(), String> {
    let _ = state;
    Ok(())
}

#[cfg(windows)]
fn create_listener(pipe_name: &str) -> Result<interprocess::local_socket::Listener, String> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions};
    use interprocess::os::windows::local_socket::ListenerOptionsExt;
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;
    use widestring::U16CString;

    // The pipe is local-only and accessible to the account that created it.
    // SYSTEM is included so Windows services can inspect it if necessary.
    let sddl =
        U16CString::from_str("D:P(A;;GA;;;OW)(A;;GA;;;SY)").map_err(|error| error.to_string())?;
    let security =
        SecurityDescriptor::deserialize(sddl.as_ucstr()).map_err(|error| error.to_string())?;
    let name = pipe_name
        .to_ns_name::<GenericNamespaced>()
        .map_err(|error| error.to_string())?;
    ListenerOptions::new()
        .name(name)
        .security_descriptor(security)
        .try_overwrite(false)
        .create_sync()
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn serve(listener: interprocess::local_socket::Listener, state: Arc<AppState>) {
    use interprocess::local_socket::traits::ListenerExt;

    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let state = Arc::clone(&state);
                if let Err(error) = std::thread::Builder::new()
                    .name("local-api-vault-connection".to_owned())
                    .spawn(move || {
                        if let Err(error) = handle_connection(stream, &state) {
                            eprintln!("[bridge] connection failed: {error}");
                        }
                    })
                {
                    eprintln!("[bridge] could not start connection thread: {error}");
                }
            }
            Err(error) => eprintln!("[bridge] accept failed: {error}"),
        }
    }
}

#[cfg(windows)]
fn handle_connection(
    mut stream: interprocess::local_socket::Stream,
    state: &AppState,
) -> std::io::Result<()> {
    // Ask the kernel who is on the other end BEFORE reading anything. This is
    // the one piece of the access log the caller cannot lie about.
    let caller = caller_from(&stream);
    let line = read_request_line(&mut stream)?;
    if line.is_empty() {
        return Ok(());
    }
    if !line.ends_with('\n') {
        write_response(
            &mut stream,
            &ClientResponse::error("invalid_request", "request is too large"),
        )?;
        return Ok(());
    }

    let response = match serde_json::from_str::<ClientRequest>(&line) {
        Ok(request) => handle_request(state, request, &caller),
        Err(_) => ClientResponse::error("invalid_request", "invalid request"),
    };
    write_response(&mut stream, &response)
}

/// Read the client process id straight off the pipe handle.
#[cfg(windows)]
fn caller_from(stream: &interprocess::local_socket::Stream) -> Caller {
    use interprocess::local_socket::traits::StreamCommon;

    let pid = stream.peer_creds().ok().and_then(|creds| creds.pid());
    Caller {
        pid,
        process: pid.and_then(crate::audit::process_name),
    }
}

#[cfg(windows)]
fn read_request_line(stream: &mut interprocess::local_socket::Stream) -> std::io::Result<String> {
    let mut limited = stream.take(MAX_REQUEST_BYTES);
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let count = limited.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.contains(&b'\n') || bytes.len() as u64 >= MAX_REQUEST_BYTES {
            break;
        }
    }
    String::from_utf8(bytes)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid UTF-8"))
}

#[cfg(windows)]
fn write_response(
    mut stream: &mut interprocess::local_socket::Stream,
    response: &ClientResponse,
) -> std::io::Result<()> {
    serde_json::to_writer(&mut stream, response)?;
    stream.write_all(b"\n")?;
    stream.flush()
}

fn handle_request(state: &AppState, request: ClientRequest, caller: &Caller) -> ClientResponse {
    if request.version != PROTOCOL_VERSION {
        return ClientResponse::error("invalid_request", "unsupported protocol version");
    }

    let operation = request.operation.as_deref().unwrap_or("get");

    // `audit` reads the app's own access log, not the vault. It is still gated
    // on "unlocked": the log holds entry NAMES, and names are exactly what a
    // locked vault must not reveal.
    if operation == "audit" {
        let status = match state.status() {
            Ok(status) => status,
            Err(_) => return ClientResponse::error("internal_error", "internal error"),
        };
        if status.locked {
            return ClientResponse::error("vault_locked", "Vault is locked");
        }
        return ClientResponse::events(state.audit().events(MAX_AUDIT_EVENTS));
    }

    let name = request.name.trim();
    if name.is_empty() || name.len() > 200 {
        return ClientResponse::error("invalid_request", "name is required");
    }

    let status = match state.status() {
        Ok(status) => status,
        Err(_) => return ClientResponse::error("internal_error", "internal error"),
    };
    if status.locked {
        record_event(state, &request, name, caller, "vault_locked");
        return ClientResponse::error("vault_locked", "Vault is locked");
    }

    let response = match operation {
        "get" => handle_secret_request(state, &request, name),
        "profile" => handle_profile_request(state, name),
        _ => return ClientResponse::error("invalid_request", "unsupported operation"),
    };
    record_event(state, &request, name, caller, &response.code);
    response
}

/// Write one line to the access log.
///
/// The outcome is the response code, so the log distinguishes "read it" from
/// "no such entry" from "entry is disabled" from "vault was locked".
fn record_event(
    state: &AppState,
    request: &ClientRequest,
    name: &str,
    caller: &Caller,
    outcome: &str,
) {
    state.audit().record(AuditEvent {
        at: AuditEvent::now(),
        at_local: crate::audit::local_time_string(),
        pid: caller.pid,
        process: caller.process.clone(),
        name: name.to_owned(),
        project: request.project.clone(),
        environment: request.environment.clone(),
        cwd: request.cwd.clone(),
        client: request.client.clone(),
        outcome: outcome.to_owned(),
    });
}

/// Case-insensitive substring test used for name suggestions.
fn name_contains(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

/// Rank candidate entry names by how plausibly they were meant to be `wanted`.
///
/// The user-facing problem this solves: someone remembers "I saved something
/// for that one service" but not under what exact name. Returning a few nearby
/// NAMES turns a dead-end error into a usable correction. Only names are ever
/// compared here; no value is involved, and this only runs on the unlocked path.
fn suggest_names(candidates: &[String], wanted: &str, limit: usize) -> Vec<String> {
    let mut scored: Vec<(u32, &String)> = Vec::new();
    for candidate in candidates {
        let score = if candidate.eq_ignore_ascii_case(wanted) {
            0 // exact match ignoring case — overwhelmingly the intended one
        } else if name_contains(candidate, wanted) {
            1 // wanted appears inside the candidate
        } else if wanted.len() >= 3 && name_contains(wanted, candidate) {
            2 // candidate appears inside what was typed
        } else {
            continue;
        };
        scored.push((score, candidate));
    }
    // Stable within a score: keep the document's ordering so output is
    // deterministic across identical requests.
    scored.sort_by_key(|(score, _)| *score);
    scored
        .into_iter()
        .take(limit)
        .map(|(_, name)| name.clone())
        .collect()
}

fn handle_secret_request(state: &AppState, request: &ClientRequest, name: &str) -> ClientResponse {
    // First pass: is there an entry that satisfies the filters?
    // Clone what we need out of the lock; a borrowed &SecretEntry cannot escape
    // with_mut's borrow.
    let found: Option<(bool, String, i64)> = state
        .with_mut(|vault| {
            Ok(vault
                .document
                .entries
                .iter()
                .find(|entry| {
                    entry.name == name
                        && matches_filter(&entry.project, request.project.as_deref())
                        && matches_filter(&entry.environment, request.environment.as_deref())
                })
                .map(|entry| (entry.enabled, entry.secret.clone(), entry.updated_at)))
        })
        .unwrap_or(None);

    match found {
        Some((false, _, _)) => ClientResponse::error("disabled", "secret is disabled"),
        Some((true, secret, entry_version)) => ClientResponse::success(secret, entry_version),
        None => {
            // No match. Offer nearby names from the same filter scope. Disabled
            // entries are included on purpose: "not found" is frequently really
            // "found but disabled", and a disabled entry's name is not a secret.
            let names = state
                .with_mut(|vault| {
                    Ok(vault
                        .document
                        .entries
                        .iter()
                        .filter(|entry| {
                            matches_filter(&entry.project, request.project.as_deref())
                                && matches_filter(
                                    &entry.environment,
                                    request.environment.as_deref(),
                                )
                        })
                        .map(|entry| entry.name.clone())
                        .collect::<Vec<_>>())
                })
                .unwrap_or_default();
            let suggestions = suggest_names(&names, name, 3);
            ClientResponse::error_with("not_found", "secret not found", suggestions)
        }
    }
}

fn handle_profile_request(state: &AppState, name: &str) -> ClientResponse {
    let result: Result<RunProfile, String> = state.with_mut(|vault| {
        vault
            .document
            .profiles
            .iter()
            .find(|profile| profile.name.eq_ignore_ascii_case(name))
            .cloned()
            .ok_or_else(|| "profile_not_found".to_owned())
    });
    match result {
        Ok(profile) => ClientResponse::profile(profile),
        Err(code) => ClientResponse::error(
            if code == "profile_not_found" {
                "not_found"
            } else {
                "internal_error"
            },
            if code == "profile_not_found" {
                "run profile not found"
            } else {
                "internal error"
            },
        ),
    }
}

fn matches_filter(actual: &str, requested: Option<&str>) -> bool {
    requested
        .map(|value| value.trim().is_empty() || value == actual)
        .unwrap_or(true)
}

/// A failed request, carrying the machine-readable code plus any name
/// suggestions, so callers can render something a person can act on.
#[derive(Debug, Clone)]
pub struct ClientError {
    pub code: String,
    pub message: String,
    pub suggestions: Vec<String>,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if !self.suggestions.is_empty() {
            write!(f, " (did you mean: {})", self.suggestions.join(", "))?;
        }
        Ok(())
    }
}

#[cfg(windows)]
pub fn request_secret(
    name: &str,
    project: Option<&str>,
    environment: Option<&str>,
    working_directory: Option<&str>,
) -> Result<String, ClientError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ClientError {
            code: "invalid_request".to_owned(),
            message: "name is required".to_owned(),
            suggestions: Vec::new(),
        });
    }
    let response = send_request(ClientRequest {
        version: PROTOCOL_VERSION,
        operation: Some("get".to_owned()),
        name: name.to_owned(),
        project: project.map(str::to_owned),
        environment: environment.map(str::to_owned),
        cwd: working_directory.map(str::to_owned),
        client: Some(CLIENT_NAME.to_owned()),
    })
    .map_err(|message| ClientError {
        code: "bridge_unavailable".to_owned(),
        message,
        suggestions: Vec::new(),
    })?;
    if response.ok {
        response.value.ok_or_else(|| ClientError {
            code: "internal_error".to_owned(),
            message: "bridge returned no value".to_owned(),
            suggestions: Vec::new(),
        })
    } else {
        Err(ClientError {
            code: response.code,
            message: response.message,
            suggestions: response.suggestions,
        })
    }
}

#[cfg(windows)]
pub fn request_profile(
    name: &str,
    working_directory: Option<&str>,
) -> Result<RunProfile, ClientError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ClientError {
            code: "invalid_request".to_owned(),
            message: "profile name is required".to_owned(),
            suggestions: Vec::new(),
        });
    }
    let response = send_request(ClientRequest {
        version: PROTOCOL_VERSION,
        operation: Some("profile".to_owned()),
        name: name.to_owned(),
        project: None,
        environment: None,
        cwd: working_directory.map(str::to_owned),
        client: Some(CLIENT_NAME.to_owned()),
    })
    .map_err(|message| ClientError {
        code: "bridge_unavailable".to_owned(),
        message,
        suggestions: Vec::new(),
    })?;
    if response.ok {
        response.profile.ok_or_else(|| ClientError {
            code: "internal_error".to_owned(),
            message: "bridge returned no profile".to_owned(),
            suggestions: Vec::new(),
        })
    } else {
        Err(ClientError {
            code: response.code,
            message: response.message,
            suggestions: response.suggestions,
        })
    }
}

/// The access log, newest first.
#[cfg(windows)]
pub fn request_audit() -> Result<Vec<AuditEvent>, ClientError> {
    let response = send_request(ClientRequest {
        version: PROTOCOL_VERSION,
        operation: Some("audit".to_owned()),
        name: String::new(),
        project: None,
        environment: None,
        cwd: None,
        client: Some(CLIENT_NAME.to_owned()),
    })
    .map_err(|message| ClientError {
        code: "bridge_unavailable".to_owned(),
        message,
        suggestions: Vec::new(),
    })?;
    if response.ok {
        Ok(response.events)
    } else {
        Err(ClientError {
            code: response.code,
            message: response.message,
            suggestions: response.suggestions,
        })
    }
}

#[cfg(windows)]
fn send_request(request: ClientRequest) -> Result<ClientResponse, String> {
    send_request_to(PIPE_NAME, request)
}

#[cfg(windows)]
fn send_request_to(pipe_name: &str, request: ClientRequest) -> Result<ClientResponse, String> {
    use interprocess::local_socket::{prelude::*, GenericNamespaced, Stream};

    let name = pipe_name
        .to_ns_name::<GenericNamespaced>()
        .map_err(|error| error.to_string())?;
    let mut stream = Stream::connect(name).map_err(|error| error.to_string())?;
    serde_json::to_writer(&mut stream, &request).map_err(|error| error.to_string())?;
    stream.write_all(b"\n").map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&line).map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub fn request_secret(
    _name: &str,
    _project: Option<&str>,
    _environment: Option<&str>,
    _working_directory: Option<&str>,
) -> Result<String, ClientError> {
    Err(ClientError {
        code: "bridge_unavailable".to_owned(),
        message: "Named Pipe bridge is currently available on Windows only.".to_owned(),
        suggestions: Vec::new(),
    })
}

#[cfg(not(windows))]
pub fn request_profile(
    _name: &str,
    _working_directory: Option<&str>,
) -> Result<RunProfile, ClientError> {
    Err(ClientError {
        code: "bridge_unavailable".to_owned(),
        message: "Named Pipe bridge is currently available on Windows only.".to_owned(),
        suggestions: Vec::new(),
    })
}

#[cfg(not(windows))]
pub fn request_audit() -> Result<Vec<AuditEvent>, ClientError> {
    Err(ClientError {
        code: "bridge_unavailable".to_owned(),
        message: "Named Pipe bridge is currently available on Windows only.".to_owned(),
        suggestions: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SecretEntry;
    use crate::state::UnlockedVault;
    use crate::vault::VaultFile;
    use tempfile::tempdir;

    fn request(name: &str) -> ClientRequest {
        ClientRequest {
            version: PROTOCOL_VERSION,
            operation: Some("get".to_owned()),
            name: name.to_owned(),
            project: Some("demo".to_owned()),
            environment: Some("development".to_owned()),
            cwd: Some("C:\\demo".to_owned()),
            client: Some("lvault".to_owned()),
        }
    }

    fn caller() -> Caller {
        Caller {
            pid: Some(4242),
            process: Some("demo.exe".to_owned()),
        }
    }

    #[test]
    fn bridge_respects_locked_and_disabled_states() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let passphrase = "bridge test passphrase with entropy";
        VaultFile::create(&path, passphrase).unwrap();
        let mut document = VaultFile::load(&path, passphrase).unwrap();
        document.entries.push(SecretEntry {
            id: "id".to_owned(),
            name: "API_KEY".to_owned(),
            kind: "api-key".to_owned(),
            project: "demo".to_owned(),
            environment: "development".to_owned(),
            username: String::new(),
            secret: "value".to_owned(),
            url: String::new(),
            notes: String::new(),
            tags: vec![],
            enabled: false,
            created_at: 1,
            updated_at: 1,
        });
        document.profiles.push(crate::model::RunProfile {
            id: "profile".to_owned(),
            name: "demo".to_owned(),
            working_directory: "C:\\demo".to_owned(),
            command: "npm run dev".to_owned(),
            secrets: vec!["API_KEY".to_owned()],
            project: "demo".to_owned(),
            environment: "development".to_owned(),
            created_at: 1,
            updated_at: 1,
        });

        let state = AppState::default();
        assert_eq!(
            handle_request(&state, request("API_KEY"), &caller()).code,
            "vault_locked"
        );

        let mut disabled = document.clone();
        disabled.entries[0].enabled = false;
        state
            .replace(UnlockedVault::from_parts(
                path.clone(),
                passphrase.to_owned(),
                disabled,
            ))
            .unwrap();
        assert_eq!(
            handle_request(&state, request("API_KEY"), &caller()).code,
            "disabled"
        );

        let mut enabled = document;
        enabled.entries[0].enabled = true;
        state
            .replace(UnlockedVault::from_parts(
                path,
                passphrase.to_owned(),
                enabled,
            ))
            .unwrap();
        let response = handle_request(&state, request("API_KEY"), &caller());
        assert!(response.ok);
        assert_eq!(response.value.as_deref(), Some("value"));

        let mut profile_request = request("demo");
        profile_request.operation = Some("profile".to_owned());
        let profile_response = handle_request(&state, profile_request, &caller());
        assert!(profile_response.ok);
        assert_eq!(profile_response.profile.unwrap().command, "npm run dev");
    }

    fn unlocked_state() -> AppState {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let passphrase = "audit test passphrase with entropy";
        VaultFile::create(&path, passphrase).unwrap();
        let mut document = VaultFile::load(&path, passphrase).unwrap();
        document.entries.push(SecretEntry {
            id: "id".to_owned(),
            name: "API_KEY".to_owned(),
            kind: "api-key".to_owned(),
            project: "demo".to_owned(),
            environment: "development".to_owned(),
            username: String::new(),
            secret: "super-secret-value".to_owned(),
            url: String::new(),
            notes: String::new(),
            tags: vec![],
            enabled: true,
            created_at: 1,
            updated_at: 1,
        });
        let state = AppState::default();
        state
            .replace(UnlockedVault::from_parts(
                path,
                passphrase.to_owned(),
                document,
            ))
            .unwrap();
        state
    }

    fn audit_request() -> ClientRequest {
        ClientRequest {
            version: PROTOCOL_VERSION,
            operation: Some("audit".to_owned()),
            name: String::new(),
            project: None,
            environment: None,
            cwd: None,
            client: Some("lvault".to_owned()),
        }
    }

    #[test]
    fn records_who_read_what() {
        let state = unlocked_state();
        let response = handle_request(&state, request("API_KEY"), &caller());
        assert!(response.ok);

        let events = state.audit().events(0);
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.name, "API_KEY");
        assert_eq!(event.outcome, "ok");
        // Server-side facts: these cannot be forged by the caller.
        assert_eq!(event.pid, Some(4242));
        assert_eq!(event.process.as_deref(), Some("demo.exe"));
        // Caller-reported facts: leads, not proof.
        assert_eq!(event.cwd.as_deref(), Some("C:\\demo"));
        assert_eq!(event.client.as_deref(), Some("lvault"));
        // Never a value.
        assert!(!format!("{event:?}").contains("super-secret-value"));
    }

    #[test]
    fn records_misses_and_disabled_entries_too() {
        let state = unlocked_state();
        handle_request(&state, request("NOPE"), &caller());
        let events = state.audit().events(0);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].outcome, "not_found");
        assert_eq!(events[0].name, "NOPE");
    }

    #[test]
    fn records_attempts_while_locked() {
        let state = AppState::default();
        let response = handle_request(&state, request("API_KEY"), &caller());
        assert_eq!(response.code, "vault_locked");
        let events = state.audit().events(0);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].outcome, "vault_locked");
    }

    #[test]
    fn the_audit_operation_returns_the_log() {
        let state = unlocked_state();
        handle_request(&state, request("API_KEY"), &caller());
        let response = handle_request(&state, audit_request(), &caller());
        assert!(response.ok);
        assert_eq!(response.events.len(), 1);
        assert_eq!(response.events[0].name, "API_KEY");
    }

    #[test]
    fn the_audit_operation_is_locked_out_too() {
        // The log holds entry NAMES, and a locked vault must not reveal names.
        let state = AppState::default();
        let response = handle_request(&state, audit_request(), &caller());
        assert_eq!(response.code, "vault_locked");
        assert!(response.events.is_empty());
    }

    #[test]
    fn reading_the_log_does_not_add_to_the_log() {
        let state = unlocked_state();
        handle_request(&state, request("API_KEY"), &caller());
        handle_request(&state, audit_request(), &caller());
        assert_eq!(state.audit().events(0).len(), 1);
    }

    /// End to end over a real named pipe: the server must learn the calling
    /// process from the pipe handle itself, not from anything the caller said.
    #[cfg(windows)]
    #[test]
    fn records_the_calling_process_end_to_end() {
        // A private pipe name, so the test never collides with a running app.
        let pipe_name = format!("LocalApiVault-test-{}", std::process::id());
        let state = std::sync::Arc::new(unlocked_state());
        start_with_pipe(std::sync::Arc::clone(&state), &pipe_name).unwrap();

        let response = send_request_to(&pipe_name, request("API_KEY")).unwrap();
        assert!(response.ok);
        assert_eq!(response.value.as_deref(), Some("super-secret-value"));

        let events = state.audit().events(0);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].pid,
            Some(std::process::id()),
            "服务端应当从管道句柄问出客户端 PID"
        );
        let process = events[0]
            .process
            .clone()
            .expect("应当能由 PID 反查出进程名");
        assert!(process.to_lowercase().ends_with(".exe"), "{process}");
    }

    fn suggest_names_for(candidates: &[&str], wanted: &str) -> Vec<String> {
        let owned: Vec<String> = candidates.iter().map(|s| (*s).to_owned()).collect();
        suggest_names(&owned, wanted, 3)
    }

    #[test]
    fn suggests_nearby_names_on_a_miss() {
        // The realistic beginner case: the name is remembered approximately.
        let suggestions = suggest_names_for(
            &["NEWSFEED_API_KEY", "OPENAI_API_KEY", "ANTHROPIC_API_KEY"],
            "NEWSFEED",
        );
        assert_eq!(suggestions, vec!["NEWSFEED_API_KEY".to_owned()]);

        // Case-insensitive exact match ranks first.
        let suggestions = suggest_names_for(
            &["newsfeed.example.com", "NEWSFEED_API_KEY"],
            "newsfeed.example.com",
        );
        assert_eq!(suggestions[0], "newsfeed.example.com");

        // Candidate contained in what was typed, when the query is long enough.
        let suggestions = suggest_names_for(&["api_key", "SOMETHING_ELSE"], "my_api_key");
        assert!(suggestions.contains(&"api_key".to_owned()));

        // Nothing related: no suggestions rather than noise.
        let suggestions = suggest_names_for(&["OPENAI_API_KEY"], "completely_unrelated_xyz");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn not_found_carries_suggestions_only_when_unlocked() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault.age");
        let passphrase = "bridge suggestion test passphrase";
        VaultFile::create(&path, passphrase).unwrap();
        let mut document = VaultFile::load(&path, passphrase).unwrap();
        document.entries.push(SecretEntry {
            id: "id".to_owned(),
            name: "NEWSFEED_API_KEY".to_owned(),
            kind: "api-key".to_owned(),
            project: "demo".to_owned(),
            environment: "development".to_owned(),
            username: String::new(),
            secret: "value".to_owned(),
            url: String::new(),
            notes: String::new(),
            tags: vec![],
            enabled: true,
            created_at: 1,
            updated_at: 1,
        });

        let state = AppState::default();

        // Locked: vault_locked, and NO name suggestions — a locked vault reveals
        // nothing, not even which entries exist.
        let locked = handle_request(&state, request("NEWSFEED"), &caller());
        assert_eq!(locked.code, "vault_locked");
        assert!(locked.suggestions.is_empty());

        state
            .replace(UnlockedVault::from_parts(
                path.clone(),
                passphrase.to_owned(),
                document,
            ))
            .unwrap();

        // Unlocked miss: suggestions present, names only — never a value.
        let response = handle_request(&state, request("NEWSFEED"), &caller());
        assert_eq!(response.code, "not_found");
        assert!(response
            .suggestions
            .contains(&"NEWSFEED_API_KEY".to_owned()));
        assert!(!response.message.contains("value"));
        // The stable machine-readable message is unchanged (contract).
        assert_eq!(response.message, "secret not found");
    }

    #[test]
    fn response_without_suggestions_still_deserialises() {
        // Regression: the server omits `suggestions` when empty. A client that
        // requires the field fails to parse EVERY response — including the ones
        // that carry a value — and reports it as "bridge unreachable", which is
        // indistinguishable from the app being closed. Default-on-deserialise
        // is the only thing that keeps the field additive.
        let without =
            r#"{"version":1,"ok":true,"code":"ok","message":"ok","value":"v","entryVersion":3}"#;
        let parsed: ClientResponse = serde_json::from_str(without).unwrap();
        assert!(parsed.ok);
        assert_eq!(parsed.value.as_deref(), Some("v"));
        assert!(parsed.suggestions.is_empty());

        let with = r#"{"version":1,"ok":false,"code":"not_found","message":"secret not found","suggestions":["A"]}"#;
        let parsed: ClientResponse = serde_json::from_str(with).unwrap();
        assert_eq!(parsed.suggestions, vec!["A".to_owned()]);
    }
}
