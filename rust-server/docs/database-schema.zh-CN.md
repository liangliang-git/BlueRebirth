# BlueRebirth 服务端数据库中文说明

本文档对应 `客户端补丁/server/saves/profiles.db` 当前 SQLite 结构。结构来源为实际数据库及 `rust-server/migrations` 迁移文件。

## 通用约定

- `profile_id`：存档/账号标识，通常为 `local-player`，关联 `profiles.id`。
- `hero_id`、`equip_id`：账号内实例 ID；同一模板可有多个实例。
- `template_id`、`copy_id`、`chapter_id` 等：配置表 ID，不是数据库自增 ID。
- `position`、`slot`、`record_index`：列表顺序或槽位编号，通常参与复合主键。
- `*_time`、`*_at`：除 `updated_utc`、`created_utc` 等 ISO 8601 文本外，通常为 Unix 秒时间戳。
- SQLite 使用 `INTEGER` 保存整数与布尔值；布尔值通常为 `0=否`、`1=是`。
- 多数账号数据使用 `profile_id` 级联删除；删除 `profiles` 记录会删除关联存档。
- 服务端保存账号时会重写标准化业务表。运行中直接修改数据库可能被内存状态覆盖，应先停止服务端。

## 账号与版本

### `schema_meta` - 数据库结构版本

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `id` | INTEGER | 固定元数据行 ID，主键 |
| `version` | INTEGER | 已应用迁移版本号 |
| `applied_at` | TEXT | 最近迁移应用时间，ISO 8601 UTC |

### `profiles` - 存档账号目录

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `id` | TEXT | 存档 ID，主键 |
| `name` | TEXT | 存档显示名称 |
| `updated_utc` | TEXT | 最近更新时间，ISO 8601 UTC |

### `account_revisions` - 账号并发版本

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，主键，关联 `profiles.id` |
| `revision` | INTEGER | 乐观并发修订号；每次成功保存递增 |
| `updated_utc` | TEXT | 修订更新时间，ISO 8601 UTC |

### `characters` - 指挥官与账号资源

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，主键 |
| `uid` | INTEGER | 游戏用户 UID |
| `name` | TEXT | 指挥官名称 |
| `level` | INTEGER | 指挥官等级 |
| `exp` | INTEGER | 指挥官当前经验 |
| `secretary_id` | INTEGER | 当前秘书舰实例 ID，对应 `hero_runtime.hero_id` |
| `gold` | INTEGER | 金币数量 |
| `diamond` | INTEGER | 钻石数量 |
| `supply` | INTEGER | 燃料数量 |
| `pve_pt` | INTEGER | PVE 点数/行动点 |
| `head` | INTEGER | 头像配置 ID |
| `head_frame` | INTEGER | 头像框配置 ID |
| `class_id` | INTEGER | 指挥官职业/类型 ID |
| `create_time` | INTEGER | 账号创建时间，Unix 秒 |
| `message` | TEXT | 个性签名/留言 |

## 舰船、装备与背包

### `hero_runtime` - 舰船统一运行时数据

舰船实例、等级、经验、状态、服务器计算属性、装备槽、技能等级全部按舰船实例保存。该表已合并原 `heroes`；静态模板属性仍通过 `template_id` 关联全局 `ship_template`。

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `hero_id` | INTEGER | 舰船实例 ID，复合主键 |
| `template_id` | INTEGER | 全局舰船模板 ID |
| `fashioning` | INTEGER | 当前装备时装 ID |
| `name` | TEXT | 舰船自定义名称 |
| `change_name_time` | INTEGER | 最近改名时间，Unix 秒 |
| `level` / `exp` | INTEGER | 舰船等级 / 当前经验 |
| `mood` / `affection` | INTEGER | 心情值 / 好感值 |
| `current_hp` | INTEGER | 当前生命值 |
| `lock_state` | INTEGER | 锁定状态，`0=未锁定`、`1=锁定` |
| `created_utc` | TEXT | 获得时间，ISO 8601 UTC |
| `max_level` | INTEGER | 舰船最大等级 |
| `intensify_level` | INTEGER | 强化等级汇总 |
| `breakthrough_level` | INTEGER | 突破等级 |
| `remould_level` | INTEGER | 改造等级 |
| `resonance_level` | INTEGER | 共鸣等级 |
| `computed_max_hp` | INTEGER | 服务端计算最大生命值 |
| `computed_scout_num` | INTEGER | 服务端计算索敌/侦察值 |
| `computed_attack` / `computed_defense` | INTEGER | 服务端计算攻击 / 防御 |
| `computed_torpedo_attack` / `computed_torpedo_defense` | INTEGER | 服务端计算雷击 / 雷击防御 |
| `computed_to_air_attack` / `computed_to_torpedo_attack` | INTEGER | 服务端计算对空 / 对鱼雷攻击 |
| `computed_ship_bomb_attack` / `computed_ship_torpedo_attack` | INTEGER | 服务端计算舰爆 / 舰攻 |
| `computed_ship_air_control` | INTEGER | 服务端计算制空值 |
| `computed_crit` / `computed_anti_crit` | INTEGER | 服务端计算暴击 / 抗暴 |
| `computed_hit` / `computed_dodge` | INTEGER | 服务端计算命中 / 闪避 |
| `computed_attributes_json` | TEXT | 其他服务端计算属性 JSON |
| `equip_slot_1` ~ `equip_slot_6` | INTEGER | 六个装备槽中的装备实例 ID，可为空 |
| `skill_levels_json` | TEXT | 技能 ID 到技能等级的 JSON 映射；不再单独持久化技能槽字段 |
| `updated_utc` | TEXT | 最近更新时间，ISO 8601 UTC |

### `equipments` - 装备实例

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `equip_id` | INTEGER | 装备实例 ID，复合主键 |
| `template_id` | INTEGER | 装备模板 ID |
| `enhance_level` | INTEGER | 强化等级 |
| `star` | INTEGER | 装备星级 |
| `enhance_exp` | INTEGER | 强化经验/强化进度 |
| `hero_id` | INTEGER | 装备所属舰船实例 ID；未装备时为空 |

### `inventory` - 道具背包

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `template_id` | INTEGER | 道具模板 ID，复合主键 |
| `amount` | INTEGER | 道具数量 |

### `fashion_entries` - 已拥有时装

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `sf_id` | INTEGER | 时装所属基础舰船 ID，复合主键 |
| `fashion_tid` | INTEGER | 已拥有时装模板 ID，复合主键 |

## 舰队与预设舰队

### `fleets` - 舰队主体

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `fleet_id` | INTEGER | 舰队 ID，复合主键 |
| `formation_id` | INTEGER | 阵型配置 ID |
| `tactic_id` | INTEGER | 当前战术配置 ID |
| `tactic_name` | TEXT | 战术/舰队名称 |
| `tactic_type` | INTEGER | 战术类型 |

### `fleet_members` - 舰队成员

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `fleet_id` | INTEGER | 舰队 ID，复合主键 |
| `member_kind` | INTEGER | 成员类别，`0=普通`、`1=扩展`，参与复合主键 |
| `position` | INTEGER | 编队位置，复合主键 |
| `hero_id` | INTEGER | 舰船实例 ID |

### `preset_fleet_meta` - 预设舰队全局状态

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，主键 |
| `name_num` | INTEGER | 已使用/生成的预设名称序号 |
| `red_dot` | INTEGER | 预设舰队红点状态 |

### `preset_fleets` - 预设舰队主体

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `slot` | INTEGER | 预设槽位，复合主键 |
| `name` | TEXT | 预设名称 |
| `mode_id` | INTEGER | 预设适用模式 ID |
| `strategy_id` | INTEGER | 预设战略/策略 ID |

### `preset_fleet_members` - 预设舰队成员

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `slot` | INTEGER | 预设槽位，复合主键 |
| `position` | INTEGER | 成员位置，复合主键 |
| `hero_id` | INTEGER | 舰船实例 ID |
| `is_ex` | INTEGER | 是否扩展成员，`0=普通`、`1=扩展`，参与复合主键 |

### `supply_heroes` - 补给舰船列表

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `position` | INTEGER | 列表位置，复合主键 |
| `hero_id` | INTEGER | 补给中的舰船实例 ID |

## 战斗会话与关卡进度

### `battle_sessions` - 当前战斗会话

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，主键；每个账号最多一个活动会话 |
| `chapter_id` | INTEGER | 章节 ID |
| `copy_id` | INTEGER | 关卡 ID |
| `current_fleet` | INTEGER | 当前敌方舰队/战斗段索引 |
| `state` | TEXT | 会话状态文本 |
| `started_at` | INTEGER | 战斗开始时间，Unix 秒 |
| `expires_at` | INTEGER | 会话过期时间，Unix 秒 |
| `revision` | INTEGER | 战斗会话修订号 |
| `attack_count` | INTEGER | 已记录攻击次数 |

### `battle_session_heroes` - 当前战斗参战舰船

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `position` | INTEGER | 参战顺序，复合主键 |
| `hero_id` | INTEGER | 舰船实例 ID |

### `battle_session_fleets` - 当前战斗敌方舰队序列

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `position` | INTEGER | 战斗段位置，复合主键 |
| `fleet_id` | INTEGER | 敌方舰队/关卡舰队配置 ID |

### `copy_progress` - 普通关卡通关进度

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `copy_id` | INTEGER | 关卡 ID，复合主键 |
| `star_level` | INTEGER | 关卡星级位掩码/星级状态 |
| `first_passed` | INTEGER | 是否首次通关已记录，`0=否`、`1=是` |

### `copy_star_rewards` - 已领取章节星级奖励

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `chapter_id` | INTEGER | 章节 ID，复合主键 |
| `reward_index` | INTEGER | 星级奖励档位索引，复合主键 |

### `copy_records` - 关卡历史阵容记录

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `record_index` | INTEGER | 历史记录序号，复合主键 |
| `copy_id` | INTEGER | 关卡 ID |
| `pass_time` | INTEGER | 通关耗时 |
| `secret_id` | INTEGER | 隐藏条件/密令 ID |
| `strategy_id` | INTEGER | 使用策略 ID |
| `power` | INTEGER | 记录阵容战力 |
| `record_time` | INTEGER | 记录生成时间，Unix 秒 |

### `copy_record_heroes` - 历史记录舰船

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `record_index` | INTEGER | 历史记录序号，复合主键 |
| `position` | INTEGER | 阵容位置，复合主键 |
| `hero_id` | INTEGER | 舰船实例 ID |

### `copy_record_ex_buffs` - 历史记录额外增益

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `record_index` | INTEGER | 历史记录序号，复合主键 |
| `position` | INTEGER | 增益顺序，复合主键 |
| `buff_id` | INTEGER | 增益配置 ID |

### `daily_copy_progress` - 每日/挑战关卡进度

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `reset_day` | INTEGER | 最近日重置日期标识 |
| `chapter_id` | INTEGER | 每日关卡章节 ID，复合主键 |
| `group_id` | INTEGER | 当前关卡组 ID |
| `challenge_times` | INTEGER | 已挑战次数 |
| `success_times` | INTEGER | 已成功次数 |
| `select_ex` | INTEGER | 已选择的扩展难度/模式 |
| `extra_group` | INTEGER | 额外关卡组 ID |
| `ex_star` | INTEGER | 扩展难度累计星级 |

### `sea_difficulty` - 海域难度选择

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，主键 |
| `difficulty` | INTEGER | 当前海域难度等级 |

## 基地、建造与支援

### `buildings` - 基地建筑

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `building_id` | INTEGER | 建筑实例 ID，复合主键 |
| `level` | INTEGER | 建筑等级 |
| `land_index` | INTEGER | 建筑地块位置 |
| `template_id` | INTEGER | 建筑模板 ID |
| `production_status` | INTEGER | 生产状态 |
| `recipe_id` | INTEGER | 当前生产配方 ID |
| `item_count` | INTEGER | 投入/待处理物品数量 |
| `product_count` | INTEGER | 已累计产物数量 |
| `last_update_at` | INTEGER | 最近生产结算时间，Unix 秒 |
| `recipe_time` | INTEGER | 单次配方生产时间 |
| `productivity` | INTEGER | 当前生产力 |
| `produce_speed` | INTEGER | 当前生产速度 |

### `building_hero_assignments` - 建筑派驻舰船

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `building_id` | INTEGER | 建筑实例 ID，复合主键 |
| `position` | INTEGER | 派驻位置，复合主键 |
| `hero_id` | INTEGER | 派驻舰船实例 ID |

### `construction_jobs` - 舰船建造队列

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `job_id` | INTEGER | 建造任务实例 ID，复合主键 |
| `building_id` | INTEGER | 使用的建造设施/槽位 ID |
| `started_at` | INTEGER | 开始时间，Unix 秒 |
| `finish_at` | INTEGER | 完成时间，Unix 秒 |
| `state` | TEXT | 建造任务状态 |
| `duration_seconds` | INTEGER | 建造总时长，秒 |
| `project_gold` | INTEGER | 投入金币数量 |
| `project_steel` | INTEGER | 投入钢材数量 |
| `project_aluminium` | INTEGER | 投入铝材数量 |
| `completed` | INTEGER | 是否完成，`0=否`、`1=是` |

### `support_entries` - 后勤/支援任务

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `entry_id` | INTEGER | 支援任务实例 ID，复合主键 |
| `support_id` | INTEGER | 支援任务配置 ID |
| `start_time` | INTEGER | 开始时间，Unix 秒 |

### `support_entry_heroes` - 支援任务舰船

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `entry_id` | INTEGER | 支援任务实例 ID，复合主键 |
| `position` | INTEGER | 队伍位置，复合主键 |
| `hero_id` | INTEGER | 派遣舰船实例 ID |

## 任务与引导

### `tasks` - 任务进度

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `task_id` | INTEGER | 任务配置 ID，复合主键 |
| `task_type` | INTEGER | 任务类型 |
| `progress` | INTEGER | 当前任务进度 |
| `completed` | INTEGER | 是否达到完成条件，`0=否`、`1=是` |
| `reset_day` | INTEGER | 日/周任务最近重置日期标识 |

### `task_claims` - 已领取任务奖励

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `task_id` | INTEGER | 任务配置 ID，复合主键 |
| `claimed_at` | INTEGER | 奖励领取时间，Unix 秒 |

### `guide_settings` - 游戏引导状态

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `setting_key` | TEXT | 引导状态键，复合主键；如阶段进度或客户端偏好 |
| `setting_value` | TEXT | 引导状态值；通常为 Lua/JSON 风格序列化文本 |

### `guide_plot_rewards` - 已领取剧情引导奖励

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `plot_id` | INTEGER | 已领取奖励的剧情/引导 ID，复合主键 |

## 聊天、好友与公会

### `chat_state` - 聊天状态

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，主键 |
| `channel` | INTEGER | 当前聊天频道 |

### `chat_messages` - 聊天消息

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `message_id` | INTEGER | 消息 ID，复合主键 |
| `channel` | INTEGER | 消息频道 |
| `sender_uid` | INTEGER | 发送者 UID |
| `body` | TEXT | 消息正文 |
| `sent_at` | INTEGER | 发送时间，Unix 秒 |
| `receive_uid` | INTEGER | 接收者 UID；公共频道通常为 0 |
| `message_type` | INTEGER | 消息类型 |
| `voice` | TEXT | 语音资源/语音消息字段 |

### `chat_barrages` - 弹幕消息

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `barrage_id` | INTEGER | 弹幕所属内容/弹幕 ID，复合主键 |
| `offset_value` | INTEGER | 播放偏移位置，复合主键 |
| `content` | TEXT | 弹幕正文 |
| `uid` | INTEGER | 发送者 UID，参与复合主键 |
| `sent_at` | INTEGER | 发送时间，Unix 秒，参与复合主键 |

### `friend_relations` - 好友关系

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 当前存档 ID，复合主键 |
| `friend_profile_id` | TEXT | 对方存档 ID，复合主键 |
| `relation` | TEXT | 关系状态，如好友、申请、拉黑 |
| `created_at` | INTEGER | 关系创建时间，Unix 秒 |

### `guilds` - 公会主体

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 所属存档 ID，主键 |
| `guild_id` | INTEGER | 公会 ID |
| `name` | TEXT | 公会名称 |
| `emblem` | INTEGER | 公会徽章 ID |
| `frame` | INTEGER | 公会徽章边框 ID |
| `enounce` | TEXT | 公会宣言文本 |
| `notice` | TEXT | 公会公告文本 |
| `level` | INTEGER | 公会等级 |
| `exp` | INTEGER | 公会经验 |
| `member_num` | INTEGER | 成员数量 |
| `leader_id` | INTEGER | 会长 UID |
| `leader_name` | TEXT | 会长名称 |
| `limit_level` | INTEGER | 加入所需最低指挥官等级 |
| `power` | INTEGER | 公会总战力 |
| `honor` | INTEGER | 公会荣誉值 |
| `create_time` | INTEGER | 公会创建时间，Unix 秒 |
| `chat_room` | TEXT | 公会聊天室标识 |
| `my_post` | INTEGER | 当前账号在公会中的职位 |
| `join_time` | INTEGER | 当前账号入会时间，Unix 秒 |
| `apply_num` | INTEGER | 待处理申请数量 |

### `guild_members` - 公会成员

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 所属存档 ID，复合主键 |
| `uid` | INTEGER | 成员 UID，复合主键 |
| `name` | TEXT | 成员名称 |
| `post` | INTEGER | 公会职位 |
| `contribute` | INTEGER | 累计贡献 |
| `today_contribute` | INTEGER | 今日贡献 |
| `power` | INTEGER | 成员战力 |

### `guild_applications` - 公会加入申请

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 所属存档 ID，复合主键 |
| `uid` | INTEGER | 申请人 UID，复合主键 |
| `name` | TEXT | 申请人名称 |
| `applied_at` | INTEGER | 申请时间，Unix 秒 |
| `quality` | INTEGER | 申请人展示品质/秘书舰品质 |

### `guild_box_items` - 公会礼包/宝箱

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 所属存档 ID，复合主键 |
| `box_kind` | TEXT | 宝箱类别，如分享箱、任务箱，复合主键 |
| `box_id` | INTEGER | 宝箱实例 ID，复合主键 |
| `end_time` | INTEGER | 过期时间，Unix 秒 |
| `box_uid` | INTEGER | 宝箱提供者 UID |
| `is_picked` | INTEGER | 是否已领取，`0=否`、`1=是` |
| `recharge_id` | INTEGER | 关联充值/礼包配置 ID |
| `recharge_name` | TEXT | 关联充值/礼包名称 |

## 爬塔与活动

### `tower_progress` - 常规爬塔进度

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，主键 |
| `chapter_id` | INTEGER | 当前爬塔章节 ID |
| `floor` | INTEGER | 当前层数 |
| `reset_day` | INTEGER | 最近重置日期标识 |
| `area_index` | INTEGER | 当前区域索引 |
| `copy_index` | INTEGER | 当前关卡索引 |
| `topic_index` | INTEGER | 当前主题索引 |
| `daily_count` | INTEGER | 今日挑战次数 |
| `reset_time` | INTEGER | 下次/最近重置时间，Unix 秒 |
| `pass_last_chapter_id` | INTEGER | 最近通关章节 ID |
| `is_reset` | INTEGER | 是否处于重置状态 |
| `max_level` | INTEGER | 历史最高层级 |
| `max_area` | INTEGER | 历史最高区域 |
| `max_copy` | INTEGER | 历史最高关卡 |
| `daily_count_ex` | INTEGER | 扩展模式今日次数 |
| `is_new_level` | INTEGER | 是否出现新层级标记 |

### `tower_ids` - 常规爬塔 ID 列表

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `kind` | TEXT | ID 列表类别，复合主键 |
| `position` | INTEGER | 列表位置，复合主键 |
| `value` | INTEGER | 对应配置/实例 ID |

### `tower_rewards` - 爬塔待结算/已生成奖励

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `position` | INTEGER | 奖励位置，复合主键 |
| `reward_type` | INTEGER | 奖励类型 |
| `config_id` | INTEGER | 奖励物品/配置 ID |
| `amount` | INTEGER | 奖励数量 |
| `instance_id` | INTEGER | 奖励实例 ID；非实例奖励通常为 0 |

### `tower_sf_counts` - 爬塔舰种/舰船计数

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `sf_id` | INTEGER | 基础舰船/舰种 ID，复合主键 |
| `count` | INTEGER | 对应累计数量 |

### `activity_tower_progress` - 活动爬塔进度

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，主键 |
| `activity_id` | INTEGER | 活动 ID |
| `reset_time` | INTEGER | 重置时间，Unix 秒 |
| `small_reset_number` | INTEGER | 小重置次数 |
| `quick_number` | INTEGER | 快速挑战/扫荡次数 |
| `history_max` | INTEGER | 历史最高进度 |

### `activity_tower_ids` - 活动爬塔 ID 列表

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `kind` | TEXT | ID 列表类别，复合主键 |
| `position` | INTEGER | 列表位置，复合主键 |
| `value` | INTEGER | 对应配置/实例 ID |

### `activity_progress` - 通用活动状态

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `activity_id` | TEXT | 活动或兼容状态命名空间，复合主键 |
| `progress_kind` | TEXT | 状态项名称，复合主键 |
| `value` | INTEGER | 状态值/进度值 |
| `updated_at` | INTEGER | 最近更新时间，Unix 秒 |

`activity_progress` 还承载部分尚未拆成独立表的兼容状态，例如活动计数、货币映射、勋章、战术和客户端功能状态。具体语义由 `activity_id + progress_kind` 组合决定。

## 旧本地服务兼容表

以下三张表服务早期简化协议和本地接口，不等同于当前游戏登录协议的 `characters`、`hero_runtime`、`fleets`。

## 表合并检查结论

- `heroes` 已合并到 `hero_runtime`，后续只读写 `hero_runtime`。
- `hero_equip_slots` 与 `hero_runtime.equip_slot_1` ~ `equip_slot_6` 重复，已删除。
- `fleet_ex_members` 已合并到 `fleet_members`，使用 `member_kind` 区分普通/扩展编队。
- `sea_progress` 是旧版关卡进度表，已并入 `copy_progress` 后删除。
- `equipments`、`ship_template`、`characters` 不合并：分别是装备实例、全局静态模板、指挥官账号资源，生命周期和数量关系不同。
- `copy_*`、`battle_*`、`support_*`、`supply_*`、`building_*`、`preset_fleet_*` 是不同业务父对象下的关系/进度表，不能按舰船实例直接合并。
- `activity_progress`、任务、聊天、公会、时装、背包和旧 `local_*` 表的数据域不同，保留独立表。

### `local_runtime` - 旧本地账号状态

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，主键 |
| `level` | INTEGER | 本地接口玩家等级 |
| `fuel` | INTEGER | 本地接口燃料 |
| `coins` | INTEGER | 本地接口金币 |
| `completed_stages` | INTEGER | 本地接口累计完成关卡数 |

### `local_ships` - 旧本地舰船

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `ship_id` | INTEGER | 本地舰船 ID，复合主键 |
| `name` | TEXT | 舰船名称 |
| `level` | INTEGER | 舰船等级 |
| `power` | INTEGER | 舰船战力 |

### `local_formation` - 旧本地编队

| 字段 | 类型 | 中文说明 |
|---|---|---|
| `profile_id` | TEXT | 存档 ID，复合主键 |
| `position` | INTEGER | 编队位置，复合主键 |
| `ship_id` | INTEGER | `local_ships.ship_id` |

## 常用查询

### 查看指挥官资源

```sql
SELECT profile_id, name, level, exp, gold, diamond, supply, pve_pt
FROM characters;
```

### 查看舰船状态

```sql
SELECT hero_id, template_id, level, exp, mood, affection, current_hp, fashioning
FROM hero_runtime
WHERE profile_id = 'local-player'
ORDER BY hero_id;
```

### 查看已拥有时装

```sql
SELECT sf_id, fashion_tid
FROM fashion_entries
WHERE profile_id = 'local-player'
ORDER BY sf_id, fashion_tid;
```

### 查看关卡与章节星级奖励

```sql
SELECT copy_id, star_level, first_passed
FROM copy_progress
WHERE profile_id = 'local-player'
ORDER BY copy_id;

SELECT chapter_id, reward_index
FROM copy_star_rewards
WHERE profile_id = 'local-player'
ORDER BY chapter_id, reward_index;
```

### 查看当前战斗会话

```sql
SELECT *
FROM battle_sessions
WHERE profile_id = 'local-player';
```
