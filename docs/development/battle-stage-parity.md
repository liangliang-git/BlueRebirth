# 战斗关卡接口与配置对齐

审计对象：日服客户端 `C:\Users\zhanl\Desktop\BlueOath Rebirth`，Rust 服务端 `rust-server`。

## 结论

服务端需要保留所有可进入、战斗、结算、扫荡关卡的权威配置；不需要把 499 张配置表全部接入战斗运行逻辑。

必须服务端化的数据：

- `config_chapter.db`：关卡分类、章节进度、每日组、活动海域类型。
- `config_copy.db`：关卡到舰队、战斗参数、随机分支。
- `config_fleet.db`：舰队到敌方编队、附加舰队。
- `config_ship_enemy.db`：敌方属性、编队槽位、技能和血量。
- `config_copy_display.db`：消耗、首通奖励、掉落、搜索 3D、显示关联的战斗字段。
- `config_drop_info.db`、`config_drop_item.db`、`config_rewards.db`、活动奖励表：结算奖励与掉落。
- 舰船、装备、补给、等级、技能、随机因子表：用于战斗校验、结算和成长。
- 每日、塔、活动塔、Boss、协同战斗专属配置：用于对应入口的状态和奖励。

可继续由客户端持有的数据：纯 UI、场景、动画、音频、立绘、VCR 等不参与服务端校验的资源。

## 接口匹配矩阵

| 功能 | 客户端入口 | 服务端处理 | 权威数据链 |
|---|---|---|---|
| 主线 / 周回海域 / 大活动海域 | `copy.GetCopy`、`copy.StartBase`、`copy.AttackBase`、`copy.PassBase`、`copy.QuitBase` | `game_login` + `battle_handler` | chapter → copy → fleet → enemy → drop/reward |
| 海域记录与战术 | `copy.GetRecord`、`copy.DeleteRecord`、`copy.TacticOn` | `battle_handler` | 账号战斗记录、舰队快照 |
| 战斗扩展状态 | `copyextra.AddCopyRewardCount`、`copyextra.UpdateCopyExtraInfo` | `misc_handler` | 奖励次数、Galgame 分支状态 |
| 随机因子 | `copy.GetRandomFactors` | `battle_handler` | `config_random_factor_group/set.db` |
| 每日作战 | `dailycopy.GetData`、`dailycopy.SelectEx`、`dailycopy.CopyEnter`、`dailycopy.UpdateDailyCopyData` | `battle_handler` | daily chapter → `dailygroup_id` → daily copy → tactic → `StartBaseRet` |
| 扫荡 | `mopUp.GetMopUpData`、`mopUp.StartSweep`、`mopUp.CheckSweep`、`mopUp.StopSweep` | `battle_handler` | 已通关记录、补给、掉落、扫荡次数 |
| 塔 / 活动塔 | `tower.*`、`activityTower.*` + 通用 `copy.StartBase/PassBase` | `tower_handler` + `battle_handler` | tower chapter/topic → copy → fleet/enemy；塔状态、Buff、重置、奖励 |
| Boss | `boss.GetBossData`、伤害排行接口 | `boss_handler` | Boss 配置、伤害、排行、奖励 |
| 协同 / PVP | `battle.*`、`battle.pvpMatchReady`、`battle.pvpMatchReadyTimeout`、`copy.PvpStartBase` | `coop_handler` + `battle_handler` | 房间、匹配、队伍、战斗会话 |
| 小游戏 / 宝箱 | `copy.PassMiniGame`、`copy.FetchRewardBox`、`copy.DotBase` | `legacy_handler` | 小游戏、星级、宝箱奖励配置 |

## 关卡覆盖

当前服务端内置目录审计结果：

```text
copies=1158
plot=368
sea=178
mubar=70
daily=44
goods=1
tower=15
equip_test=3
fleets=3669
enemies=11057
daily_groups=44
search_3d=2194
```

JP 与服务端目录共 499 张 `config_*.db`：497 张 SHA256 完全一致。差异仅为 `config_mail.db`、`config_reward.db`、`config_ship_main.db`；战斗核心表均一致。

大活动海域章节类型 `32`、`69`、`71` 已归入海域目录。客户端 `config_fleet.copy_enemys` 中，20243–20246 编号段存在客户端别名缺行；服务端保留客户端线上的敌方 ID，并将其属性解析到同组 `15002024x` canonical 模板，避免回退到低血量默认敌人。

每日 `CopyEnter` 已按日服协议解码 `TacticId` 字段，并校验：章节存在、关卡属于该章节 `dailygroup_id`、战术编队可用、补给足够。成功后创建战斗会话，返回 `TDailyCopyEnterRet.StartBaseRet(field=1)`，每日快照通过 `dailycopy.UpdateDailyCopyData` push。服务端不接受跨每日组关卡。

多舰队周回/海域结算按 `TPassBaseArg.EnemyFleets(field=20)` 消费剩余敌方舰队；旧客户端字段 `FleetInfo(field=17)` 仍兼容。日服 Lua 占位 `PassBase` 不带舰队字段时，服务端按目录顺序推进一支舰队，最后一支才写入通关记录、奖励和结算状态。

大活动挑战海域 `202431–202464` 的首舰队保持 `config_copy.fleet_id` 原值，不能套用 `160010000` 位置锚点；客户端需用活动舰队自身的海图、出生点和方向配置。

塔战斗关卡来自 `config_tower_topic.copy_list`，战斗本体复用通用 `copy.*` 入口；塔专属接口只负责 Buff、重置、奖励、快速通关和状态同步。

## 验证

新增 `bundled_battle_catalog_stage_reference_audit`：遍历主线、海域、大活动、每日、物资、装备测试目录，确认每个关卡拥有舰队，每个舰队的敌方引用均可解析。

配置工具支持按字段列出 JP/CN 行，用于后续差异复核：

```text
dotnet src/BlueOath.Tools/bin/Debug/net8.0/BlueOath.Tools.dll --analyze-config --config-list=jp:config_fleet:copy_enemys --jp-config-root="C:\Users\zhanl\Desktop\BlueOath Rebirth\blueoath\blueoath_Data\StreamingAssets\config" --cn-config-root="rust-server\catalog\config"
```

原生 PVP 战斗帧传输仍需真实客户端抓包才能宣称字节级一致；普通关卡、每日、扫荡、塔、活动塔的接口与配置链已覆盖。
