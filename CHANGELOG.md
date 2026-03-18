# Changelog

## [0.2.0] - 2026-03-19

### Breaking Changes

- **拆分 `TunnelConfig`** 为 `HostConfig` 和 `JoinConfig`，各端只暴露相关配置字段
- **包装 iroh re-export**：`RelayUrl` 和 `SecretKey` 改为 newtype，隔离上游 breaking change
- **所有公共枚举和结构体添加 `#[non_exhaustive]`**，下游 match 需加通配分支
- **`TunnelEvent` 中 `id` / `remote_id` 字段类型从 `String` 改为 `PeerId`**
- **`ConnectionSnapshot::timestamp` (`Instant`) 改为 `elapsed` (`Duration`)**
- **`TunnelError` 变体从 `String` 改为 `BoxError`**，保留完整错误链

### Features

- 升级 iroh 0.96 → 0.97，适配 preset builder API 和 `Option<PathStats>` 返回值
- `HostConfig` / `JoinConfig` 提供 builder 模式构造
- TUI 连接成功及关闭后自动清空密码输入框

### Internal

- 新增 `types` 模块统一管理 newtype 封装
- 新增 `PeerId` 类型，实现 `Display` / `AsRef<str>` / `Clone` / `Eq` / `Hash`
- 新增 `BoxError` 类型别名 (`Box<dyn Error + Send + Sync>`)

## [0.1.0] - 2026-03-03

初始发布。
