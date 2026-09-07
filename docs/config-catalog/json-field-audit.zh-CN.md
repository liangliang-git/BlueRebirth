# 服务端 JSON 字段审计

服务端配置读取入口为 `rust-server/crates/server/src/catalog_loader.rs`。
`tools/prune-rust-catalog-json.py` 按该 loader 的实际字段访问建立白名单：

- typed 配置表：只保留 Rust 当前读取的顶层字段，保留数组内容原样；
- raw/动态配置表：完整保留，避免活动规则、接口透传和未来字段被误删；
- 未映射新表：默认保留并输出审计警告，不静默删除。

当前日服目录状态：

- 100 个 `config_*.json`；
- 83,675 行；
- 本地目录、部署目录均为 JSON-only，`config_*.db` 数量为 0；
- 两目录 JSON SHA-256 完全一致；
- 本轮删除 3,963,942 个冗余字段实例，双目录合计约 92.4 MB；
- raw 表未裁剪，nested array 未裁剪。

复查或重新导出后执行：

```powershell
python .\tools\prune-rust-catalog-json.py .\rust-server\catalog\config
python .\tools\prune-rust-catalog-json.py --apply .\rust-server\catalog\config
```

`tools/export-rust-catalog.ps1 -ConfigFormat json` 已自动执行 `--apply`。新增
`config_*.db` 表若未进入 loader 白名单，会保持原字段并提示 `UNMAPPED TABLES`。
