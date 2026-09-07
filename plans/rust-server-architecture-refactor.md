# BlueRebirth 服务端整体改造计划

## 目标

将当前“原始 protobuf 字节 + `serde_json::Value` + 字符串路由 + 多个可变输出参数”结构，改造成边界清晰、类型安全、可测试、可扩展的 Rust 游戏服务端。

本计划不保留旧存档兼容层。改造期间允许删除旧账号数据并重新生成新账号默认状态；协议字段编号、字段类型和客户端可见行为仍必须保持与日服客户端一致。

## 当前基线

- Workspace 当前包含 `protocol`、`server`、`storage`、`transport` 四个 crate。
- [protocol/lib.rs](../rust-server/crates/protocol/src/lib.rs) 已有基础 protobuf Codec 和部分 DTO。
- [server/game_login.rs](../rust-server/crates/server/src/game_login.rs) 负责大量路由、上下文构造和响应拼装。
- 业务 Handler 普遍接收 `&[u8]`，直接读取数字字段，再返回 `Option<Vec<u8>>`。
- 账号状态大量使用 `serde_json::Value`，字段名、数组结构、默认值分散在各 Handler。
- `legacy_handler.rs` 仍承载大量跨领域接口，需在迁移完成后删除。

## 目标架构

```text
client
  │ protobuf / TCP frame
  ▼
transport
  ▼
protocol          只负责 wire codec、字段编号、协议 DTO
  ▼
router            Method enum、路由注册、请求生命周期
  ▼
feature handlers  requests / responses / service / rules
  ▼
domain            强类型账号状态、枚举、ID、资源、错误
  ▼
storage           repository、事务、账号快照持久化
  ▲
catalog           配置加载、索引、启动校验、版本快照
```

目标 Workspace：

```text
crates/
  protocol/       protobuf wire 层，不依赖业务规则
  transport/      TCP/KCP/frame 层
  domain/         账号状态、枚举、值对象、领域错误
  catalog/        配置模型、加载器、索引、校验
  storage/        ProfileRepository、事务、序列化
  game/           用例服务、奖励、资源、战斗、进度规则
  server/         路由、Handler、连接生命周期、推送
  testkit/        请求构造、快照工厂、golden packet、测试夹具
```

## 统一接口模型

### 请求

每个接口定义独立请求结构体；Handler 不再直接读取字段编号。

```rust
pub struct EnterBattleRequest {
    pub chapter_id: ChapterId,
    pub copy_id: CopyId,
    pub fleet_id: FleetId,
    pub tactic_id: TacticId,
}

impl Decode for EnterBattleRequest {
    fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        // 字段编号、wire type、范围、重复字段统一校验
    }
}
```

统一处理：

- 缺失字段与默认字段区分。
- varint、signed/unsigned、重复字段、嵌套消息统一 Codec。
- 数量上限、ID 正数、数组长度、重复 ID 统一校验。
- 非法请求返回 `GameError::InvalidRequest`，不在 Handler 内散落错误字符串。

### 响应

内部不再使用 `Option<Vec<u8>>` 表达全部结果：

```rust
pub enum HandlerResult {
    Reply(Response),
    PushOnly,
    Empty,
}

pub struct Response {
    pub method: GameMethod,
    pub payload: ResponsePayload,
}

pub enum ResponsePayload {
    User(UserInfoResponse),
    Battle(BattleResponse),
    Raw(Vec<u8>),
}
```

只有 protocol 边界将响应 DTO 编码成 `Vec<u8>`。

### 错误

```rust
pub enum GameError {
    InvalidRequest(&'static str),
    AccountUnavailable,
    NotFound(&'static str),
    InsufficientResource(ResourceKind),
    InvalidState(&'static str),
    CatalogUnavailable,
    Internal(String),
}
```

统一转换为客户端 `err`、`errMsg`，移除 `response_err` 与 `response_err_msg` 双可变参数。

### 路由

将字符串路由集中到 `GameMethod`：

```rust
pub enum GameMethod {
    UserGetInfo,
    BattleStart,
    BattleAttack,
    BattleEnd,
    DailyCopyGetData,
    Unknown(String),
}
```

使用静态路由表或模块注册表：

```rust
Router::new()
    .register(GameMethod::UserGetInfo, user::handle)
    .register(GameMethod::BattleStart, battle::handle);
```

删除跨文件 `starts_with` 路由和重复 `match`。

## 账号状态改造

### 强类型状态

按业务域拆分 `AccountState`：

```rust
pub struct AccountState {
    pub profile: ProfileState,
    pub character: CharacterState,
    pub dock: DockState,
    pub fleet: FleetState,
    pub battle: BattleProgressState,
    pub daily_copy: DailyCopyState,
    pub tasks: TaskState,
    pub buildings: BuildingState,
    pub social: SocialState,
    pub activities: ActivityState,
}
```

枚举/值对象：

- `ChapterId`、`CopyId`、`FleetId`、`HeroId`、`EquipId`、`TemplateId`。
- `CurrencyKind`、`TaskType`、`BattleMode`、`FormationId`、`RewardKind`。
- `ChinaDay`、`ResetWeek`、`UnixSeconds`。
- `ResourceAmount`，统一非负、溢出、上下限处理。

### 默认账号

新增 `NewAccountFactory`：

- 所有持久化功能状态集中定义。
- 当前中国区日/周由统一 `Clock` 计算。
- 默认资源、初始舰船、装备、建筑、任务状态集中维护。
- 不生成公会、战斗会话、活动进度等虚假状态。
- 默认值通过领域类型构造，再由 storage 序列化。

### 持久化

允许删除旧存档兼容代码：

- 新数据库 schema 直接对应 `AccountState`。
- storage 提供 `load_account`、`save_account`、`update_account`。
- 服务层以命令方式修改状态，不允许 Handler 任意写 JSON 字段。
- 保存采用单次事务，避免部分模块更新成功、部分失败。
- 每次保存前执行领域不变量校验。

## 文件与模块分类

当前大文件按以下规则拆分：

```text
server/src/features/
  user/
    mod.rs
    requests.rs
    responses.rs
    service.rs
    state.rs
    rules.rs
    tests.rs
  hero/
  fleet/
  battle/
  copy/
  daily_copy/
  building/
  construction/
  equip/
  shop/
  task/
  tower/
  social/
  activity/
```

职责边界：

- `requests.rs`：请求 DTO 和解码。
- `responses.rs`：响应 DTO 和编码调用。
- `service.rs`：用例编排、状态修改、推送效果。
- `state.rs`：本模块持久化状态。
- `rules.rs`：纯函数规则、计算、校验。
- `tests.rs`：模块单测和协议契约测试。
- `mod.rs`：公开接口和路由注册。

公共代码统一放置：

```text
server/src/common/
  request.rs       请求上下文、参数解码辅助
  response.rs      响应、Push、HandlerResult
  error.rs         GameError、错误映射
  ids.rs           所有 ID newtype
  resources.rs     资源扣除、增加、上限
  clock.rs         时间、日切、周切
  validation.rs    通用参数校验
  pagination.rs    分页/范围校验
  rng.rs           可测试随机数接口
```

## 公共方法治理

统一替换重复实现：

- `decode_varint_field`、重复消息解码 → typed `Decode` trait。
- `json_i32/json_i64/json_u64/json_bool` → 类型化状态访问。
- 资源扣除、奖励发放、Bag 更新 → `ResourceService`、`RewardService`。
- `append_method_push` → `ResponseEffects::push`。
- 日切/周切计算 → `Clock`、`ResetCalendar`。
- `ensure_*_state` → `AccountState::new_*` 或仓储初始化，不在 Handler 懒创建。
- 章节/节点/舰队查找 → Catalog 索引，不重复遍历 JSON 数组。
- 账号保存 → 一个统一事务出口。
- 错误设置 → `?` 传播 `GameError`，边界统一编码。

禁止模式：

- Handler 直接操作 `Value` 字符串键。
- Handler 直接构造 protobuf 字节。
- 同一字段在多个模块定义不同默认值。
- 用 `0` 同时表示缺失、无效、未开始、已完成。
- 用 `String` 代替固定枚举。
- 用 `Vec<i32>` 代替有语义的 ID 集合。
- 为绕过借用检查复制整份账号快照。

## 业务服务拆分

### User / Account

- 账号身份、角色资料、资源、头像、秘书、统计。
- 资源增减统一审计。

### Hero / Equip / Fleet

- 舰船成长、技能、心情、装备槽、编队、战术。
- 强制检查舰船归属、装备归属、重复装备、舰队人数。

### Battle / Copy / DailyCopy

- 战斗会话生命周期：创建、攻击、舰队结果、结算、清理。
- 普通海域、多舰队节点、每日副本、扫荡共用节点模型。
- 节点推进只由 `BattleProgressService` 修改。
- 明确区分“单舰队结束”和“整个节点结束”。

### Building / Construction

- 建筑、土地、生产、建造队列、完成、快速完成。
- 队列、资源消耗、时间计算统一服务。

### Task / Tower / Activity

- 任务事件统一事件模型。
- 塔、战令、活动只实现各自规则，奖励走公共服务。
- 活动状态按活动 ID 分区，避免根字段无限增长。

### Social

- 好友、聊天、公会分开。
- 新账号不默认加入公会。
- 聊天消息限制长度、频率和广播范围。

## Catalog 与配置

- 所有 JSON/DB 配置先解析为 typed catalog。
- 启动阶段校验引用完整性、ID 唯一性、范围、节点终点、奖励类型。
- Catalog 使用 `Arc` 只读共享，避免每请求重复加载/复制。
- 章节、战斗、任务、装备、商店配置建立 HashMap/BTreeMap 索引。
- 配置版本写入日志和诊断接口。
- 删除 Handler 内直接读取原始配置 JSON 的路径。

## 性能与并发

- 一次请求只解析一次请求参数。
- 一次请求只生成一次响应效果列表。
- 避免账号全量深拷贝和重复 JSON 扫描。
- 读操作使用不可变引用；写操作通过领域命令集中修改。
- 账号级更新采用细粒度锁或存储事务，减少全局锁。
- 目录数据只读共享；战斗随机数通过可注入 RNG 测试。
- 对重复 Get 请求增加短生命周期缓存时，先测量再引入。

## 安全与边界

- 所有客户端 ID、数量、数组长度、字符串长度都在协议边界校验。
- 资源、奖励、战斗结算禁止信任客户端结果。
- 战斗会话绑定账号、节点、舰队和状态机。
- 重复结算、重复领奖、重放请求必须幂等。
- 敏感 token、账号数据、完整请求包禁止写日志。
- 聊天、好友、抽卡、充值接口增加频率限制和审计。

## 测试体系

```text
protocol tests       Codec round-trip、wire golden packet、未知字段
domain tests         纯规则、状态机、资源、日切、奖励
service tests        用例、事务、幂等、推送效果
handler tests        method -> request -> response 契约
integration tests    TCP/KCP 登录、完整战斗、存档读写
regression tests     1-1 三舰队节点、所有章节节点、活动入口
property/fuzz tests  protobuf 解码、边界值、随机战斗结果
```

每个接口至少覆盖：成功、缺字段、非法 ID、越界数量、重复请求、账号状态错误。

## 分阶段执行

### Phase 0：冻结基线

- 保留当前已推送前源码快照。
- 记录全量测试、Clippy、格式化结果。
- 建立协议方法清单、字段编号清单、账号字段清单。
- 禁止新代码继续增加 `legacy_handler` 和裸 `Value` 写入。

### Phase 1：基础公共层

- 新增 `common::{error,response,ids,clock,validation}`。
- 新增 `GameMethod`、`HandlerResult`、`ResponseEffects`。
- 保持现有行为，先让旧 Handler 通过适配器运行。

### Phase 2：协议请求/响应 DTO

- 按 User、Hero、Fleet、Battle、Copy 顺序定义请求结构体。
- 把公共 protobuf Reader/Writer 抽到 protocol crate。
- 添加 golden packet，锁定日服字段编号。

### Phase 3：账号领域模型

- 先迁移 `character`、`dock`、`equip`、`fleet`。
- 再迁移 `seaProgress`、`dailyCopy`、`tasks`、`tower`。
- 最后迁移建筑、社交、活动状态。
- 删除旧存档兼容分支，重建数据库。

### Phase 4：核心服务

- `ResourceService`、`RewardService`、`ProgressService`、`BattleService`。
- 将普通海域、多舰队节点、周回、每日副本统一到节点状态机。
- 将资源、奖励、推送、保存纳入一个用例事务。

### Phase 5：路由与 Handler 迁移

- 逐模块从 `legacy_handler.rs` 迁出。
- 每迁移一个模块，删除对应旧分支和重复方法。
- 路由改静态注册，未知接口统一返回明确错误。

### Phase 6：Catalog 与启动校验

- 全量 typed catalog。
- 启动时验证所有章节节点、战斗舰队、奖励、任务引用。
- 失败配置直接阻止启动并输出定位信息。

### Phase 7：存储与并发

- 新 `AccountRepository` 和事务接口。
- 账号状态原子更新。
- 保存失败回滚，增加崩溃恢复测试。

### Phase 8：活动与社交

- 活动按模块和活动 ID 隔离。
- 好友、聊天、公会独立服务。
- 删除无状态伪实现，未支持接口统一能力声明。

### Phase 9：性能、安全、可观测性

- 请求耗时、错误率、存储耗时、战斗结算计数。
- 输入边界、重放、频率限制、安全日志审查。
- 基准测试：登录、战斗、GetData、存档保存。

### Phase 10：清理与发布

- 删除 `legacy_handler.rs`、旧 JSON accessor、兼容适配器。
- 收敛模块导出和依赖方向。
- 更新 README、开发文档、部署脚本、CI。
- release 构建、部署包验证、全量回归。

## 并行关系

可并行：

- 协议 DTO 与 Catalog typed model。
- Domain ID/enum 与测试夹具。
- User、Hero、Building、Social 模块迁移。

必须串行：

- 公共错误/响应层 → Handler 迁移。
- 账号领域模型 → Storage 重建。
- Battle 状态机 → Copy/DailyCopy 迁移。
- Catalog 校验 → 全量节点回归。
- 所有模块迁移 → 删除 legacy 代码。

## 完成标准

- Handler 不再直接读取账号 JSON 字符串键。
- 业务 Handler 不再返回裸 `Option<Vec<u8>>`。
- 错误不再通过多个可变参数传递。
- 所有核心请求拥有 typed request DTO。
- 所有核心响应拥有 typed response DTO。
- 路由不再依赖散落字符串前缀判断。
- 新账号状态由单一工厂生成。
- 所有章节节点、战斗舰队、奖励引用启动校验通过。
- `cargo test --workspace`、Clippy、fmt、release 构建全部通过。
- 1-1 等多舰队节点、战斗结算、重连、重复请求回归通过。
- Git 历史不包含 `target`、临时数据库、运行时存档和大体积构建产物。
