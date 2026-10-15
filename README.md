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

到 [Releases 页面](https://github.com/bigvibe/Local-API-Vault/releases) 下载
`Local_API_Vault_<版本>_x64-setup.exe`，双击安装即可。

（GitHub 会把资产名里的空格替换成下划线，所以是 `Local_API_Vault` 而不是
`Local API Vault`。）

- 安装到当前用户目录（`%LOCALAPPDATA%`），**不需要管理员权限**
- 卸载走「设置 → 应用」，会一并清理开机自启动项

首次启动会让你创建 Vault 并设置主密码。**主密码无法找回**，界面会同时给出一份恢复身份
文件，请离线保存好。

### 安装后

```powershell
lvault get <你的条目名>
```

`lvault` 已随应用安装并加入 `PATH`，但**只对新开的终端生效**——装完之后请重新开一个
PowerShell 窗口。如果提示找不到命令，看[CLI 一节](#cli)。

> **Windows 会提示「未知发布者」**
>
> 本项目的构建产物没有代码签名，所以 SmartScreen 会拦一下。这是预期行为，不是感染。
> 原因与应对见[安全边界](#安全边界)。

## 目标

- 本地保存 API Key、Token、签名密钥和环境配置
- 默认不联网、不注册、不上传数据
- 支持项目、环境、掩码显示、复制和手动锁定
- 通过 Windows Named Pipe + `lvault` CLI 把密钥注入本地命令，不生成 `.env` 文件

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
- 原生文件选择器和加密备份导出
- 修改主密码时重新加密 Vault
- 最小化到系统托盘，托盘菜单可重新显示或退出
- 可选开机自启动，登录后静默进入托盘且保持锁定
- Windows 优先，后续扩展 macOS/Linux

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

首次创建或重建 Vault 时，应用会显示一次恢复身份密钥。请将它保存到 Vault 目录之外；如果以后忘记主密码，可在 onboarding 中选择“恢复 Vault”，使用该身份文件设置新密码。

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

`lvault` 是安装目录下的安全包装命令，只暴露两个动词：

```powershell
lvault get OPENAI_API_KEY --project web-app --environment development
lvault run web-dev
```

`lvault get` 读取时不会索要主密码，它只会通过已解锁应用的 Named Pipe 取值，因此适合脚本和 AI 助手调用。具体约定见 [`AGENTS.md`](AGENTS.md)。

## 复制参考资料

条目编辑页的“复制参考资料”会生成一段 Markdown，**由程序自动遮蔽**密钥、登录账号、URL 中的敏感参数，以及备注里任何看起来像凭据的内容。你不需要自己判断什么该隐藏，可以直接把结果粘贴给 AI 助手、同事，或写进项目文档。

标签类信息（名称、项目、环境、备注中的接口说明）会保留，否则对方无法定位这条记录。遮蔽规则偏保守，宁可多遮。

“导出加密备份”会复制主 Vault 和它的 `identity.age` sidecar，但不会自动复制你保存的恢复身份私钥；恢复身份文件需要你自己单独备份。

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

请查看 [`docs/security-model.md`](docs/security-model.md)、[`docs/mvp.md`](docs/mvp.md) 和 [`docs/client-contract.md`](docs/client-contract.md)。

报告漏洞请使用仓库的 **Security → Report a vulnerability**，详见 [`SECURITY.md`](SECURITY.md)。

## 许可证

本项目采用 Apache-2.0，详见 [`LICENSE`](LICENSE)。
