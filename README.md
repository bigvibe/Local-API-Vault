[English](README.md) | [简体中文](README.zh-CN.md)

# Local API Vault

[![CI](https://github.com/bigvibe/Local-API-Vault/actions/workflows/ci.yml/badge.svg)](https://github.com/bigvibe/Local-API-Vault/actions/workflows/ci.yml)
[![Security](https://github.com/bigvibe/Local-API-Vault/actions/workflows/security.yml/badge.svg)](https://github.com/bigvibe/Local-API-Vault/actions/workflows/security.yml)
[![CodeQL](https://github.com/bigvibe/Local-API-Vault/actions/workflows/codeql.yml/badge.svg)](https://github.com/bigvibe/Local-API-Vault/actions/workflows/codeql.yml)
[![Release](https://github.com/bigvibe/Local-API-Vault/actions/workflows/release.yml/badge.svg)](https://github.com/bigvibe/Local-API-Vault/releases)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Platform: Windows](https://img.shields.io/badge/platform-Windows-0078D6?logo=windows)](https://learn.microsoft.com/windows/)

A local-first, offline, developer-oriented vault for API keys.

> Current status: MVP in development. Do not store your only copy of high-value credentials here before the audit is complete.

> **Note:** the app UI is currently Chinese-only. Chinese button and menu names are kept in their original form below (with an English gloss where helpful) so you can find them in the app.

## Download & install

Grab the latest build from the [Releases page](https://github.com/bigvibe/Local-API-Vault/releases) and double-click `_x64-setup.exe`.

File names look like `Local.API.Vault_<version>_x64-setup.exe`. Note that GitHub replaces the **spaces** in the product name with **dots**, so it is `Local.API.Vault`, not `Local API Vault` — search the download list for `_x64-setup.exe` and you can't miss it.

- Installs into your user directory (`%LOCALAPPDATA%`), **no administrator rights needed**
- Uninstall via Settings → Apps; it also cleans up the launch-at-login entry

On first launch you create the Vault and set a master passphrase. **The master passphrase cannot be recovered.** The app will also show you a recovery identity file — save it somewhere offline.

### Verifying the download

GitHub shows the SHA-256 of each asset below the asset list; compare it locally:

```powershell
Get-FileHash .\Local.API.Vault_<version>_x64-setup.exe -Algorithm SHA256
```

### After installing

```powershell
lvault get <your-entry-name>
```

`lvault` is installed alongside the app and added to `PATH`, but **only for newly opened terminals** — open a fresh PowerShell window after installing. If the command is not found, see the [CLI section](#cli).

> **Windows will warn about an "unknown publisher"**
>
> The build artifacts are not code-signed, so SmartScreen will intercept once. This is expected, not an infection. Click "More info" → "Run anyway" to proceed. See [Security boundaries](#security-boundaries) for details.

## Goals

- Store API keys, tokens, signing secrets and environment configs locally
- Offline by default: no registration, no uploads, no telemetry
- Masked display, copy, project/environment organization, manual lock
- Inject secrets into local commands via a Windows Named Pipe + the `lvault` CLI — never writes a `.env` file

## Who it's for / who it's not for

**For:** a single user, on one machine, where the project reads secrets from environment variables (env / `os.environ` / `process.env`). Secret values are only consumed by local dev tools or scripts — no multi-user sharing, no cloud.

**Not for:**

- Syncing across machines / sharing secrets in a team — there is no account system or shared vault; that is a deliberate boundary
- CI/CD secret injection — there is no non-interactive unlock API (deliberately, so the master passphrase never appears in scripts); use your CI platform's secret store
- Automatic secret rotation / renewal — not implemented; rotation is a manual flow
- Projects that require a "plaintext config file" — this tool never writes files containing secrets; use environment-variable interpolation or pick another solution
- Cross-platform use — Windows only for now

The full integration guide (Python / Node / config-file interpolation / moving machines / rotation / why CI doesn't work…) is in [Recipes: using it in real projects](docs/recipes.md) — currently Chinese-only.

## MVP scope

- Create, open, rebuild and lock the Vault
- Stays unlocked after you unlock it; you can lock manually anytime; locking again on app exit
- Rebuild keeps the old file and creates an empty Vault
- Auto-generates an X25519 recovery identity with a recovery entry point
- Add, edit, delete, disable/enable and search entries
- Disabled entries stay in the Vault but are not returned to normal clients or the CLI
- Encrypted file storage
- Copy a secret with clipboard cleared after 30 seconds
- "Copy reference" auto-masks secrets and accounts, producing safe-to-share documentation text
- Import key-values from `.env` files
  - Import **does not filter anything** (including values that don't look like secrets, e.g. paths and flags) — rather over-store than miss one; delete what you don't need from the entry list afterwards
- Esc closes dialogs, switching entries warns about unsaved changes, keyboard focus rings and focus traps
- Readability: functional text floor of 12px, inputs at 13px, primary buttons at ≥ 4.5:1 contrast; English eyebrows replaced with Chinese; the "disabled" toggle is hidden when creating a new entry
- Native file pickers and encrypted backup export
- Sidebar "view backup": opens a backup read-only to see what's inside (names and metadata only, no secret values)
- `lvault verify` read-only Vault integrity check (never asks for the master passphrase, never prints secret values)
- `lvault audit` access log: when, which process, which entry NAME (memory-only, never written to disk)
- In-app sidebar "access log" (读取记录): the same log, no terminal needed
- Changing the master passphrase re-encrypts the Vault
- Minimize to system tray; tray menu can re-show or quit
- Optional launch-at-login; starts silently into the tray at login and stays locked
- Recovery identity can be rotated (the old one stops working immediately)
- Restore from an encrypted backup; backups carry a SHA-256 fingerprint for verification
- Windows only for now; macOS / Linux are not planned

## Tech stack

- Tauri 2
- Rust
- Vanilla JavaScript / CSS
- `age`-compatible encrypted files
- Versioned JSON document

### File layout of a new Vault

```text
vault.age          # primary Vault, encrypted to the X25519 public key
vault.identity.age # passphrase-encrypted X25519 identity sidecar
recovery.key       # recovery identity file you keep yourself
```

The master passphrase unlocks the sidecar to recover the X25519 identity, which then opens the primary Vault; the recovery identity file can bypass the passphrase entirely. The app never uploads or auto-saves the recovery private key to the cloud.

On first Vault creation or rebuild, the app shows the recovery identity once. Save it outside the Vault directory.

After some use, the directory may also contain these (all written by the app; deleting them does not affect the primary Vault):

```text
vault.before-rotate-<timestamp>.age    # old Vault before recovery identity rotation (paired with its .identity.age)
vault.before-restore-<timestamp>.age   # old Vault before a backup restore
<backup-name>.sha256                   # SHA-256 fingerprint of a backup
```

### Forgot the master passphrase?

Click **"恢复 Vault"** (Recover Vault) on the first screen — the wizard walks you through: pick the original Vault file, pick your saved recovery identity file, set a new master passphrase. When done it tells you three things: all entries are intact, the old passphrase stops working immediately, and the recovery identity file **still works** (you can use it again if you forget the new passphrase).

The "Forgot your master passphrase?" block on the first screen explains the difference between **recover** and **rebuild** — don't click the wrong one:

- **恢复 Vault (Recover Vault)** — opens **your existing data**, every entry intact
- **重建 Vault (Rebuild Vault)** — creates an **empty** new Vault; the old file is kept as `vault.age.before-rebuild-…`, the old passphrase still opens it, but the current Vault is empty

Without a recovery identity file, rebuild is the only path: the master passphrase itself has no recovery mechanism whatsoever.

### Suspect the recovery key leaked?

Sidebar **"更多 ⋯ → 更换恢复密钥"** (More → Rotate recovery key; called "设置恢复密钥" before a recovery identity exists) issues a new one:

- After rotation, **the old recovery key stops working immediately** — it can no longer open this Vault
- Save the new one carefully; the old one can be destroyed
- The pre-rotation Vault is kept alongside (`vault.before-rotate-…`); it still holds the old data openable by the old key — nothing is lost
- To check whether a recovery identity still works: `lvault verify --identity <file>` — if it can't open the Vault, it's been revoked

### Want to see what's inside a backup?

Sidebar **"查看备份"** (View backup): pick the backup you exported → enter its original master passphrase → see the inventory directly (entry names, kind, project, environment, enabled status, update time), plus the SHA-256 fingerprint comparison result.

- This is a **read-only view**: names and metadata only, no secret values, and nothing can be changed
- Just confirming "does that backup contain this key?" — look and close; no restore needed
- To actually get the content back, use "从备份恢复" (Restore from backup)

### Restoring from a backup

Sidebar **"更多 ⋯ → 从备份恢复"** (More → Restore from backup): pick the backup you exported → enter its original master passphrase → after verification passes, restore.

- A backup is a **pair** of files: `vault.age` and the `vault.identity.age` next to it; keep them in the same directory
- Verification reports **counts only** (how many entries, disabled entries, run profiles) — never names or values
- Verification also compares the **SHA-256 fingerprint**: export writes a `<backup-name>.sha256` next to the backup, and restore tells you "differs from export time" if the file was touched (even by one byte)
- Restore uses the backup's **original master passphrase**; the pre-restore Vault is kept alongside (`vault.before-restore-…`)
- Restore **keeps the original identity**, so your recovery key file still opens the restored Vault
- If this backup's passphrase ever leaked, go to sidebar "更多 ⋯ → 修改主密码" (More → Change passphrase) after restoring

## Development

Requirements:

- Node.js 22+
- Rust stable
- Windows builds need Visual Studio C++ Build Tools
- WebView2 runtime (usually already present on modern Windows)

```powershell
npm install
npm run tauri dev
```

Build:

```powershell
npm run tauri build
```

## CLI

The CLI reads the master passphrase from a secure prompt; it never accepts the passphrase as a command-line argument:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin local-api-vault-cli -- list --vault "$HOME/.local-api-vault/vault.age"
cargo run --manifest-path src-tauri/Cargo.toml --bin local-api-vault-cli -- get OPENAI_API_KEY --vault "$HOME/.local-api-vault/vault.age"
```

`get` prints only the secret value, suitable for controlled scripts; do not write the output into logs or command history. Disabled entries do not appear in `list` and cannot be read via `get`.

The Windows Named Pipe Bridge is built in, pipe name `\\.\pipe\LocalApiVault-v1`. Once the app is unlocked, local clients can read through it; locked or disabled states return the corresponding error:

```powershell
local-api-vault-cli pipe-get OPENAI_API_KEY --project web-app --environment development
```

The bridge only listens on a Windows Named Pipe — no TCP port is opened.

The in-app sidebar "运行配置" (Run profiles) can save a project directory, launch command and secret names; run them directly afterwards:

```powershell
local-api-vault-cli run web-dev
```

Or inject secrets into an ad-hoc local command without generating a `.env`:

```powershell
local-api-vault-cli run --cwd D:\projects\web --secret OPENAI_API_KEY -- npm run dev
```

`run` first checks through the Pipe whether the Vault is unlocked and the entries enabled, then passes the secrets to the child process as environment variables.

### Project-level auto-discovery: `.lvault.toml`

Drop a `.lvault.toml` in the project root containing only the **entry names** to inject (i.e. environment variable names) — no values, safe to commit:

```toml
vars = ["OPENAI_API_KEY", "NEWSFEED_API_KEY"]
```

Then run inside that directory without listing `--secret` each time:

```powershell
lvault run -- npm run dev
```

- The file contains only names — **never secret values**; malformed content errors out instead of being half-accepted
- Only looked up in the **current directory**; no parent-directory recursion, so a parent project's entries never leak into another shell
- Explicit `--secret` flags win; `.lvault.toml` is ignored when they are given
- Limits: 16 KB, at most 64 names, top-level `vars` only

`lvault` is the safe wrapper installed next to the app. It exposes only four verbs:

```powershell
lvault get OPENAI_API_KEY --project web-app --environment development
lvault run web-dev
lvault verify
lvault audit
```

`lvault get` never asks for the master passphrase — it only fetches values through the unlocked app's Named Pipe, so it's suitable for scripts and AI assistants. The conventions are documented in [`AGENTS.md`](AGENTS.md) (Chinese-only for now).

### `lvault audit`: who read my secrets

To see "who has read this key", run:

```powershell
lvault audit
```

```text
读取记录（新的在前）：
2026-11-12 10:30:15  local-api-vault-cli.exe (PID 12345)  NEWSFEED_API_KEY  ok  D:\projects\web
2026-11-12 10:31:02  node.exe (PID 20984)  NEWSFEED_API_KEY  ok  D:\projects\web

进程与 PID 由服务端核实；项目与目录是调用方自报，只能当线索。
记录只在内存里，重启应用即清空；里面只有条目名，没有密钥值。
```

No terminal switch needed: the sidebar **"更多 ⋯ → 读取记录"** (More → Access log) shows the same log with a refresh button.

Each record contains: **server-side timestamp**, **calling process and PID**, the **entry name** read, the outcome, and the caller-reported working directory.

What is trustworthy and what isn't — keep them straight:

| Field | Trust level |
|---|---|
| Time, process name, PID | **Server-verified**; the caller cannot forge them (the PID comes from `GetNamedPipeClientProcessId`) |
| Entry name | Server-recorded, but it is the name the caller requested anyway |
| Project, working directory, client | **Caller-reported**; leads only, not proof |

Boundaries to state up front:

- The pipe ACL only allows the current user and SYSTEM, so "the caller" is always some process on this machine. The log can answer "**which process** read it", not "was it you", and it doesn't block anything. It is a **forensic** tool, not a protective one.
- Only **entry names** are recorded, never secret values.
- Memory **only**, never on disk: cleared when the app restarts; no plaintext metadata left behind.
- Unreadable while locked: entry names are the same class of information that must not be revealed while locked.
- Reading the log does not itself get logged.

### `lvault verify`: read-only health check

To check "is my Vault still intact", run once:

```powershell
lvault verify
```

```text
Vault 校验：C:\Users\Admin\.local-api-vault\vault.age
[通过] Vault 文件可读：359 字节
[通过] Vault 文件格式有效：age v1，X25519 身份加密
[通过] 口令 sidecar 存在且格式有效：vault.identity.age
[跳过] 解密校验未执行：未提供恢复身份文件；加 --identity <恢复身份文件> 可验证「能不能真的打开」
[注意] 口令 sidecar 的内容未校验：解开它需要主密码，verify 不索要主密码

结论：文件层面正常，但没做解密校验；加 --identity <恢复身份文件> 可完整验证。
```

- **Read-only**: changes no files, no network
- **Never asks for the master passphrase**: the app doesn't need to be running or unlocked
- **Prints no entry names or secret values**: at most "how many entries, disabled entries, run profiles"
- Exit code `0` = no problems found, `1` = at least one check failed

Add `--identity <recovery identity file>` and it actually decrypts once (in memory only) to answer "can this recovery identity still open this Vault":

```powershell
lvault verify --identity D:\backup\recovery.key
```

The contents of the passphrase-encrypted `vault.identity.age` are **never** verified — opening it requires the master passphrase, which `verify` does not ask for.

## Copy reference (复制参考资料)

The "复制参考资料" (Copy reference) button in the entry editor produces a Markdown snippet with **programmatic redaction** of secrets, login accounts, sensitive URL parameters, and anything credential-shaped in the notes. You don't have to judge what is sensitive yourself — paste the result directly to an AI assistant, a colleague, or project docs.

Label-like information (name, project, environment, interface notes) is kept, otherwise the other party can't locate the record. Redaction errs on the conservative side — rather over-mask.

"导出加密备份" (Export encrypted backup) copies the primary Vault and its `identity.age` sidecar, but does not automatically copy your saved recovery identity private key — back that file up separately yourself. Export also writes a `<backup-name>.sha256` next to the backup containing the backup's SHA-256 fingerprint — later restores can tell you "is this file still exactly what I exported", and you can verify it yourself with `Get-FileHash` or `sha256sum -c`.

## Launch at login (optional)

The "开机自启动" (Launch at login) toggle in the sidebar's "更多 ⋯" menu and the tray right-click menu both control the same setting; off by default. When enabled it only writes one value under the current user's `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, launching the app with `--autostart` at login.

Key point: **auto-start never auto-unlocks**. At login the app only enters the system tray silently and stays locked; reading any secret still requires you to type the master passphrase manually. Turning the toggle off deletes the entry; no administrator rights needed.

## Security boundaries

This project is currently an early prototype and **has not completed an independent security audit**. The crypto libraries and data formats may still change.

The current build is not code-signed, so Windows SmartScreen may warn "unknown publisher". Note:
even with an EV certificate, SmartScreen has not been bypassed since 2024 — reputation must be built through download volume; signing alone cannot remove the first-install warning.

Automated checks already in place (run on every commit and weekly):

| Check | Coverage |
| --- | --- |
| [CodeQL](https://github.com/bigvibe/Local-API-Vault/actions/workflows/codeql.yml) | Static analysis for Rust and frontend |
| Secret scanning | Credential leaks across full git history |
| `cargo audit` | RustSec advisories for Rust dependencies |
| `npm audit` | Known vulnerabilities in frontend dependencies |
| Dependabot | Automated dependency and GitHub Actions updates |

Automated checks catch known defects; **that is not the same as an audit**. Still outstanding:

- Third-party independent security audit
- Windows code signing
- Boundary tests for manual lock and clipboard clearing
- Crash-log redaction
- Backup-corruption and wrong-passphrase recovery tests

See [`docs/security-model.md`](docs/security-model.md), [`docs/mvp.md`](docs/mvp.md), [`docs/client-contract.md`](docs/client-contract.md), [`docs/recipes.md`](docs/recipes.md) (integration guide, Chinese-only) and [`docs/roadmap.md`](docs/roadmap.md) (near-term plan, Chinese-only).

Report vulnerabilities via the repo's **Security → Report a vulnerability**, see [`SECURITY.md`](SECURITY.md).

## License

This project is licensed under Apache-2.0. See [`LICENSE`](LICENSE).
