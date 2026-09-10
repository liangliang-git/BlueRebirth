# 服务端配置字段审计

服务端配置读取入口为 `rust-server/crates/server/src/catalog_loader.rs`。
`tools/prune-rust-catalog-json.py` 按该 loader 的实际字段访问建立白名单：

- typed 配置表：只保留 Rust 当前读取的顶层字段，保留数组内容原样；
- raw/动态配置表：完整保留，避免活动规则、接口透传和未来字段被误删；
- 未映射新表：默认保留并输出审计警告，不静默删除。

当前目录状态：

- `catalog.db` 包含 87 个配置数据表，每个表使用 `id` 加上 `value` 下的顶层键作为字段；
- 标量按 SQLite 类型保存，数组和对象在对应字段中保存为 JSON 文本；
- 如果配置值自身包含 `id`，该字段导出为 `value_id`，避免与行主键冲突；
- `catalog_columns` 保存原始键名、字段类型和字段顺序，便于程序恢复 JSON；
- 掉落数量、服务器商城商品/价格、邮件模板已并入同一个数据库；
- 服务端启动时只读数据库，并将目录加载到内存 typed catalog。

复查或重新导出后执行：

```powershell
python .\tools\build-catalog-db.py
```

`tools/export-rust-catalog.ps1 -ConfigFormat json` 会先生成 JSON 源，再自动构建
`catalog.db`。新增客户端配置表会自动进入独立数据表；服务端 loader 未引用的表仍可
保留，后续可在导入校验中清理。
