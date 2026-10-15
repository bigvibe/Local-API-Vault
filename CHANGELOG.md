# Changelog

## Unreleased

- 初始 Tauri + Rust 桌面 MVP
- 使用 age 兼容的加密 Vault 文件
- 创建、打开、锁定和编辑 API 密钥
- 搜索、掩码显示、复制后定时清理剪贴板
- 从 `.env` 文件导入环境变量
- 本地 CLI 读取和列出条目
- 基础安全模型和单元测试
- Windows NSIS 安装包构建配置
- 原生 Vault/`.env` 文件选择器
- 加密 Vault 备份导出
- 主密码验证与重新加密修改
- 安全的 Vault 重建流程（保留旧文件）
- X25519 恢复身份、身份 sidecar 和恢复流程
- 旧 passphrase-only Vault 的显式迁移
- 条目停用/启用状态和已停用筛选
- 普通客户端读取接口对停用条目拒绝访问
- 默认持续解锁会话，保留手动锁定
- 最小化到系统托盘，托盘菜单支持显示和退出
- 增加 Windows Named Pipe 本地 Bridge 和 `pipe-get` CLI
- Named Pipe 使用当前用户/系统 ACL，不开放 TCP 端口
- `run` 命令通过 Pipe 注入环境变量并启动本地命令
- 应用内运行配置管理，支持 `run <profile>`
- 可选开机自启动，登录后静默进入托盘并保持锁定
- 安装 `lvault` 安全包装命令（只暴露 `get` / `run`）
- 新增 `AGENTS.md`，约定 agent 获取密钥的方式与禁止事项
- 新增旧格式 vault 的端到端向后兼容回归测试
- `client-contract.md` 增加“如何集成”快速上手章节
- 条目新增“复制参考资料”：自动遮蔽凭据后生成可分享的 Markdown
- 新增 `docs/fixtures/` 约定：第三方接口的脱敏响应样例放仓库，契约写 `docs/`
- `lvault` / CLI 错误提示改为中文并给出下一步操作（锁定→点托盘解锁、停用→去应用启用等）
- 名称不匹配时返回相近条目名建议（仅名称，仅解锁时），无需给 pipe 增加列目录能力
- 新增 `security.yml`：gitleaks 全历史密钥扫描、`cargo audit`、`npm audit`
- 新增 `codeql.yml`：Rust 与前端每周静态分析
- 新增 `dependabot.yml`：依赖与 GitHub Actions 自动更新
- 修复 `source-map-js` 高危公告（GHSA-68fv-2mgg-jv7q），1.2.1 → 1.2.2
