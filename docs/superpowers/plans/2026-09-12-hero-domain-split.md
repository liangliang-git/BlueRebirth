# Hero Domain Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 hero typed service 与兼容协议按业务领域拆分为聚焦模块，同时保持现有协议行为、入口签名和状态语义不变。

**Architecture:** 保留 `hero/mod.rs` 作为 feature 边界，新增 `service/` 与 `compat/` 两层路由模块。typed service 按查询、经验、成长、装备、时装和响应同步拆分；兼容 service 按图鉴、关系、组合和宝箱拆分。共享的状态访问和响应编码通过父模块明确暴露，领域模块只依赖父路由和已有 crate 类型，不互相反向依赖。

**Tech Stack:** Rust, Cargo workspace, `blueoath_domain`, `blueoath_protocol`, existing codec and `HandlerResult` infrastructure.

**Spec:** `docs/superpowers/specs/2026-09-12-hero-domain-split-design.md`

## Global Constraints

- 只调整 `features/hero` 内部文件组织、模块可见性和函数注释。
- 保持 `service::handle_typed`、`compat_service::handles_typed`、`compat_service::handle_typed` 的调用签名和路由行为不变。
- 不改变协议方法名、请求解码方式、响应编码、状态键、错误类型和副作用顺序。
- 保留现有测试，并将测试移动到最接近被测领域模块的位置；测试语义不变。
- 不触碰工作区已有的其他修改。
- 每个新增模块包含模块级 `//!` 文档；跨模块函数和非显而易见的业务辅助函数使用 `///` 文档注释。

## File map

- Modify: `rust-server/crates/server/src/features/hero/mod.rs` — 通过路径属性将 `service/mod.rs` 暴露为 `service`、将 `compat/mod.rs` 暴露为现有的 `compat_service`，保留 `state` 边界。
- Keep/modify: `rust-server/crates/server/src/features/hero/state.rs` — 保留 JSON 状态访问和 payload 基础编码；为现有跨领域函数补充文档注释。
- Create: `rust-server/crates/server/src/features/hero/service/mod.rs` — typed catalog 上下文、共享 typed 资源/进度辅助函数、稳定的 `handle_typed` 路由。
- Create: `rust-server/crates/server/src/features/hero/service/response.rs` — 英雄袋、背包、装备袋、用户信息的完整/增量同步响应。
- Create: `rust-server/crates/server/src/features/hero/service/query.rs` — 查询、锁定、改名协议处理。
- Create: `rust-server/crates/server/src/features/hero/service/experience.rs` — 加经验、等级变化、任务同步及经验响应编码。
- Create: `rust-server/crates/server/src/features/hero/service/growth.rs` — 技能升级、强化、突破、满级突破、MUB 突破和改造。
- Create: `rust-server/crates/server/src/features/hero/service/equipment.rs` — 换装、自动装备、自动卸装及装备关联维护。
- Create: `rust-server/crates/server/src/features/hero/service/fashion.rs` — 时装穿戴与归属校验。
- Create: `rust-server/crates/server/src/features/hero/compat/mod.rs` — 兼容方法判定、固定响应、兼容路由委派。
- Create: `rust-server/crates/server/src/features/hero/compat/illustration.rs` — 图鉴方法及图鉴 payload 编码。
- Create: `rust-server/crates/server/src/features/hero/compat/relationship.rs` — 婚姻、好感度和英雄修理。
- Create: `rust-server/crates/server/src/features/hero/compat/combination.rs` — 英雄组合、组合升级和组合突破。
- Create: `rust-server/crates/server/src/features/hero/compat/treasure.rs` — 普通/选择宝箱开启、抽取和奖励发放。
- Rename/remove after migration: `rust-server/crates/server/src/features/hero/service.rs` → `service/mod.rs` and `compat_service.rs` → `compat/mod.rs` — 旧文件先整体迁移以保留行为，再删除其中已经拆出的重复实现；逻辑模块名由 `hero/mod.rs` 的路径属性保持不变。

---

### Task 1: Establish a verified baseline and module skeleton

**Files:**
- Modify: `rust-server/crates/server/src/features/hero/mod.rs`
- Rename: `rust-server/crates/server/src/features/hero/service.rs` → `rust-server/crates/server/src/features/hero/service/mod.rs`; `compat_service.rs` → `rust-server/crates/server/src/features/hero/compat/mod.rs`
- Create: remaining files listed under `service/` and `compat/` as module declarations, then fill them during later tasks
- Test: existing hero unit tests in `service.rs` and `compat_service.rs`

**Interfaces:**
- Consumes: current `service::handle_typed` and `compat_service::{handles_typed, handle_typed}` call sites.
- Produces: module declarations that compile only after the old implementations are moved; no route behavior changes in this task.

- [ ] **Step 1: Record the current test baseline**

Run from `rust-server`:

```powershell
cargo test -p blueoath-server features::hero
```

Expected: the command reports the current hero-related test result; save the count and any pre-existing warnings for comparison.

- [ ] **Step 2: Confirm all external hero entry points**

Run from the repository root:

```powershell
rg -n "service::handle_typed|compat_service::(handles_typed|handle_typed)|features::hero" rust-server/crates/server/src rust-server/crates/server/tests
```

Expected: only the existing game-login/router integration points and in-module tests require the same function signatures after the move.

- [ ] **Step 3: Establish directory modules without changing the logical public paths**

Create the `service/` and `compat/` directories by moving the two monolithic files to their respective `mod.rs` paths. Update `hero/mod.rs` to use `#[path = "service/mod.rs"] pub(crate) mod service;` and `#[path = "compat/mod.rs"] pub(crate) mod compat_service;`. Add `//!` comments and child module declarations while leaving the old handler bodies in place temporarily; this preserves every existing logical import while the later tasks perform the extraction.

- [ ] **Step 4: Run formatting and compilation after the skeleton change**

Run:

```powershell
cargo fmt --all -- --check
cargo check -p blueoath-server
```

Expected: formatting and compilation remain clean; if empty modules are temporarily unused, resolve only the resulting module wiring errors without changing runtime behavior.

---

### Task 2: Extract typed shared context, response synchronization, query, fashion, and equipment domains

**Files:**
- Modify: `rust-server/crates/server/src/features/hero/service/mod.rs`
- Create: `rust-server/crates/server/src/features/hero/service/response.rs`
- Create: `rust-server/crates/server/src/features/hero/service/query.rs`
- Create: `rust-server/crates/server/src/features/hero/service/fashion.rs`
- Create: `rust-server/crates/server/src/features/hero/service/equipment.rs`
- Test: existing query, fashion, and equipment tests originally in `service.rs`

**Interfaces:**
- Consumes: `HeroTypedCatalogs`, `HandlerResult`, `ResponseEffects`, existing domain/protocol types, and the current response encoders.
- Produces: `query::handle`, `fashion::handle`, `equipment::handle`, `response::push_hero_changes`, `response::push_hero_delta_changes`, and the unchanged typed route entry point.

- [ ] **Step 1: Move response synchronization helpers without editing their behavior**

Move `push_hero_changes` and `push_hero_delta_changes` into `service/response.rs`. Keep their exact response method order: hero bag, bag, optional equipment bag, then optional user info. Keep the delta tombstone representation and `include_equip` behavior unchanged. Expose them only to sibling domain modules through `pub(super)`.

- [ ] **Step 2: Move query branches into `query.rs`**

Extract the `hero.GetHeroInfo`, `hero.GetHeroInfoByHeroIdArray`, `hero.LockHero`, and `hero.ChangeName` match arms from `handle_typed` into `query::handle`. Preserve request decoding, missing-hero errors, lock/name state updates, timestamps, and `hero.UpdateHeroBagData` response encoding.

- [ ] **Step 3: Move fashion handling into `fashion.rs`**

Move `handle_fashion_equip` into `fashion::handle`. Preserve catalog ownership validation, the default base fashion calculation, invalid request/state errors, and the call to `response::push_hero_changes(..., false)`.

- [ ] **Step 4: Move equipment handling into `equipment.rs`**

Move `apply_typed_hero_equip` and the `hero.ChangeEquip`, `hero.AutoEquip`, and `hero.AutoUnEquip` branches into `equipment::handle`. Preserve slot normalization, previous equipment detachment, ownership checks, automatic selection order, and the existing response push order.

- [ ] **Step 5: Route the extracted domains through `service/mod.rs`**

Keep `handle_typed` as the only typed public entry point. Make each extracted method arm delegate to exactly one domain handler, while passing the same catalog options, server state, request bytes, and `ResponseEffects` reference as before. Remove moved duplicate code.

- [ ] **Step 6: Run the focused test cycle**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p blueoath-server features::hero -- --nocapture
```

Expected: all baseline hero tests pass with no route or response-order regressions.

---

### Task 3: Extract typed experience and growth domains

**Files:**
- Modify: `rust-server/crates/server/src/features/hero/service/mod.rs`
- Create: `rust-server/crates/server/src/features/hero/service/experience.rs`
- Create: `rust-server/crates/server/src/features/hero/service/growth.rs`
- Modify/remove: `rust-server/crates/server/src/features/hero/service/mod.rs`
- Test: existing experience, skill, intensify, advance, and remould tests originally in `service.rs`

**Interfaces:**
- Consumes: shared typed progress/resource helpers, `service::response`, `HeroTypedCatalogs`, and existing typed catalog types.
- Produces: `experience::handle` and `growth::handle`; the route method names and handler result semantics remain unchanged.

- [ ] **Step 1: Move experience-only helpers and the `hero.AddExp` branch**

Move `typed_hero_max_hp`, `hp_after_level_up`, `refresh_typed_hero_hp_after_level_up`, the existing hero experience response encoder, and the `hero.AddExp` branch into `experience.rs`. Preserve level cap handling, experience item consumption, task event `10`, HP refresh semantics, and the order of hero/bag/task updates before the reply.

- [ ] **Step 2: Move growth handlers as cohesive operations**

Move `handle_skill_upgrade`, `handle_intensify`, `handle_advance`, `handle_advance_max_level`, `handle_advance_mub`, and `handle_remould` into `growth.rs`. Keep resource validation before mutation, account snapshot/rollback behavior, hero/equipment cleanup, progress keys (`advance`, `advLv`, and `intensify:*`), and existing response effects unchanged.

- [ ] **Step 3: Keep shared resource and occupancy helpers at the narrowest stable boundary**

Move or expose `hero_is_in_use`, `hero_progress`, `set_hero_progress`, `typed_currency_kind`, `typed_item_count`, `consume_typed_item`, `costs_available`, and `consume_costs` from `service/mod.rs` with `pub(super)` visibility. Add `///` comments that state the business invariant for each helper, especially atomic cost consumption and the compatibility progress-key namespace.

- [ ] **Step 4: Route experience and growth methods**

Update the typed dispatcher so `hero.AddExp` delegates to `experience::handle`, and skill/strengthening/advance/remould methods delegate to `growth::handle`. The dispatcher must remain free of state mutation other than passing the existing arguments through.

- [ ] **Step 5: Run the growth regression cycle**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p blueoath-server features::hero -- --nocapture
```

Expected: all existing hero growth tests pass; no changes to state-key or response assertions are needed.

---

### Task 4: Split compatibility protocols into illustration, relationship, combination, and treasure domains

**Files:**
- Rename/modify: `rust-server/crates/server/src/features/hero/compat_service.rs` → `rust-server/crates/server/src/features/hero/compat/mod.rs` only as an intermediate move, then remove moved implementations
- Create: `rust-server/crates/server/src/features/hero/compat/illustration.rs`
- Create: `rust-server/crates/server/src/features/hero/compat/relationship.rs`
- Create: `rust-server/crates/server/src/features/hero/compat/combination.rs`
- Create: `rust-server/crates/server/src/features/hero/compat/treasure.rs`
- Modify: `rust-server/crates/server/src/features/hero/mod.rs`
- Test: existing compatibility tests originally in `compat_service.rs`

**Interfaces:**
- Consumes: existing `compat_service::handles_typed` and `compat_service::handle_typed` signatures, shared state payload helpers, and typed service response/reward utilities.
- Produces: `compat_service::handles_typed`, `compat_service::handle_typed`, `illustration::handle`, `relationship::handle`, `combination::handle`, and `treasure::handle` with identical return values and effects.

- [ ] **Step 1: Move illustration payload helpers and methods**

Move `illustrate_info_payload_for_templates`, `illustrate_info_payload_for_entries`, `append_illustrate_info_item`, and the `illustrate.VowDecTime`, `illustrate.VowHero`, `illustrate.IllustrateNew`, `illustrate.AddBehaviour`, `illustrate.EquipNew`, and `illustrate.ModiVowHeroList` branches into `illustration.rs`. Preserve all `compat:illustrate:*` and `compat:illustrateEquip:*` keys and the repeated field-9 payload marker.

- [ ] **Step 2: Move relationship and repair methods**

Move the `hero.Marry`, `hero.AddAffection`, and `repair.RepairHero` branches plus their private helpers into `relationship.rs`. Preserve the oath-ring template rule, affection bounds, HP repair calculation, resource/item consumption, and response effects.

- [ ] **Step 3: Move combination methods**

Move `typed_combination_value`, `set_typed_combination_value`, `typed_combination_rule`, `typed_combination_cost_available`, `typed_combination_consume`, and `handle_typed_combination` into `combination.rs`. Preserve `combine`/`beCombined` progress keys, level/grade limits, cost rollback semantics, and all four `hero.HeroCombine*` method routes.

- [ ] **Step 4: Move treasure methods and reward helpers**

Move `encode_treasure_response`, `typed_treasure_reward_supported`, `typed_next_hero_id`, `typed_next_equip_id`, `grant_typed_treasure_reward`, and `handle_typed_treasure` into `treasure.rs`. Preserve random-roll inputs, selected-option validation, item consumption timing, reward rollback, instance IDs, tombstones, and pre/post response placement.

- [ ] **Step 5: Rebuild the compatibility router**

Implement `compat_service::handles_typed` with the same method set and make `compat_service::handle_typed` route each method group to exactly one new domain module. Keep fixed `cachedata.CacheData` and `user.GetHeadBuyCount` responses in `compat/mod.rs`; no new compatibility routes may be added.

- [ ] **Step 6: Run compatibility regression tests**

Run:

```powershell
cargo fmt --all -- --check
cargo test -p blueoath-server features::hero -- --nocapture
```

Expected: all existing marriage, affection, combination, treasure, and illustration tests pass with the same response bytes and state values.

---

### Task 5: Complete documentation, remove transitional duplication, and verify the full crate

**Files:**
- Modify: all new `features/hero/service/*.rs` and `features/hero/compat/*.rs` files
- Modify: `rust-server/crates/server/src/features/hero/state.rs`
- Modify: `rust-server/crates/server/src/features/hero/mod.rs`
- Confirm absent: old `service.rs` and `compat_service.rs` paths after their implementations have been renamed into directory modules
- Test: `rust-server/crates/server` complete test suite

**Interfaces:**
- Consumes: all extracted domain handlers and existing tests.
- Produces: final domainized hero module with documented public/internal interfaces and no duplicated production implementations.

- [ ] **Step 1: Add module-level and function-level documentation**

Document every new module with `//!`. Add `///` docs to every `pub(crate)`/`pub(super)` handler and every non-obvious private helper. Explicitly document resource consumption, progress-key namespaces, response ordering, rollback guarantees, and protocol compatibility where those rules are relied upon.

- [ ] **Step 2: Check documentation coverage mechanically**

Run:

```powershell
rg -n "^(pub\(crate\)|pub\(super\)|pub )?(async )?fn " rust-server/crates/server/src/features/hero
```

Review each listed function and verify it has an immediately preceding `///` block unless it is a trivial one-line conversion or a test function.

- [ ] **Step 3: Remove old duplicate implementations**

Confirm the original monolithic paths remain absent after the directory rename, and remove any leftover duplicate handler implementation from the directory modules only after all call sites and tests compile through the new modules. If a compatibility path is required by existing imports, leave only a documented re-export/thin wrapper; do not retain a second handler implementation.

- [ ] **Step 4: Verify the complete server crate**

Run from `rust-server`:

```powershell
cargo fmt --all -- --check
cargo test -p blueoath-server
cargo clippy -p blueoath-server --all-targets --all-features -- -D warnings
```

Expected: all commands exit successfully with zero test failures, zero formatting differences, and zero clippy warnings introduced by the refactor.

- [ ] **Step 5: Review the scoped diff**

Run from the repository root:

```powershell
git status --short
git diff --stat -- rust-server/crates/server/src/features/hero
git diff --check -- rust-server/crates/server/src/features/hero
```

Confirm that only the hero feature files and the previously committed design/plan documents are part of this work; do not stage or alter unrelated existing modifications.

- [ ] **Step 6: Commit the implementation**

After the verification commands pass, stage only the hero feature files and commit:

```powershell
git add -- rust-server/crates/server/src/features/hero
git commit -m "refactor(server): split hero feature by domain"
```
