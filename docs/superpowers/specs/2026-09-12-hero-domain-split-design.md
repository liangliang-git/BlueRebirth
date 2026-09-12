# Hero Feature Domain Split Design

## Goal

将 `rust-server/crates/server/src/features/hero` 中按协议方法堆叠的英雄服务代码按领域职责拆分，保持现有协议行为、入口函数和状态语义不变，并为领域处理函数与关键辅助函数补充 Rust 文档注释。

## Scope and compatibility

- 只调整 `features/hero` 内部文件组织、模块可见性和函数注释。
- 保持 `service::handle_typed`、`compat_service::handles_typed`、`compat_service::handle_typed` 的调用签名和路由行为不变。
- 不改变协议方法名、请求解码方式、响应编码、状态键、错误类型和副作用顺序。
- 保留现有测试，并将测试移动到最接近被测领域模块的位置；测试语义不变。
- 不触碰工作区已有的其他修改。

## Proposed structure

```text
hero/
├── mod.rs
├── state.rs
├── service/
│   ├── mod.rs
│   ├── response.rs
│   ├── query.rs
│   ├── experience.rs
│   ├── growth.rs
│   ├── equipment.rs
│   └── fashion.rs
└── compat/
    ├── mod.rs
    ├── illustration.rs
    ├── relationship.rs
    ├── combination.rs
    └── treasure.rs
```

`service/mod.rs` 和 `compat/mod.rs` 是稳定的领域路由边界；具体处理模块只暴露给父模块使用的 `pub(super)` 或 `pub(crate)` 函数。公共状态操作和响应同步函数集中在 `state.rs` 与 `service/response.rs`，避免各领域重新实现相同的编码和推送逻辑。

## Domain responsibilities

### Typed hero service

- `query.rs`: `hero.GetHeroInfo`、批量查询、锁定和改名。
- `experience.rs`: `hero.AddExp` 以及等级变化后的经验、生命值和任务同步。
- `growth.rs`: `hero.StudySkill`、`hero.HeroIntensify`、`hero.HeroAdvance`、`hero.HeroAdvMaxLv`、`hero.HeroAdvanceMUB` 和 `hero.HeroRemould`，以及这些流程共用的资源、进度和英雄占用判断。
- `equipment.rs`: `hero.ChangeEquip`、`hero.AutoEquip` 和 `hero.AutoUnEquip`。
- `fashion.rs`: `hero.ChangeFashion`。
- `response.rs`: 完整或增量英雄袋、背包、装备袋、用户信息响应的构造与推送。
- `mod.rs`: 保持原有 `handle_typed` 签名，只负责方法匹配、上下文组装和委派。

### Compatibility service

- `illustration.rs`: `illustrate.*` 方法及图鉴 payload 构造。
- `relationship.rs`: `hero.Marry`、`hero.AddAffection`、`repair.RepairHero`。
- `combination.rs`: `hero.HeroCombine*` 方法、组合进度与资源消耗。
- `treasure.rs`: `bag.GetNormalTreasureInfo`、`bag.GetSelectTreasureInfo` 及奖励发放。
- `compat/mod.rs`: 保持兼容方法判定和 `handle_typed` 签名，处理低复杂度固定响应并委派领域模块。

## Dependency direction

```text
hero/mod.rs
  ├── state.rs
  ├── service/mod.rs ──> service/{response,query,experience,growth,equipment,fashion}
  └── compat/mod.rs ──> compat/{illustration,relationship,combination,treasure}
```

领域模块通过 `super`/`crate` 使用既有协议、目录和领域状态类型；不得反向依赖路由模块。兼容领域可以复用 `service/response.rs` 的响应同步能力，但 typed service 不依赖 compat service，避免形成循环依赖。若多个领域需要同一小段纯逻辑，则将其放入已有 `state.rs` 或明确命名的响应/资源辅助模块，而不是复制实现。

## Documentation requirements

- 每个模块增加模块级 `//!` 说明其领域职责。
- 每个跨模块调用的 `pub(crate)`/`pub(super)` 函数使用 `///` 说明协议入口、状态变化和返回结果。
- 每个非显而易见的私有辅助函数使用 `///` 说明其业务用途；纯粹一行的显然转换不强制扩写。
- 注释描述业务行为和协议语义，不重复逐行解释实现。
- 对资源扣除、状态键、响应推送顺序等易被误改的约束，在对应函数文档中明确说明。

## Testing strategy

- 先运行当前 hero 相关测试，记录基线。
- 拆分过程中先保留现有测试断言，再按领域移动测试模块，不改变测试数据和期望结果。
- 为新模块路由边界补充最小测试：typed 路由继续覆盖已有方法，compat 路由继续覆盖已有兼容方法。
- 对纯辅助函数优先使用现有单元测试；不为了追求文件覆盖率而引入行为变化。
- 完成后运行 `cargo fmt --check`、hero 所在 crate 的完整 `cargo test`，并在可行时运行 `cargo clippy --all-targets --all-features -- -D warnings`。

## Non-goals

- 不重命名协议方法或领域状态字段。
- 不把英雄与装备、建造、战斗等跨 feature 逻辑迁移到本次范围之外的模块。
- 不修复与本次拆分无关的业务问题。
