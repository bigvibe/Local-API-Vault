# Recovery Key Design

## 目标

在用户忘记主密码时，允许使用独立的高强度恢复身份访问原 Vault；没有恢复身份时只能重建空 Vault，不能绕过加密。

## 方案

age 规范不允许在同一个 age 文件中混用 passphrase recipient 和公钥 recipient。因此采用两层文件：

- `vault.age`：使用随机 X25519 identity 的 public recipient 加密 Vault 文档
- `vault.identity.age`：使用主密码派生的 scrypt recipient 加密同一个 X25519 identity

用户保存的恢复身份文件就是这个 X25519 identity 的私钥。应用不会把私钥写入普通 JSON、Vault 文档或日志。

这种设计避免了自定义混合加密格式，也允许使用标准 age 工具检查每个加密文件。

## 用户流程

### 创建

- 创建 Vault 时生成随机 X25519 identity
- 用 identity 的 public recipient 加密主 Vault
- 用主密码加密 identity sidecar
- 将 identity private key 展示一次，并允许用户保存为 age identity 文件
- 如果用户没有保存恢复身份文件，明确提示恢复能力不可用

### 忘记主密码

界面上的「恢复 Vault」是一个三步向导，按顺序做：

- 选择原 Vault 文件
- 选择保存的 recovery identity 文件
- 输入新的主密码

底下做的事：

- 使用 recovery identity 解密主 Vault
- 用同一个 identity 和新主密码重新写入主 Vault 与 identity sidecar

因为 identity 没有换，恢复身份文件在恢复之后**依然有效**——它始终是「等同主密码」的那把钥匙，
必须继续保存好。旧主密码则立即失效。

恢复身份文件与 Vault 不匹配时，报错说的是「恢复身份与这个 Vault 不匹配，或 Vault 文件已损坏」，
不会说「主密码错误」——这条路径里用户根本没有输入主密码。

恢复身份文件本身即可解密主 Vault；identity sidecar 主要用于主密码解锁。

### 修改主密码

只重新加密 `vault.identity.age` sidecar，主 Vault 的 X25519 密文和恢复身份不变。

### 旧 Vault 迁移

没有 identity sidecar 的旧 Vault 继续使用原有 passphrase-only 格式。用户可以在正常解锁后显式启用恢复能力，迁移到新格式。

## 安全要求

- recovery identity 是高熵随机密钥，不使用短口令
- recovery identity 文件写入临时文件后原子替换
- recovery identity 文件权限限制为当前用户（Unix 使用私有文件模式）
- 不在日志、错误和崩溃报告中记录 recovery identity
- recovery identity 不自动上传、同步或写入 Vault 所在目录
- 恢复失败不能删除或覆盖原文件
- 修改恢复能力、密码和备份后必须重新测试
- 用户应同时备份主 Vault、identity sidecar 和 recovery identity 文件

## 兼容性

- 没有 `vault.identity.age` 的旧 Vault 继续只支持主密码打开
- 通过“设置恢复密钥”显式迁移
- 恢复流程不能把没有 recovery identity 的旧文件当作可恢复文件
- 旧格式和新格式的密码修改都需要保留原文件，直到用户确认新文件可打开

## 暂不实现

- 云端恢复
- 邮箱找回
- 恢复码托管
- 自动把私钥放入系统钥匙串
