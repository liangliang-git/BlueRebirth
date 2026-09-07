# 日服客户端接口对齐审计

审计对象：`C:\Users\zhanl\Desktop\BlueOath Rebirth` 日服运行目录及其 `客户端补丁/server/catalog` 配置；实现对象：`rust-server`。

## 结论

- Rust 服务端是唯一运行时实现；桌面目录没有可维护的服务端源码。
- 旧 C# 路由表共有 423 个业务路由；协议目录还记录 73 个候选消息类型。两者不是同一统计口径，不能把候选消息数当成真实接口数。
- 对桌面 JP 客户端 `GameAssembly.dll`/`global-metadata.dat` 实测静态扫描：97 个消息候选、1351 个符号、4 个 SDK 事件、0 个已捕获 HTTP 接口；协议扫描工具支持 `--only-jp` 与外部客户端目录参数。
- Rust 已覆盖 JP 客户端提取的 382 条请求路由；382 条全部进入 dispatcher（381 条服务端显式字面量 + 1 条 `matchsvr_` 动态模板），未解析 0 条。
- 本轮补齐关键旧接口：结婚、好感礼物、修理、头像购买/解锁、许愿舰娘、教学任务/教学点奖励、普通/自选宝箱、缓存数据、副本星级奖励箱、小游戏结算、舰船共鸣绑定/解绑/升级/突破。
- 本轮继续补齐日服实际调用：`user.SetMiniGameScore`、`user.GetMiniGameScore`、`user.GetMiniGameScoreRank` 持久化分数并返回榜单；`user.BuyGold`、`user.BuySupply`、`user.BuyPvePt` 扣钻石、加资源、更新每日购买计数；`user.TeacherRank` 返回分页排序的 `TTeachingRankRet.UserInfo`，`friend.UpdateUserState` 返回 `TUserStateRet(Type, Uid)`。
- 本轮新增聊天、远征、Boss、公会宝箱、战令、兑换、料理、世界事件、公会任务/悬赏/战争、杂志、互动道具、体育会 protobuf 响应与账户持久化；运动会奖励、抽取、剪纸、周年视频、情人节礼物、英雄觉醒、船任务、军团战奖励读取日服配置并保持幂等。
- 已用桌面客户端真实链路验证：`player.Login`、`player.GetUserList`、`user.UserLogin`、`user.GetUserInfo`、`user.Refresh` 均返回成功；客户端已进入主场景。

## 本轮实现

- `config_affection_item.db`：读取礼物好感值。
- `config_teaching_achievement.db`：读取教学点奖励映射。
- `config_item_info.db`、`config_item_selected.db`：读取普通/自选宝箱与嵌套掉落池。
- `config_ship_main.db.fixed_money`：修理费用按缺失 HP 比例计算。
- `config_chapter.db`：读取章节星级门槛/奖励；`copy.StarReward` 按通关星级发奖并记录幂等领取状态。
- JP 协议新增 Hero 合成相关字段证据：实际活动链路使用 `TCombineArg`（`MainHero/DeputyHero`）与 `TCombineUpArg`（`HeroId`）；`THeroComposeArg/Ret` 等短名类型仅作为静态候选收录。`THeroGrid.CombinationInfo` field 27 已同步 Rust/C# 编码器。
- `config_combination_ship.db` 与 `config_ship_fleet.db`：读取共鸣等级/突破成本、阶段星级、开放舰船；升级支持单级与快速升级，突破消耗真实材料。
- 所有资源扣除先校验，事务失败不修改账号；成功后刷新船坞、背包、装备、图鉴、用户信息。
- 料理与剪纸材料按精确多重集合匹配，互动道具兼容直接奖励与 `drop_id` 掉落池；公会积分宝箱、情人节礼物领取读取配置并推送资源刷新。
- 运动会奖励按日服 `TotalPoints`/`ReceivedList` 领取；`ReceiveAllPointsReward` 扫描全部已达成分数，单项领取不再伪增总分，奖励领取保持幂等。
- 兑换接口执行 `change_count` 次数上限；料理、兑换在奖励/材料配置无效时先失败，避免扣除资源后无法返还。
- 世界事件按 `config_activity` 的当前活动 `p1[1]` 选择 `config_world_event`，领取前校验个人进度与阶段，领取状态按活动隔离；公会随机奖励响应按 JP `TRANDOMREWARDDATA` 的稀疏字段 `3/5/7/9` 编码。
- 日服兑换码奖励按 `config_activity.p4[index]` 解析 `config_rewards`，领取扣减对应回执、按请求数量发奖并刷新用户/背包；配置或回执不足时拒绝请求。
- `config_outpost_info.db`、`config_outpost_level.db`：据点等级按 `outpost_id/level/next_id` 校验，升级先校验并扣除 `item_cost`；据点 `ItemInfo`/`TacticList` 回填协议，领取按 `TOPReceiveRet.ItemInfo` field 6 返回并刷新用户/背包；据点战术删除/改名路由已持久化。
- `config_parameter.db` 与 `config_interaction_figurte.db`：圣诞活动兑换按日服参数执行，金币 `5000` 换 1 个盲盒币 `17007`，重复公仔 `17008` 按 `10:1` 换币，真实扣除并返回 `TCOMMONARRREWARD`；盲盒抽取按持有舰船/皮肤过滤可抽公仔，维护 `BuyInfo/ToyInfo` 与 `ToyId` 返回；重复公仔不增加持有数，返 1 个 `17008`，并同步用户/背包。
- `config_activity.db`、`config_drop_item.db`：日服新年福袋活动 `id=77/type=41` 按 `p14=[[5,2,30]]` 扣钻石；购买依据持有 `2032014` 时装选择 `3000501/3000502` 掉落池，100 抽使用 `3000505` 保底；30/60 抽里程碑按 `3000503/3000504` 发奖，奖励领取幂等并同步用户、背包、时装。
- JP `battle_pb/match_pb`：`battle.CreateRoom/JoinRoom/LeaveRoom/MatchJoin/MatchLeave/SendAutoMsg/CreateMutiBattle/createBattleInfo` 返回对应房间、匹配、自动聊天、多人战斗消息；`TBattleMatchRet -> TMatchRet -> UidList` 嵌套响应与同类型跨账号匹配队列已接入；Battle 房间注册表维护跨账号成员、权限与自动消息补发，并从账号快照恢复；TCP/KCP 在线连接均订阅共享事件，`matchsvr_<zone>` 房间操作同步本地房间状态与 `match.UpdateRoomInfo`；`CreateMutiBattle/createBattleInfo` 使用实际绑定的前门端口，支持 `--port=0` 与非 7080 部署。
- SQLite `accounts` 目录新增稳定排序读取；好友、教学等请求的目标用户详情在运行时按真实账号 UID 查询，未找到目标时才回退当前账号兼容数据；启动时恢复持久化 PVE 房间注册表。
- 修复 Clippy 质量门：复杂 battle payload 改为结构参数，battle 详情改类型别名。

## 空路由清零

JP 客户端 80 个 service 文件共提取 382 条去重请求路由。已补齐有明确协议/状态语义的空实现：`dailycopy.CopyEnter`、`tower.Receive/Replacement/SendUpgrade/ReceiveBuff`、`activityTower.Reset/ReceiveBuff/QuickPass`、`illustrate.IllustrateNew/EquipNew/VowDecTime`、`fashion.fashionReplaceReward`、`copy.DotBase`、`guild.CancelApply`、`user.TeacherRank`、`friend.UpdateUserState`、好友组、教学组、远征据点、战令、兑换、料理、杂志、互动道具，以及用户订单、队列、徽章补偿、小游戏/资源购买接口。

这些接口中，塔、日常副本、图鉴、好友、教学、据点、小游戏、资源购买、战令、兑换、料理、世界事件、公会任务/悬赏/战争、杂志、互动道具、体育会会写入账户状态；无返回体的协议仍保留空 body，但不再是无路由入口。聊天、远征、Boss、公会宝箱、核心限时活动、公会扩展已进入独立 handler；社交详情开始从 SQLite 多账号目录按 UID 读取，PVE/Battle 房间通过进程共享注册表同步，并从账号快照恢复，房间事件通过 TCP/KCP 在线事件总线发送，断线时按 UID pending 补发；Boss、大活动、Guild 大活动榜已做多账号聚合。剩余差异集中在部分限时榜字段、PVP 实战战斗帧传输。注意：据点被动产出规则只有客户端展示配置，服务端未臆造随机/计时产出；已有 `ItemInfo` 才可领取/加速，避免凭空发奖。

## JP 动态路由全量对比

当前 Rust dispatcher 与桌面 JP Lua 路由逐项比对：382 条全部进入精确/前缀 handler；其中 `matchsvr_<zone>.*` 已通过动态 zone 规范化接入 co-op handler，不再作为未覆盖差集。脚本报告 381 条客户端路由有服务端显式字面量、1 条为 `matchsvr_` 动态模板、未解析 0 条；服务端源代码总字面量为 634 条（含 push/回调方法）。

JP `ProtobufTypeManager` 共提取 319 条返回类型映射，382 条请求中 189 条直接命中类型表。回调扫描确认其中 160 条请求确实执行 `PbToLua`，且类型表缺口为 0；其余路由主要是写操作、错误/状态回调或动态服务调用，不等价于缺少接口。`user.TeacherRank` 已按 `TTeachingRankRet.UserInfo` 返回分页教师榜；`friend.UpdateUserState` 已按 `TUserStateRet` 返回状态事件。脚本会在真实 `PbToLua` 路由缺少类型表时以非零退出。

可重复审计命令：`pwsh -File tools/compare-jp-routes.ps1 -ClientRoot "C:\Users\zhanl\Desktop\BlueOath Rebirth" -ServerSourceRoot "E:\Rust\BlueRebirth\rust-server" -ProtobufTypeManagerPath "C:\Users\zhanl\Desktop\BlueOath Rebirth\runtime\lua-normalized\net\protobuftypemanager.lua"`。脚本退出码非零表示存在未覆盖路由。

入口覆盖已完成；字段/状态完整度仍分两层：核心系统与新增聊天、远征、Boss、公会宝箱、抽取/SSR/生日/兑换码/剪纸/秘密副本/情人节/视频/圣诞活动走独立 protobuf handler；公会任务/悬赏/战争、排行榜、体育会、世界事件、船任务、邀请积分、充值扩展已进入独立域或显式兼容入口；Boss 用户榜、大活动榜、Guild 大活动榜已按 SQLite 多账号聚合排序，TCP/KCP 实时推送已接入，匹配控制面已返回真实嵌套用户列表。剩余差异集中在部分限时榜字段、PVP 实战战斗帧传输和少数活动缺失配置奖励，不是空路由。

`copy.FetchRewardBox` 复用已验证的 `TStarRewardArg/Ret` 星级奖励领取链；`illustrate.AddBehaviour` 已按旧 C# 的 `HandbookBehaviourLoader.AllBehaviourIds` 规则持久化；`illustrate.ModiVowHeroList` 额外持久化许愿名单，并推送对应图鉴。
