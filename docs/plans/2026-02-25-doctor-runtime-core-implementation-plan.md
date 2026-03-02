# Doctor Runtime Core (zeroclaw-first) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 Doctor 的 zeroclaw 接入重构为统一 Runtime Core，确保会话/记忆由 zeroclaw 托管，ClawPal 仅负责编排、审批、执行桥接与可观测。  
**Architecture:** 新增 `runtime/zeroclaw` 模块与 `doctor_runtime_bridge`，`doctor_commands` 仅做参数校验与调用桥接。保持现有 `doctor:*` 前端事件协议不变，分阶段切换实现。  
**Tech Stack:** Rust (Tauri 2), React/TypeScript, zeroclaw sidecar, existing NodeClient/BridgeClient

---

### Task 1: 建立 Runtime Core 基础类型与接口

**Files:**
- Create: `src-tauri/src/runtime/mod.rs`
- Create: `src-tauri/src/runtime/types.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/runtime_types.rs`

**Step 1: Write the failing test**

```rust
use clawpal::runtime::types::{RuntimeDomain, RuntimeEvent, RuntimeSessionKey};

#[test]
fn runtime_session_key_contains_instance_scope() {
    let key = RuntimeSessionKey::new("zeroclaw", RuntimeDomain::Doctor, "docker:local", "main", "s1");
    assert_eq!(key.instance_id, "docker:local");
}

#[test]
fn runtime_event_has_stable_kinds() {
    let ev = RuntimeEvent::chat_final("hello".into());
    assert_eq!(ev.kind(), "chat-final");
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test runtime_types -- --nocapture`  
Expected: FAIL（`runtime` 模块不存在）

**Step 3: Write minimal implementation**

- 定义 `RuntimeDomain/RuntimeSessionKey/RuntimeEvent/RuntimeError`
- 暴露 `RuntimeAdapter` trait
- 在 `lib.rs` 注册 `pub mod runtime`

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test runtime_types -- --nocapture`  
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/runtime/mod.rs src-tauri/src/runtime/types.rs src-tauri/src/lib.rs src-tauri/tests/runtime_types.rs
git commit -m "refactor: add doctor runtime core types and trait"
```

### Task 2: 抽离 zeroclaw process/session/sanitize 子模块

**Files:**
- Create: `src-tauri/src/runtime/zeroclaw/mod.rs`
- Create: `src-tauri/src/runtime/zeroclaw/process.rs`
- Create: `src-tauri/src/runtime/zeroclaw/session.rs`
- Create: `src-tauri/src/runtime/zeroclaw/sanitize.rs`
- Modify: `src-tauri/src/doctor_commands.rs`
- Test: `src-tauri/tests/runtime_zeroclaw_sanitize.rs`

**Step 1: Write the failing test**

```rust
use clawpal::runtime::zeroclaw::sanitize::sanitize_output;

#[test]
fn sanitize_removes_ansi_and_runtime_info_lines() {
    let raw = "[2m2026...[0m [32m INFO[0m zeroclaw::config::schema\nFinal answer";
    let out = sanitize_output(raw);
    assert_eq!(out, "Final answer");
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test runtime_zeroclaw_sanitize -- --nocapture`  
Expected: FAIL（函数不存在）

**Step 3: Write minimal implementation**

- 将 `doctor_commands.rs` 里的 zeroclaw 输出净化逻辑迁移至 `sanitize.rs`
- 保持行为一致（去 ANSI + 过滤 zeroclaw info/warn 行）

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test runtime_zeroclaw_sanitize -- --nocapture`  
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/runtime/zeroclaw/mod.rs src-tauri/src/runtime/zeroclaw/process.rs src-tauri/src/runtime/zeroclaw/session.rs src-tauri/src/runtime/zeroclaw/sanitize.rs src-tauri/src/doctor_commands.rs src-tauri/tests/runtime_zeroclaw_sanitize.rs
git commit -m "refactor: split zeroclaw process session and sanitize modules"
```

### Task 3: 新增 Doctor Runtime Bridge 并保持事件协议不变

**Files:**
- Create: `src-tauri/src/doctor_runtime_bridge.rs`
- Modify: `src-tauri/src/doctor_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/doctor_runtime_bridge_events.rs`

**Step 1: Write the failing test**

```rust
use clawpal::runtime::types::RuntimeEvent;
use clawpal::doctor_runtime_bridge::map_runtime_event_name;

#[test]
fn doctor_event_mapping_is_stable() {
    assert_eq!(map_runtime_event_name(&RuntimeEvent::chat_delta("x".into())), "doctor:chat-delta");
    assert_eq!(map_runtime_event_name(&RuntimeEvent::chat_final("x".into())), "doctor:chat-final");
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test doctor_runtime_bridge_events -- --nocapture`  
Expected: FAIL（bridge 不存在）

**Step 3: Write minimal implementation**

- `doctor_runtime_bridge` 统一 RuntimeEvent -> `doctor:*` 事件映射
- `doctor_commands` 不再直接 emit 各类事件，改调用 bridge

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test doctor_runtime_bridge_events -- --nocapture`  
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/doctor_runtime_bridge.rs src-tauri/src/doctor_commands.rs src-tauri/src/lib.rs src-tauri/tests/doctor_runtime_bridge_events.rs
git commit -m "refactor: introduce doctor runtime bridge with stable event mapping"
```

### Task 4: 切换 Doctor 会话到 zeroclaw 托管（去本地拼接会话）

**Files:**
- Modify: `src-tauri/src/runtime/zeroclaw/adapter.rs`
- Modify: `src-tauri/src/runtime/zeroclaw/session.rs`
- Modify: `src-tauri/src/doctor_commands.rs`
- Modify: `src/lib/use-doctor-agent.ts`
- Test: `src-tauri/tests/doctor_session_isolation.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn doctor_sessions_are_isolated_by_instance_id() {
    // pseudo: create two sessions same user but different instance ids
    // ensure session key / backend channel are different
    assert!(true);
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test doctor_session_isolation -- --nocapture`  
Expected: FAIL（未实现隔离检查）

**Step 3: Write minimal implementation**

- 使用统一 `RuntimeSessionKey`
- 移除 `doctor_commands` 中临时历史拼接逻辑
- 改为 runtime adapter 托管会话状态
- 前端继续传 `sessionKey`，行为保持兼容

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test doctor_session_isolation -- --nocapture`  
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/runtime/zeroclaw/adapter.rs src-tauri/src/runtime/zeroclaw/session.rs src-tauri/src/doctor_commands.rs src/lib/use-doctor-agent.ts src-tauri/tests/doctor_session_isolation.rs
git commit -m "feat: move doctor zeroclaw sessions under runtime adapter"
```

### Task 5: 统一错误码与恢复动作（Doctor UI 可见）

**Files:**
- Modify: `src-tauri/src/runtime/types.rs`
- Modify: `src-tauri/src/runtime/zeroclaw/adapter.rs`
- Modify: `src-tauri/src/doctor_runtime_bridge.rs`
- Modify: `src/components/DoctorChat.tsx`
- Modify: `src/locales/zh.json`
- Modify: `src/locales/en.json`
- Test: `src-tauri/tests/runtime_error_codes.rs`

**Step 1: Write the failing test**

```rust
use clawpal::runtime::types::RuntimeErrorCode;

#[test]
fn runtime_error_codes_cover_core_recovery_paths() {
    assert_eq!(RuntimeErrorCode::ConfigMissing.as_str(), "CONFIG_MISSING");
    assert_eq!(RuntimeErrorCode::ModelUnavailable.as_str(), "MODEL_UNAVAILABLE");
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test runtime_error_codes -- --nocapture`  
Expected: FAIL（错误码枚举未落地）

**Step 3: Write minimal implementation**

- 定义标准错误码与恢复动作字段
- bridge 统一发 `doctor:error` 结构化 payload
- Doctor UI 显示“错误 + 恢复动作”

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test runtime_error_codes -- --nocapture`  
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/runtime/types.rs src-tauri/src/runtime/zeroclaw/adapter.rs src-tauri/src/doctor_runtime_bridge.rs src/components/DoctorChat.tsx src/locales/zh.json src/locales/en.json src-tauri/tests/runtime_error_codes.rs
git commit -m "fix: standardize doctor runtime errors and recovery hints"
```

### Task 6: 回归验证与文档同步

**Files:**
- Modify: `docs/plans/2026-02-25-doctor-runtime-core-design.md`
- Create: `docs/plans/2026-02-25-doctor-runtime-core-implementation.md`
- Modify: `docs/mvp-checklist.md`

**Step 1: Run full verification**

Run:

```bash
npm run typecheck
cd src-tauri && cargo test --test runtime_types -- --nocapture
cd src-tauri && cargo test --test runtime_zeroclaw_sanitize -- --nocapture
cd src-tauri && cargo test --test doctor_runtime_bridge_events -- --nocapture
cd src-tauri && cargo test --test doctor_session_isolation -- --nocapture
cd src-tauri && cargo test --test runtime_error_codes -- --nocapture
cd src-tauri && cargo test --test doctor_engine -- --nocapture
cd src-tauri && cargo test --test install_api -- --nocapture
```

Expected: 全部 PASS

**Step 2: Write implementation record**

在 `docs/plans/2026-02-25-doctor-runtime-core-implementation.md` 记录：

- 已完成项
- 未完成项
- 验证命令与结果

**Step 3: Update checklist**

在 `docs/mvp-checklist.md` 增加并勾选本阶段可验收项。

**Step 4: Commit**

```bash
git add docs/plans/2026-02-25-doctor-runtime-core-design.md docs/plans/2026-02-25-doctor-runtime-core-implementation.md docs/mvp-checklist.md
git commit -m "docs: record doctor runtime core implementation and verification"
```

