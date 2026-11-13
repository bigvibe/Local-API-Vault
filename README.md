# Local API Vault

[![CI](https://github.com/bigvibe/Local-API-Vault/actions/workflows/ci.yml/badge.svg)](https://github.com/bigvibe/Local-API-Vault/actions/workflows/ci.yml)
[![Security](https://github.com/bigvibe/Local-API-Vault/actions/workflows/security.yml/badge.svg)](https://github.com/bigvibe/Local-API-Vault/actions/workflows/security.yml)
[![CodeQL](https://github.com/bigvibe/Local-API-Vault/actions/workflows/codeql.yml/badge.svg)](https://github.com/bigvibe/Local-API-Vault/actions/workflows/codeql.yml)
[![Release](https://github.com/bigvibe/Local-API-Vault/actions/workflows/release.yml/badge.svg)](https://github.com/bigvibe/Local-API-Vault/releases)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Platform: Windows](https://img.shields.io/badge/platform-Windows-0078D6?logo=windows)](https://learn.microsoft.com/windows/)

一个默认离线、面向开发者的本地 API 密钥保险箱。

> 当前状态：MVP 开发中。请不要在审计完成前存放唯一的高价值凭据。

## 下载安装

到 [Releases 页面](https://github.com/bigvibe/Local-API-Vault/releases) 下载最新版本，
双击 `_x64-setup.exe` 即可。

文件名形如 `Local.API.Vault_<版本>_x64-setup.exe`。注意 GitHub 会把产品名里的**空格
替换成点号**，所以是 `Local.API.Vault` 而不是 `Local API Vault` —— 在下载列表里按
`_x64-setup.exe` 找最不容易认错。

- 安装到当前用户目录（`%LOCALAPPDATA%`），**不需要管理员权限**
- 卸载走「设置 → 应用」，会一并清理开机自启动项

首次启动会让你创建 Vault 并设置主密码。**主密码无法找回**，界面会同时给出一份恢复身份
文件，请离线保存好。

### 校验下载的文件

GitHub 在资产列表下方会显示该文件的 SHA-256，可与本地文件比对：

```powershell
Get-FileHash .\Local.API.Vault_<版本>_x64-setup.exe -Algorithm SHA256
```

### 安装后

```powershell
lvault get <你的条目名>
```

`lvault` 已随应用安装并加入 `PATH`，但**只对新开的终端生效**——装完之后请重新开一个
PowerShell 窗口。如果提示找不到命令，看[CLI 一节](#cli)。

> **Windows 会提示「未知发布者」**
>
> 本项目的构建产物没有代码签名，所以 SmartScreen 会拦一下。这是预期行为，不是感染。
> 点「更多信息」→「仍要运行」可跳过。原因与应对见[安全边界](#安全边界)。

## 目标

- 本地保存 API Key、Token、签名密钥和环境配置
- 默认不联网、不注册、不上传数据
- 支持项目、环境、掩码显示、复制和手动锁定
- 通过 Windows Named Pipe + `lvault` CLI 把密钥注入本地命令，不生成 `.env` 文件

## 适合谁 / 不适合谁

**适合：** 单用户、本机、项目能读环境变量（env / `os.environ` / `process.env`）。密钥值只用于
本地开发工具或脚本，不需要多人共享、不需要云端。

**不适合：**

- 多台机器同步 / 团队共享密钥 —— 没有账号系统和共享 Vault，这是明确的边界
- CI/CD 注入密钥 —— 没有非交互解锁 API（刻意为之，避免主密码出现在脚本里），用 CI 平台的 secret store
- 密钥自动轮换 / 续期 —— 不做，轮换是手动流程
- 需要「明文配置文件」的项目 —— 本工具不会写出含密钥的文件；改用环境变量插值，否则换方案
- 跨操作系统 —— 目前仅支持 Windows

完整的接入方法（Python / Node / 配置文件插值 / 换机器 / 轮换 / CI 为什么不行…）见
[Recipes：如何在真实项目里用起来](docs/recipes.md)。

## MVP 范围

- 创建、打开、重建和锁定 Vault
- 解锁后默认持续使用，用户可随时手动锁定；应用退出后会重新锁定
- 重建时保留旧文件并创建空 Vault
- 自动生成 X25519 恢复身份，并提供恢复入口
- 添加、编辑、删除、停用/启用和搜索条目
- 停用条目保留在 Vault 中，但普通客户端和 CLI 不会返回
- 加密文件存储
- 复制密钥并在 30 秒后清除剪贴板
- “复制参考资料”自动遮蔽密钥与账号，生成可安全分享的说明文本
- 从 `.env` 文件导入键值
  - 导入**不过滤任何键值**（包括路径、开关等“不像密钥”的配置项）——宁可多存勿漏存；
    导入后可在条目列表里逐个删除你不需要保留的项
- 原生文件选择器和加密备份导出
- `lvault verify` 只读校验 Vault 完整性（不索要主密码、不打印密钥值）
- `lvault audit` 读取记录：时间、服务端核实的进程与 PID、条目名（只放内存，不落盘）
- 应用侧边栏「读取记录」面板：同一份记录，不用切终端
- 修改主密码时重新加密 Vault
- 最小化到系统托盘，托盘菜单可重新显示或退出
- 可选开机自启动，登录后静默进入托盘且保持锁定
- 恢复身份可以轮换（旧的那把立即失效）
- 从加密备份恢复，备份带 SHA-256 指纹可校验
- 目前只承诺 Windows；macOS / Linux 暂不在计划内

## 技术栈

- Tauri 2
- Rust
- Vanilla JavaScript / CSS
- `age` 兼容的加密文件
- 版本化 JSON 文档

### 新 Vault 的文件关系

```text
vault.age          # X25519 公钥加密的主 Vault
vault.identity.age # 主密码加密的 X25519 身份 sidecar
recovery.key       # 用户自行保存的恢复身份文件
```

主密码通过 sidecar 解出 X25519 身份，再打开主 Vault；恢复身份文件则可以绕过主密码直接打开主 Vault。应用不会自动把恢复私钥上传或保存到云端。

首次创建或重建 Vault 时，应用会显示一次恢复身份密钥。请将它保存到 Vault 目录之外。

用久了之后，目录里还可能出现这些（都是应用自己写的，删掉不影响主 Vault）：

```text
vault.before-rotate-<时间戳>.age    # 换发恢复身份前的旧 Vault（和它的 .identity.age 配成一对）
vault.before-restore-<时间戳>.age   # 从备份恢复前的旧 Vault
<备份名>.sha256                     # 备份的 SHA-256 指纹
```

### 忘了主密码怎么办

在首屏点**「恢复 Vault」**，向导会一步步带你走：选回原来的 Vault 文件、选出你保存的恢复身份文件、设一个新的主密码。恢复完成后它会告诉你三件事：条目都还在、旧主密码立即失效、恢复身份文件**依然有效**（以后忘了新密码还能用它）。

首屏那一块「忘记主密码了？」专门说明**恢复**和**重建**的区别，别点错：

- **恢复 Vault** —— 打开**原来的数据**，条目一个不少
- **重建 Vault** —— 创建**空的**新 Vault；原文件保留成 `vault.age.before-rebuild-…`，旧主密码仍能打开它，但当前这个 Vault 是空的

没有恢复身份文件就只剩重建这一条路：主密码本身没有任何找回途径。

### 怀疑恢复密钥泄露了？

侧栏的**「更换恢复密钥」**（原来叫「设置恢复密钥」，有恢复身份后自动变成这个）会换发一把新的：

- 换发后，**旧的那把恢复密钥立刻失效** —— 它再也打不开这个 Vault
- 新的这把必须保存好；旧的那把可以销毁
- 换发前的 Vault 保留在旁边（`vault.before-rotate-…`），里面还是旧密钥能打开的旧数据，不会丢
- 想确认某个恢复身份还在不在：`lvault verify --identity <文件>`，打不开就说明已经失效

### 从备份恢复

侧栏**「从备份恢复」**：选回你导出的那份备份 → 输入它原来的主密码 → 校验通过后恢复。

- 备份是**一对**文件：`vault.age` 和旁边的 `vault.identity.age`，要放在同一个目录里
- 校验只报**数量**（几个条目、几个已停用、几个运行配置），不报名字和值
- 校验还会比对 **SHA-256 指纹**：导出时会在备份旁边写一个 `<备份名>.sha256`，
  恢复时如果文件被动过（哪怕只改了一个字节），会直接告诉你「和导出时不一样」
- 恢复用备份**原来的主密码**，恢复前的 Vault 保留在旁边（`vault.before-restore-…`）
- 恢复**保留原来的身份**，所以你的恢复密钥文件照样能打开恢复后的 Vault
- 如果这份备份的主密码曾经泄露过，恢复完记得去侧栏「修改主密码」换一把

## 开发

环境要求：

- Node.js 22+
- Rust stable
- Windows 构建需要 Visual Studio C++ Build Tools
- Tauri 所需的 WebView2 运行时（现代 Windows 通常已安装）

```powershell
npm install
npm run tauri dev
```

构建：

```powershell
npm run tauri build
```

## CLI

CLI 从安全提示读取主密码，不接受把主密码放在命令行参数中：

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin local-api-vault-cli -- list --vault "$HOME/.local-api-vault/vault.age"
cargo run --manifest-path src-tauri/Cargo.toml --bin local-api-vault-cli -- get OPENAI_API_KEY --vault "$HOME/.local-api-vault/vault.age"
```

`get` 只输出对应密钥的值，适合在受控脚本中读取；不要把输出直接写进日志或命令历史。已停用条目不会出现在 `list` 中，也不能通过 `get` 读取。

Windows Named Pipe Bridge 已内置，管道名为 `\\.\pipe\LocalApiVault-v1`。应用解锁后可由本地客户端读取，停用或锁定时返回对应错误：

```powershell
local-api-vault-cli pipe-get OPENAI_API_KEY --project web-app --environment development
```

Bridge 只监听 Windows Named Pipe，不占用 TCP 端口。

应用侧边栏的“运行配置”可以保存项目目录、启动命令和密钥名称；保存后直接运行：

```powershell
local-api-vault-cli run web-dev
```

也可以临时直接把密钥注入一个本地命令，不生成 `.env`：

```powershell
local-api-vault-cli run --cwd D:\projects\web --secret OPENAI_API_KEY -- npm run dev
```

`run` 会先通过 Pipe 检查 Vault 是否解锁、条目是否启用，再把密钥作为子进程环境变量传入。

### 项目级自动发现：`.lvault.toml`

在项目根目录放一个 `.lvault.toml`，只写要注入的**条目名**（也就是环境变量名），没有值，可以安全提交进仓库：

```toml
vars = ["OPENAI_API_KEY", "NEWSFEED_API_KEY"]
```

之后在该目录运行，就不必再逐个列 `--secret`：

```powershell
lvault run -- npm run dev
```

- 文件里只有名字，**绝不会出现密钥值**；格式写错会直接报错，而不是半懂不懂地接受
- 只在**当前目录**查找，不会向父目录递归，避免把父项目的条目带进别的 shell
- 同时给了 `--secret` 时以命令行为准，`.lvault.toml` 不参与
- 上限：16 KB、最多 64 个名字、只允许顶层 `vars`

`lvault` 是安装目录下的安全包装命令，只暴露四个动词：

```powershell
lvault get OPENAI_API_KEY --project web-app --environment development
lvault run web-dev
lvault verify
lvault audit
```

`lvault get` 读取时不会索要主密码，它只会通过已解锁应用的 Named Pipe 取值，因此适合脚本和 AI 助手调用。具体约定见 [`AGENTS.md`](AGENTS.md)。

### `lvault audit`：谁读过我的密钥

想知道「这个密钥都被谁读过了」，跑一次：

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

不想切终端也行：应用侧边栏的**「读取记录」**打开的就是同一份记录，可以刷新。

每条记录包含：**服务端时间**、**调用方进程与 PID**、被读的**条目名**、结果，以及调用方自报的工作目录。

哪些可信，哪些不可信，务必分清：

| 字段 | 可信度 |
|---|---|
| 时间、进程名、PID | **服务端核实**，调用方改不了（PID 来自 `GetNamedPipeClientProcessId`） |
| 条目名 | 服务端记录，但它本来就是调用方请求的名字 |
| 项目、工作目录、客户端 | **调用方自报**，只能当线索 |

边界要说在前面：

- 管道 ACL 只允许当前用户和 SYSTEM，所以「调用方」永远是这台机器上的某个进程。
  记录能回答「**哪个进程**读的」，回答不了「是不是你本人」，也拦不住它。这是**事后追查**工具，不是防护工具。
- 只记**条目名**，从不记密钥值。
- 只放**内存**，不落盘：重启应用即清空，也不会有明文元数据留在磁盘上。
- 锁定时读不到记录：条目名和未锁定时能看到的名称是同一类信息，锁着就不给。
- 读记录本身不会被记进记录。

### `lvault verify`：只读体检

想知道「我的 Vault 还是好的吗」，跑一次就行：

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

- **只读**：不改任何文件，不联网
- **不索要主密码**：也不需要应用在运行或解锁
- **不打印任何条目名或密钥值**：最多报「几个条目、几个已停用、几个运行配置」
- 退出码 `0` = 没发现问题，`1` = 有失败项

再加上 `--identity <恢复身份文件>`，它会真的解密一次（只在内存里），用来回答「这份恢复身份到底还能不能打开这个 Vault」：

```powershell
lvault verify --identity D:\backup\recovery.key
```

主密码加密的 `vault.identity.age` **内容**始终不会被校验——解开它需要主密码，而 `verify` 不索要主密码。

## 复制参考资料

条目编辑页的“复制参考资料”会生成一段 Markdown，**由程序自动遮蔽**密钥、登录账号、URL 中的敏感参数，以及备注里任何看起来像凭据的内容。你不需要自己判断什么该隐藏，可以直接把结果粘贴给 AI 助手、同事，或写进项目文档。

标签类信息（名称、项目、环境、备注中的接口说明）会保留，否则对方无法定位这条记录。遮蔽规则偏保守，宁可多遮。

“导出加密备份”会复制主 Vault 和它的 `identity.age` sidecar，但不会自动复制你保存的恢复身份私钥；恢复身份文件需要你自己单独备份。导出时还会在备份旁边写一个 `<备份名>.sha256`，里面是这份备份的 SHA-256 指纹 —— 以后恢复时它能告诉你「这份文件还和导出时一模一样吗」，也能用 `Get-FileHash` 或 `sha256sum -c` 自己核对。

## 开机自启动（可选）

侧边栏的“开机自启动”开关和托盘右键菜单里的“开机自启动”复选框都可以切换，默认关闭。开启后只会在当前用户的 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 写入一项，登录时以 `--autostart` 参数启动程序。

关键点：**自启动不会自动解锁**。程序登录后只静默进入系统托盘并保持锁定，读到任何密钥仍然需要你手动输入主密码。关闭开关即删除该项，不需要管理员权限。

## 安全边界

本项目目前是早期原型，**尚未完成独立安全审计**。加密库和数据格式仍可能发生变化。

当前构建不包含代码签名，因此 Windows SmartScreen 可能提示「未知发布者」。请注意：
即便购买 EV 证书，自 2024 年起也不再绕过 SmartScreen，信誉需要靠下载量积累，
签名本身不能消除首次安装的警告。

已经做到的自动化检查（每次提交与每周定时运行）：

| 检查 | 覆盖范围 |
| --- | --- |
| [CodeQL](https://github.com/bigvibe/Local-API-Vault/actions/workflows/codeql.yml) | Rust 与前端静态分析 |
| 密钥扫描 | 全量 git 历史中的凭据泄漏 |
| `cargo audit` | Rust 依赖的 RustSec 公告 |
| `npm audit` | 前端依赖的已知漏洞 |
| Dependabot | 依赖与 GitHub Actions 自动更新 |

自动化检查能发现已知缺陷，**不等于经过审计**。尚未完成的项：

- 第三方独立安全审计
- Windows 代码签名
- 手动锁定和剪贴板清理的边界测试
- 崩溃日志脱敏
- 备份损坏和密码错误恢复测试

请查看 [`docs/security-model.md`](docs/security-model.md)、[`docs/mvp.md`](docs/mvp.md)、[`docs/client-contract.md`](docs/client-contract.md) 和 [`docs/recipes.md`](docs/recipes.md)（接入指南）和 [`docs/roadmap.md`](docs/roadmap.md)（近期计划）。

报告漏洞请使用仓库的 **Security → Report a vulnerability**，详见 [`SECURITY.md`](SECURITY.md)。

## 许可证

本项目采用 Apache-2.0，详见 [`LICENSE`](LICENSE)。
