# BlueOath Rust Server

Rust server is the canonical local server. Current slice provides:

- `blueoath-transport`: compatible 4-byte big-endian local frame codec, 4 MiB limit.
- `blueoath-protocol`: NetSocket, TMessage, game-login protobuf, `user.GetUserInfo`,
  `hero.UpdateHeroBagData`, `bag.UpdateBagData`, `fashion.updateData`,
  `equip.UpdateEquipBagData`, `building.UpdateBuildingInfo`,
  `tactic.GetHerosTactic`, `player.GetUserList`, `player.CreateUser`,
  `user.UserLogin` bootstrap codecs, and 11-byte client wire codecs.
- `blueoath-server`: loopback TCP front door multiplexing HTTP bootstrap, framed `login`, `state`,
  `set_formation`, `enter_stage`, and `battle_result` JSON routes, plus protobuf game-login frames with `player.Login`,
  `player.GetUserList`, `player.CreateUser`, `user.UserLogin`, `user.GetUserInfo`, and account-backed
  bag/fashion/equip/hero/building/fleet/construction/bath/task/shop/recharge/buildship initialization
  pushes and request refreshes. `user.UserLogin` sends the C#-ordered
  minimum bootstrap sequence (`user.UpdateUserInfo`, `guide.GuideInfo`, four `copy.GetCopy` snapshots,
  `dailycopy.UpdateDailyCopyData`) before its response. `config_chapter.db` is loaded dynamically;
  built-in IDs are used only when client config is unavailable. When
  `--client-path` is provided, `config_fashion.db` is read with C#'s XOR-0x55 format and merged
  into the fashion push; `config_shop.db` and `config_shop_goods.db` provide valid shop/shelf IDs
  for `shop.UpdateShopInfo`; server-local `gm-goods.json` drives `shop.BuyGoods` and
  `shop.QualityBuyGoods` rewards, while `config_shop_goods.db` supplies purchase costs;
  handbook behaviour and story tables populate illustration
  bootstrap fields; server-local `gm-mails.json` drives repeatable `mail.GetMailList` and
  `mail.FetchItem`/`mail.FetchAllItems` rewards. Mutations use a serialized candidate-state → SQLite-save → in-memory-commit
  path.
- `blueoath-domain`: typed profile/hero/equipment/fleet IDs, non-negative resource ledger,
  domain errors, and database-independent `AccountRepository` transaction contract.
- Building assignments validate ownership, duplicates, construction status, and
  `config_buildinginfo.db` capacity. `hero.RetireHero` removes dependent fleet/bath/equipment
  state, emits equipment tombstones, and applies `config_ship_main.db` breakdown rewards.
- `blueoath-storage`: SQLite persistence with startup migrations under `rust-server/migrations/`.
  New databases create normalized character/hero/equipment/fleet/inventory/battle/task tables,
  foreign keys, non-negative checks, indexes, WAL, busy timeout, and account revision CAS.
  Existing JSON snapshot methods remain only as a transition adapter while handlers migrate by domain.

Run checks:

```powershell
cargo test --manifest-path .\rust-server\Cargo.toml
cargo clippy --manifest-path .\rust-server\Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path .\rust-server\Cargo.toml --all -- --check
```

Run server:

```powershell
cargo run --manifest-path .\rust-server\Cargo.toml -p blueoath-server -- --port=0 --profile-id=local-player
# Optional: set `--game-login-port=<port>`; otherwise Rust binds a free local port.
# Set `--kcp-game-login-port=<port>` to expose the same login protocol over KCP/UDP.
# If `catalog/config` and `catalog/data` exist beside the binary (or in the
# repository's `rust-server/catalog`), they are loaded automatically. Explicit
# `--client-path`/`--data` override these bundled paths.
```

启动参数可放在仓库根目录 `server.json`，命令行参数优先覆盖文件值：

```json
{
  "port": 7080,
  "gameLoginPort": 7201,
  "dropMultiplier": 1.0,
  "shipExpMultiplier": 1.0,
  "commanderExpMultiplier": 1.0,
  "shipStatMultiplier": 1.0,
  "moodRecoveryMultiplier": 1.0,
  "affectionMultiplier": 1.0,
  "buildingOilMultiplier": 1.0,
  "buildingGoldMultiplier": 1.0
}
```

`shipStatMultiplier` 作用于服务端舰船战斗属性：耐久、炮击、装甲、雷装、雷防、舰载机轰炸/雷击、命中、闪避、暴击、抗暴，并包含等级成长和舰船强化属性（intensify）。倍率只作用于新产生的奖励和战斗属性；账号存档不会被倍率或初始模板覆盖。新 `profile-id` 首次登录时创建初始账号。

`moodRecoveryMultiplier` 作用于自然恢复和浴室恢复；`affectionMultiplier` 作用于出击、扫荡的正向好感增加。倍率不放大战斗心情损失、沉船好感扣减；客户端心情规则使用固定点数：上限 1500000、自然恢复上限 1190000、每 6 分钟自然恢复 100，契约舰额外恢复 100，浴室恢复最多 300000。

`buildingOilMultiplier` 和 `buildingGoldMultiplier` 作用于基地油、金币的新增产出；已存产出不重复放大。命令行参数分别为 `--building-oil-multiplier`、`--building-gold-multiplier`。

出击、扫荡共用掉落规则：`dropMultiplier: 10` 表示每个关卡掉落池独立抽取 10 次；2.5 表示抽 2 次，再以 50% 概率多抽一次；0 禁用随机掉落。首通奖励独立发放，不重复乘倍率。扫荡每次均发放关卡配置的指挥官、参战舰船经验，分别应用两种经验倍率；背包外舰船不获得经验。

`catalog/battle-drop-quantities.json` 是服务端自定义数量平衡表，启动读取，可单独编辑后重启服务端。它补充原配置中数量为 1 的常用材料（10182、10185、60000）和金币（5:1）；原配置明确给出的较大数量、舰船和装备实例数量保持原配置。`defaultRewards` 覆盖其他关卡，`copies` 覆盖指定关卡 ID。键为 `物品类型:配置ID`，值为 `[最小数量,最大数量]`，每次抽取独立随机数量。这是单机端平衡配置，不代表原服掉率。

| 海域章节 | 每次抽中的常用材料数量 | 每次抽中的金币数量 |
| --- | --- | --- |
| 1–3 | 2–4 | 第1章80–120，每章增加40 |
| 4–6 | 3–5 | 第4章200–240，每章增加40 |
| 7–9 | 4–6 | 第7章320–360，每章增加40 |
| 10–12 | 5–7 | 第10章440–480，每章增加40 |

出击开始扣一次石油，扫荡开始按次数扣足石油：`(supply_basic_cost + ceil(参战舰船 supple_cost 合计 × supple_cost_argu / 10000)) × 次数`。费用读取服务端配置，不足时拒绝开始，不发放奖励或经验；结算、刷新不再次扣油。原配置费用为 0 的关卡仍免费。

One-click Windows launcher (starts Rust server, waits for port 7080, then injects client):

```powershell
E:\Rust\BlueRebirth\start-rust-game.bat
```

For the Japanese client at `C:\Users\zhanl\Desktop\日服\blueoath`, use fixed local ports:

```powershell
cargo run --manifest-path .\rust-server\Cargo.toml -p blueoath-server -- `
  --port=7080 --game-login-port=7201 --profile-id=local-player `
  --client-path='C:\Users\zhanl\Desktop\日服\blueoath'
```

Create server-local JSON catalog snapshot (removes runtime dependency on game install):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\export-rust-catalog.ps1 `
  -ClientRoot 'E:\BlueOath Rebirth\blueoath'
cargo run --manifest-path .\rust-server\Cargo.toml -p blueoath-server -- --port=7080 --game-login-port=7201 --profile-id=local-player
# bundled catalog is auto-detected; flags remain valid for custom locations
```

The exporter converts all 100 configuration tables currently referenced by Rust loaders
from XOR/SQLite to `config_*.json`, plus server-owned runtime JSON files. The loader
prefers JSON and accepts legacy DB files as fallback. Use `-ConfigFormat db` only for a
legacy DB snapshot, or `-ConfigFormat both` during migration. Keep generated catalog
files with deployment; re-export when client configuration changes. JSON export also
runs the field audit/pruner: typed tables keep only fields read by Rust, while raw
gameplay/forward-compatible tables stay intact. Run it manually after editing JSON:

```powershell
python .\tools\prune-rust-catalog-json.py .\rust-server\catalog\config
python .\tools\prune-rust-catalog-json.py --apply .\rust-server\catalog\config
```

Inject an installed client without copying it into this repository:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\inject-game.ps1 `
  -ClientRoot 'E:\BlueOath Rebirth\blueoath' `
  -NativeRoot 'E:\BlueOath Rebirth\native' `
  -GameHash 8AEE607813A759E047D81C2428990609322DE072437DD4597F80E8E3FAD1D404 `
  -Redirect -Port 10173 -HttpPort 7080 -AllowUntrusted
```

`BlueOath.Injector.exe` and `BlueOath.Payload.dll` remain unchanged; only bootstrap paths and
server ports are supplied by the script.

The Rust server owns runtime behaviour and catalog data. Keep protocol evidence and client
reverse-engineering tools under `docs/` and `src/BlueOath.Tools`; do not reintroduce a second
server implementation. KCP/UDP login is available behind `--kcp-game-login-port`.
