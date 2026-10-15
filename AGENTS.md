# AGENTS.md

Working agreement for AI coding agents (and humans) in this repository.

## What this project is

Local API Vault — a local-first, offline, encrypted vault for API keys, with a
Windows Named Pipe bridge and a CLI that injects secrets into local commands.
No server, no cloud, no account.

## Secrets: the one rule that matters

Secrets live in an encrypted vault. They never live in the repository.

- **Never** read, parse, decrypt or copy these files:
  - `%USERPROFILE%\.local-api-vault\vault.age`
  - `%USERPROFILE%\.local-api-vault\vault.identity.age`
  - any `recovery.key`
- **Never** ask the user for the master passphrase or a recovery key.
- **Never** run `age`, `rage`, or any other decryption tooling on the vault.
- **Never** place a secret in a prompt, a transcript, a log, a test fixture, a
  generated file, or a commit.

If you are blocked because "the contract is inside the encrypted vault and I
cannot read it", that is a design problem, not a reason for decryption access.
See **API contracts are not secrets** below.

## How to obtain a secret

The desktop app must be running and unlocked. Then:

```powershell
lvault get EXAMPLE_API_KEY
```

- `lvault get <NAME>` writes exactly one secret value to stdout.
- Prefer assigning it rather than letting it scroll:

  ```powershell
  $env:EXAMPLE_API_KEY = (lvault get EXAMPLE_API_KEY)
  ```

- Or let the CLI launch the process for you — it injects the variables and
  never materialises a `.env` file:

  ```powershell
  lvault run web-dev
  ```

- If the app reports `Vault is locked`, **ask the user to unlock it**. Do not
  try to work around it. `lvault` prints this in Chinese with the exact next
  step.
- If the app is not running, ask the user to start it. Do not launch it yourself
  expecting it to be unlocked — it always starts locked.

If a name is not found, the response may carry `suggestions`: **entry names**
close to what you asked for. Use them to correct your next call. They never
contain secret values, and they only appear while the vault is unlocked.

`lvault.cmd` is installed next to the app executable
(`%LOCALAPPDATA%\Local API Vault\lvault.cmd`). Use the full path if that folder
is not on `PATH`.

`lvault` deliberately exposes only `get` and `run`. It is a thin wrapper over
`local-api-vault-cli.exe`, whose `get`/`list` verbs prompt for the master
passphrase — do not use those.

## API contracts are not secrets

Endpoints, request shape, response shape and the auth *scheme* (for example
"Authorization: Bearer") are ordinary documentation.

- Put them in `docs/`.
- Put a sanitized response body in `docs/fixtures/`.
- Put **only the credential itself** in the vault.

Never request decryption access in order to read documentation.

If a user needs to share an entry with you, they can use the **“复制参考资料”**
button in the entry editor. The application redacts the secret, the account,
sensitive URL parameters, and anything credential-shaped in the notes before
copying to the clipboard. Ask for that output rather than asking for the secret.

## Commands that are safe to run

```powershell
npm install
npm run build
npm run tauri build

cargo fmt   --manifest-path src-tauri/Cargo.toml
cargo test  --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

On Windows, `cargo` needs the MSVC environment. Run the build from a
"Developer PowerShell for VS", or initialise it in place:

```powershell
cmd /c '"<path-to-vs>\VC\Auxiliary\Build\vcvars64.bat" && npm run tauri -- build'
```

The test `autostart::tests::toggles_run_key_and_restores` briefly writes and then
restores a value under `HKCU\...\Run`. That is expected.

## Layout

| Path | Purpose |
| --- | --- |
| `src-tauri/src/vault.rs` | encrypted file format, atomic writes, rebuild/recover |
| `src-tauri/src/crypto.rs` | age wrappers: X25519 primary + scrypt identity sidecar |
| `src-tauri/src/bridge.rs` | Windows Named Pipe server/client, protocol v1 |
| `src-tauri/src/model.rs` | `SecretEntry`, `RunProfile`, `VaultDocument` |
| `src-tauri/src/state.rs` | in-memory unlocked session |
| `src-tauri/src/commands.rs` | Tauri commands (the frontend API) |
| `src-tauri/src/autostart.rs` | optional per-user start-at-login toggle |
| `src-tauri/src/bin/local-api-vault-cli.rs` | CLI (`list`, `get`, `pipe-get`, `run`) |
| `src/` | frontend, vanilla JS/CSS |
| `docs/` | security model, client contract, MVP scope |

## Client contract

`docs/client-contract.md` defines the Named Pipe protocol. Only two operations
exist: `get` and `profile`. There is no HTTP listener and no TCP port.

`document.version` is the JSON schema version and is independent of the age
recipient mode; adding fields requires `#[serde(default)]` so old vaults keep
loading.

## Invariants that must not be broken

- A locked vault reads nothing. A disabled entry blocks new reads only.
- Run profiles store names and commands, never secret values.
- Auto start only starts the process. It never auto-unlocks.
- The pipe ACL is current user + SYSTEM.

## Out of scope — do not add

HTTP listeners, fixed TCP ports, cloud sync, accounts/login, RBAC, HSM, idle
auto-lock, auto-unlock, telemetry, or secret values in run profiles.