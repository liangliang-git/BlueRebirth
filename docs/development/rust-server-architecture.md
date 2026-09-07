# Rust 服务端架构改造进度

服务端按“wire → router → feature handler → domain → repository → SQLite”分层迁移。

当前已落地：

- `blueoath-domain`：强类型 ID、资源账本、领域错误、`AccountRepository` 契约。
- `blueoath-protocol`：通用 varint 字段读取、`Decode` trait、`CopyStartRequest` typed DTO；未知字段跳过，必填字段缺失和重复字段拒绝。
- `blueoath-server::router`：集中 `GameMethod`/`MethodFamily` 分类；dispatcher 不再重复对已迁移路由做 `starts_with`。
- `server::common`：请求上下文、响应/推送效果、错误、ID、Clock、校验、分页、可注入 RNG。
- `storage`：启动迁移、schema version、SQLite `foreign_keys`/WAL/busy timeout、账号 revision CAS；新增规范化核心表和索引。
- 未知 game-login 方法返回明确错误响应；已知但暂未迁移方法继续返回兼容空 protobuf。

迁移规则：

1. 新请求先在 `protocol` 定义 typed DTO，再接入对应 feature handler。
2. 领域修改经 `AccountRepository::transact` 完成；奖励、资源和进度必须在同一事务内提交。
3. 旧 JSON snapshot API 只作为过渡适配器；完成模块迁移后删除 `accounts.account_json` 读写路径。
4. 每个迁移文件只追加不修改；服务启动发现 schema version 超前时拒绝启动。

验证：

```powershell
cargo fmt --manifest-path .\rust-server\Cargo.toml --all -- --check
cargo test --manifest-path .\rust-server\Cargo.toml --workspace
cargo clippy --manifest-path .\rust-server\Cargo.toml --workspace --all-targets -- -D warnings
```
