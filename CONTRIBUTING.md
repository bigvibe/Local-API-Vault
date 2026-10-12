# Contributing

感谢参与 Local API Vault。

## 开发原则

- 优先复用成熟、经过审查的密码学库
- 不在日志、错误信息或崩溃报告中记录密钥
- 新增网络能力前先说明必要性，并默认关闭
- 所有 Vault 数据格式变更必须提供迁移策略
- 涉及加密、文件权限和进程启动的改动需要补充测试

## 本地开发

```powershell
npm install
npm run tauri dev
```

提交前请运行：

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
