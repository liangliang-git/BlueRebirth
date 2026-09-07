# Mood and Affection Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make server-side mood and affection behavior match the installed client configuration, with configurable recovery/gain multipliers and persisted state updates.

**Architecture:** Keep gameplay values in the existing fixed-point units (mood scale 10,000; affection scale 10,000 where configured), add server-owned runtime multipliers to `ServerConfig`/`ServerState`, and centralize mood/affection arithmetic in account-state helpers. Apply elapsed-time mood recovery during account load/request processing, apply bath recovery from elapsed bath time, and apply battle affection/mood settlement from catalog rules before emitting `HeroBag` pushes.

**Tech Stack:** Rust workspace, `serde_json` account snapshots, XOR-encoded SQLite catalog DBs, Cargo unit/integration tests.

**Spec:** Client `config_parameter.db` rows 139-145 and 206-222; client `config_affection_mood.db`; client `config_affection_change.db`.

## Global Constraints

- Preserve fixed-point values received by the client; do not convert stored mood or affection to display units.
- Default new ships and accounts to client mood `1500000`.
- Clamp mood to `0..=1500000` and affection to `0..=1000000` unmarried or `0..=2000000` married.
- Default `moodRecoveryMultiplier` and `affectionMultiplier` to `1.0`; normalize invalid, negative, or non-finite values using existing multiplier rules.
- Preserve existing account fields and unrelated working-tree changes.
- Every production behavior change gets a failing regression test before implementation and a passing rerun after implementation.

---

### Task 1: Add server tuning parameters

**Files:**
- Modify: `rust-server/crates/server/src/config.rs`
- Modify: `rust-server/crates/server/src/runtime.rs`
- Modify: `server.json`
- Test: `rust-server/crates/server/tests/config.rs`

**Interfaces:**
- Consumes: existing `ServerConfig::from_args`, `ServerState`, `normalize_multiplier`.
- Produces: `mood_recovery_multiplier` and `affection_multiplier` on `ServerConfig` and `ServerState`; JSON keys `moodRecoveryMultiplier` and `affectionMultiplier`; CLI flags `--mood-recovery-multiplier` and `--affection-multiplier`.

- [ ] **Step 1: Write failing config tests**

Add assertions that config-file and CLI values parse, invalid negative values fail, and defaults equal `1.0`.

- [ ] **Step 2: Run config tests and verify RED**

Run `cargo test --target-dir target-mood-affection -p blueoath-server --test config -- mood`.
Expected: compile failure because fields and flags do not exist.

- [ ] **Step 3: Implement config propagation**

Add fields to `ServerConfig`, `ServerState`, and `ServerFileConfig`; parse both `--flag=value` and separated values; include flags in `is_known_flag`; initialize runtime state from config; add `1.0` defaults to `server.json`.

- [ ] **Step 4: Run config tests and verify GREEN**

Run `cargo test --target-dir target-mood-affection -p blueoath-server --test config -- mood`.
Expected: all matching tests pass.

### Task 2: Centralize client mood and affection arithmetic

**Files:**
- Modify: `rust-server/crates/server/src/lib.rs`
- Modify: `rust-server/crates/server/src/account_state.rs`
- Modify: `rust-server/crates/server/src/projection.rs`
- Test: `rust-server/crates/server/src/tests.rs`

**Interfaces:**
- Consumes: Task 1 multipliers; account JSON heroes.
- Produces: helpers for clamping and scaled mood/affection deltas, preserving `MOOD_MIN`, `MOOD_MAX`, and `MOOD_INITIAL` behavior.

- [ ] **Step 1: Write failing arithmetic tests**

Cover multiplier `0.0`, `1.0`, `2.0`, fractional rounding, negative deltas, mood upper/lower clamp, unmarried/married affection caps, and missing/zero mood repair.

- [ ] **Step 2: Run tests and verify RED**

Run `cargo test --target-dir target-mood-affection -p blueoath-server --lib mood_affection`.
Expected: missing helper or incorrect expected result.

- [ ] **Step 3: Implement helpers**

Use integer-safe saturating arithmetic after applying normalized finite multipliers; use `MOOD_MIN..=MOOD_MAX`; keep affection cap based on `marryTime`; keep protocol projection fallback at `MOOD_INITIAL`.

- [ ] **Step 4: Run tests and verify GREEN**

Run the same focused command and confirm all boundary cases pass.

### Task 3: Implement elapsed natural mood recovery

**Files:**
- Modify: `rust-server/crates/server/src/account_state.rs`
- Modify: `rust-server/crates/server/src/runtime.rs`
- Test: `rust-server/crates/server/src/tests.rs`

**Interfaces:**
- Consumes: client parameters `mood_addtime=6`, `mood_normal_add=100`, `mood_normal_limit=1190000`, `mood_marry_add=100`; Task 1 recovery multiplier.
- Produces: persisted elapsed recovery based on a server-only per-hero timestamp, with no protocol schema change.

- [ ] **Step 1: Write failing elapsed-recovery tests**

Test no elapsed time, one six-minute interval, multiple intervals, partial interval retention, normal-limit cap, hard-cap clamp, multiplier `0.0` and `2.0`, and account-load persistence.

- [ ] **Step 2: Run tests and verify RED**

Run `cargo test --target-dir target-mood-affection -p blueoath-server --lib natural_mood_recovery`.
Expected: no recovery helper or unchanged mood.

- [ ] **Step 3: Implement recovery**

Store `moodUpdateTime` in hero JSON; calculate complete intervals from current Unix seconds; multiply recovery increment; stop natural recovery at `MOOD_NORMAL_LIMIT`; update timestamp without granting time twice; call it in both account load paths before save.

- [ ] **Step 4: Run tests and verify GREEN**

Run the focused recovery tests and confirm persistence behavior.

### Task 4: Complete bath mood recovery

**Files:**
- Modify: `rust-server/crates/server/src/building_state.rs`
- Modify: `rust-server/crates/server/src/progression_handler.rs`
- Modify: `rust-server/crates/server/src/account_state.rs`
- Test: `rust-server/crates/server/src/tests.rs`

**Interfaces:**
- Consumes: client `mood_bath_add=40000`, `ship_bath_mood_up=300000`, bath timestamps, Task 1 recovery multiplier.
- Produces: elapsed bath recovery, explicit service bonus, final `HeroBag` push, no unconditional full reset unless calculated value reaches cap.

- [ ] **Step 1: Write failing bath tests**

Cover bath duration below/at/above recovery interval, service bonus, multiple heroes, multiplier, cap, bath end idempotency, and post-operation HeroBag mood.

- [ ] **Step 2: Run tests and verify RED**

Run `cargo test --target-dir target-mood-affection -p blueoath-server --lib bath_mood`.
Expected: current implementation restores full regardless of duration, so duration/bonus assertions fail.

- [ ] **Step 3: Implement bath calculation**

Read the persisted bath entry’s start/end timing; apply elapsed bath increment plus configured bath bonus through shared clamp/multiplier helpers; remove only completed bath entries; update hero mood timestamp.

- [ ] **Step 4: Run tests and verify GREEN**

Run focused bath tests and confirm repeated `BathEnd` does not double-apply.

### Task 5: Apply affection and mood battle rules with multipliers

**Files:**
- Modify: `rust-server/crates/server/src/battle_state.rs`
- Modify: `rust-server/crates/server/src/game_login.rs`
- Modify: `rust-server/crates/server/src/catalog_loader.rs` only if rule plumbing needs extension.
- Test: `rust-server/crates/server/src/tests.rs`

**Interfaces:**
- Consumes: `BattleSettlementRule`, Task 1 multipliers, battle pass hero/MVP/shipwreck IDs.
- Produces: one atomic settlement operation applying configured affection gains/reductions and mood reductions, clamped and persisted before response pushes.

- [ ] **Step 1: Write failing multiplier tests**

Cover base, flagship, MVP, shipwreck reduction, mood normal/shipwreck reduction, unmarried/married affection caps, zero multipliers, and missing catalog rule.

- [ ] **Step 2: Run tests and verify RED**

Run `cargo test --target-dir target-mood-affection -p blueoath-server --lib battle_settlement`.
Expected: multiplier assertions fail because settlement currently has no runtime multiplier input.

- [ ] **Step 3: Implement scaled settlement**

Pass both multipliers from runtime state; scale positive affection gains and configured reductions consistently; clamp final values; keep settlement atomic and mark account changed only when fields change.

- [ ] **Step 4: Run tests and verify GREEN**

Run focused settlement tests and confirm battle response includes updated `HeroBag`.

### Task 6: Implement mood-stage effects and complete validation

**Files:**
- Modify: `rust-server/crates/server/src/catalog.rs`
- Modify: `rust-server/crates/server/src/catalog_loader.rs`
- Modify: `rust-server/crates/server/src/battle_state.rs`
- Modify: `rust-server/crates/server/src/game_login.rs`
- Test: `rust-server/crates/server/src/tests.rs`
- Test: `rust-server/crates/server/tests/game_login.rs`

**Interfaces:**
- Consumes: client `config_affection_mood.db` stage rows.
- Produces: server-side mood stage lookup for affection gain suppression and high-mood experience bonus, with protocol-visible updated values.

- [ ] **Step 1: Write failing stage-effect tests**

Assert stages `0`, `1..309999`, `310000..1199999`, `1200000..1500000`; low mood blocks battle affection gain; high mood scales ship battle experience by `12000/10000`; boundary values select correct stage.

- [ ] **Step 2: Run tests and verify RED**

Run `cargo test --target-dir target-mood-affection -p blueoath-server --lib mood_stage`.
Expected: stage catalog is absent and effects are not applied.

- [ ] **Step 3: Implement stage catalog and effects**

Load `config_affection_mood.db` into a sorted stage table; select by inclusive minimum/maximum; apply `mood_affection_add` and `mood_exp_up` as fixed-point multipliers; preserve low-mood no-affection rule from client text.

- [ ] **Step 4: Run focused integration tests and verify GREEN**

Run stage and game-login tests; confirm updated affection, mood, and experience are persisted and pushed.

### Task 7: Full verification and operational handoff

**Files:**
- Review: all files changed by Tasks 1-6.
- Review: `server.json`.

- [ ] **Step 1: Run full workspace tests**

Run `cargo test --target-dir target-mood-affection --workspace`.

- [ ] **Step 2: Run formatting, lint, build, and diff checks**

Run `cargo fmt --all -- --check`, `cargo clippy --target-dir target-mood-affection --workspace --all-targets -- -D warnings`, `cargo build --target-dir target-mood-affection --workspace`, and `git diff --check`.

- [ ] **Step 3: Review final diff**

Confirm only mood/affection config, state, settlement, tests, and plan changes are included; preserve unrelated dirty files.

- [ ] **Step 4: Report operational state**

Report changed locally, verified locally, build path, and that the running server must be restarted before new multipliers and calculations take effect.
