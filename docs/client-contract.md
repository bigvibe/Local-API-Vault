# Local Client Contract（v0.1）

这是 Local API Vault 对未来本地客户端/桥接进程提供的逻辑契约。

> 当前已提供 Windows Named Pipe 的最小本地桥接；仍不在 Vault 核心中实现 HTTP 监听或固定端口。

## 核心语义

```text
Vault 已锁定 + 条目任意状态   → 拒绝读取
Vault 已解锁 + 条目已停用     → 拒绝读取
Vault 已解锁 + 条目已启用     → 允许读取
```

条目被删除后不再存在于 Vault；“停用”条目仍保留在加密文件中，可以重新启用。

Named Pipe 还支持 `operation: "profile"`，只返回运行配置的目录、命令和密钥名称，不返回密钥值。

## 读取请求

```json
{
  "version": 1,
  "project": "web-app",
  "environment": "development",
  "name": "OPENAI_API_KEY"
}
```

约束：

- `name` 必填
- `environment` 建议必填
- `project` 可以为空，表示默认项目
- 不允许通过 URL、命令行参数或日志传递 Vault 主密码
- 不允许客户端直接读取 `vault.age` 或 `vault.identity.age`

## 成功响应

```json
{
  "version": 1,
  "name": "OPENAI_API_KEY",
  "value": "sk-...",
  "entryVersion": 3
}
```

响应约定：

- `value` 只在内存中短暂存在
- 不写入日志、缓存目录或错误信息
- 客户端不得把返回值写入 Git、`.env` 或命令行历史
- 如果未来使用 HTTP，必须绑定回环地址并禁止缓存

## 错误响应

```json
{
  "version": 1,
  "code": "vault_locked",
  "message": "Vault is locked"
}
```

错误码：

| code | 含义 |
|---|---|
| `invalid_request` | 请求字段不完整或格式错误 |
| `vault_locked` | 当前 Vault 未解锁 |
| `not_found` | 找不到对应条目 |
| `disabled` | 条目已停用 |
| `unauthorized` | 客户端没有访问权限（桥接阶段） |
| `internal_error` | 内部错误，不包含敏感数据 |

调用方遇到 `vault_locked` 或 `disabled` 时应停止读取，不应回退到明文 `.env`。

## 传输层边界

本契约与传输方式无关。未来可以有：

1. Windows Named Pipe（当前已实现，名称：`\\.\pipe\LocalApiVault-v1`）
2. 仅绑定 `127.0.0.1` 的本地 HTTP（未来可选）
3. 项目内置 CLI

Named Pipe 的 ACL 限制为创建者和 SYSTEM；它不会区分同一 Windows 用户下的不同程序，因此同用户的可信进程都可以访问已启用条目。

核心 Vault 不监听外部网络地址，也不占用固定 TCP 端口。

## 如何集成（5 分钟）

Vault 应用需要**已启动且已解锁**。默认解锁状态会持续一整天，每天解锁一次即可。

### 方式一：命令行（推荐）

```powershell
lvault get OPENAI_API_KEY
lvault get OPENAI_API_KEY --project web-app --environment development
```

赋给环境变量，而不是打印到屏幕：

```powershell
$env:OPENAI_API_KEY = (lvault get OPENAI_API_KEY)
```

或者让 Vault 直接启动命令并注入变量（不会生成 `.env`）：

```powershell
lvault run web-dev
```

`lvault` 与应用安装在同一目录，只暴露 `get` 和 `run`。完整约定见
[`AGENTS.md`](../AGENTS.md)。

### 方式二：直接使用 Pipe 协议

管道名 `\\.\pipe\LocalApiVault-v1`。一次连接写入一行 JSON 请求，读取一行
JSON 响应后关闭。请求中的 `operation` 可省略，缺省为 `get`。

请求：

```json
{"version":1,"operation":"get","name":"OPENAI_API_KEY","project":"web-app","environment":"development"}
```

成功响应：

```json
{"version":1,"ok":true,"code":"ok","message":"ok","value":"sk-...","entryVersion":3}
```

失败响应：

```json
{"version":1,"ok":false,"code":"vault_locked","message":"Vault is locked"}
```

当名称接近但不完全匹配时，错误响应可附带 `suggestions`——**仅名称，永不包含值，且仅在已解锁时产生**：

```json
{
  "version": 1,
  "ok": false,
  "code": "not_found",
  "message": "secret not found",
  "suggestions": ["EXAMPLE_API_KEY"]
}
```

`suggestions` 是可选字段，`message` 保持稳定的英文（供程序判断）；面向人的中文
提示由客户端（如 `lvault`）根据 `code` 生成。这让"名称记不清"变成可操作的提示，
而不需要给 pipe 增加"列出全部条目"的能力——那会扩大读取面。

Python 示例：

```python
import json

PIPE = r"\\.\pipe\LocalApiVault-v1"


def get_secret(name, project=None, environment=None):
    request = {
        "version": 1,
        "operation": "get",
        "name": name,
        "project": project,
        "environment": environment,
    }
    with open(PIPE, "r+b", buffering=0) as pipe:
        pipe.write((json.dumps(request) + "\n").encode())
        pipe.flush()
        response = json.loads(pipe.readline())
    if not response.get("ok"):
        raise RuntimeError(response.get("code", "unknown"))
    return response["value"]
```

### 收到错误时应该做什么

| code | 处理方式 |
|---|---|
| `vault_locked` | 请用户解锁 Vault，不要尝试绕过 |
| `disabled` | 条目已被停用，交由用户决定是否重新启用 |
| `not_found` | 核对 `name`、`project`、`environment` 是否匹配 |
| `internal_error` | 属于 Vault 侧问题，记录 code 即可，不要记录任何 secret |

### 不要做的事

- 不要在失败时回退到明文 `.env`、命令行参数或硬编码密钥
- 不要缓存 `value`，也不要写入日志、崩溃报告或版本库
- 不要尝试读取或解密 `vault.age` / `vault.identity.age`
- 不要把主密码或恢复密钥放进任何自动化流程

## 版本策略

- `version` 当前固定为 `1`
- 新增字段保持向后兼容
- 删除字段或改变语义必须升级主版本
- 错误码只增加，不随意改变含义

`version` 与 age 的接收者模式相互独立。新增字段必须带默认值，加字段时不改动
`version`。`vault::tests::opens_and_rewrites_a_pre_profiles_vault` 会用一个不含
`profiles` 和 `enabled` 的旧格式文件做端到端回归，任何破坏兼容的改动都会让它失败。

## 当前实现状态

- Vault 内部已有按 ID 读取的 Tauri 命令
- CLI 已拒绝读取停用条目
- Windows Named Pipe Bridge 已在应用进程内运行
- 当前支持 `get` 请求、`profile` 请求和 CLI `run` 注入环境变量
- 运行配置保存在加密 Vault 中，只包含引用信息
- 安装目录提供 `lvault` 包装命令，只暴露 `get` 与 `run`
- 不支持通过 Pipe 列目录
- 尚未实现 HTTP 监听、固定 TCP 端口或量化策略适配
