# Doctor Dual-Engine (OpenClaw + zeroclaw) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 在不重写 Doctor UI 与权限模型的前提下，新增可切换的双引擎（OpenClaw / zeroclaw），默认使用 zeroclaw。  
**Architecture:** 在 `src-tauri` 引入统一 `DoctorRuntime` 抽象，分别实现 OpenClaw 与 zeroclaw runtime；前端只增加 `engine` 参数与选择器，继续消费同一套 `doctor:*` 事件。命令执行仍走既有 ClawPal 节点审批链路。  
**Tech Stack:** Tauri 2 (Rust), React + TypeScript, existing NodeClient/BridgeClient, zeroclaw sidecar

---

### Task 1: 定义 Doctor runtime 抽象与引擎枚举

**Files:**
- Create: `src-tauri/src/doctor/runtime.rs`
- Create: `src-tauri/src/doctor/openclaw_runtime.rs`
- Create: `src-tauri/src/doctor/zeroclaw_runtime.rs`
- Modify: `src-tauri/src/doctor/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/doctor_runtime_selection.rs`

**Step 1: Write the failing test**

```rust
use clawpal::doctor::{DoctorEngine, parse_engine};

#[test]
fn parse_engine_defaults_to_zeroclaw() {
    assert_eq!(parse_engine(None).unwrap(), DoctorEngine::ZeroClaw);
}

#[test]
fn parse_engine_accepts_openclaw_and_zeroclaw() {
    assert_eq!(parse_engine(Some("openclaw".into())).unwrap(), DoctorEngine::OpenClaw);
    assert_eq!(parse_engine(Some("zeroclaw".into())).unwrap(), DoctorEngine::ZeroClaw);
}

#[test]
fn parse_engine_rejects_unknown() {
    let err = parse_engine(Some("foo".into())).unwrap_err();
    assert!(err.contains("Unsupported doctor engine"));
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test doctor_runtime_selection -- --nocapture`  
Expected: FAIL（`DoctorEngine`/`parse_engine` 不存在）

**Step 3: Write minimal implementation**

在 `runtime.rs` 增加：

- `DoctorEngine` 枚举：`OpenClaw`、`ZeroClaw`
- `parse_engine(input: Option<String>) -> Result<DoctorEngine, String>`
- `DoctorRuntime` trait（生命周期与消息接口）

在 `doctor/mod.rs` 暴露类型，在 `lib.rs` 注册新模块。

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test doctor_runtime_selection -- --nocapture`  
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/doctor/runtime.rs src-tauri/src/doctor/openclaw_runtime.rs src-tauri/src/doctor/zeroclaw_runtime.rs src-tauri/src/doctor/mod.rs src-tauri/src/lib.rs src-tauri/tests/doctor_runtime_selection.rs
git commit -m "refactor: add doctor runtime abstraction and engine parsing"
```

### Task 2: 后端命令增加 engine 参数并路由 runtime

**Files:**
- Modify: `src-tauri/src/doctor_commands.rs`
- Modify: `src-tauri/src/node_client.rs`
- Modify: `src-tauri/src/bridge_client.rs`
- Test: `src-tauri/tests/doctor_engine_commands.rs`

**Step 1: Write the failing test**

```rust
use serde_json::json;

#[tokio::test]
async fn start_diagnosis_defaults_to_zeroclaw_engine() {
    let payload = json!({
      "context": "ctx",
      "sessionKey": "s1",
      "agentId": "main"
    });
    let engine = clawpal::doctor::extract_engine_from_payload(&payload).unwrap();
    assert_eq!(engine.as_str(), "zeroclaw");
}

#[tokio::test]
async fn start_diagnosis_rejects_unknown_engine() {
    let payload = json!({
      "context": "ctx",
      "sessionKey": "s1",
      "agentId": "main",
      "engine": "bad"
    });
    let err = clawpal::doctor::extract_engine_from_payload(&payload).unwrap_err();
    assert!(err.contains("Unsupported doctor engine"));
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test doctor_engine_commands -- --nocapture`  
Expected: FAIL（`extract_engine_from_payload` 不存在）

**Step 3: Write minimal implementation**

在 `doctor_commands.rs`：

- `doctor_start_diagnosis` 新增 `engine: Option<String>`
- `doctor_send_message` 新增 `engine: Option<String>`（会话存在时以会话 engine 为准）
- 根据 engine 选择 runtime 分发

保持 invoke 审批与 `doctor_approve_invoke/doctor_reject_invoke` 行为不变。

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test doctor_engine_commands -- --nocapture`  
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/doctor_commands.rs src-tauri/src/node_client.rs src-tauri/src/bridge_client.rs src-tauri/tests/doctor_engine_commands.rs
git commit -m "feat: route doctor commands by engine"
```

### Task 3: 前端接入 engine 参数与选择器

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/use-doctor-agent.ts`
- Modify: `src/pages/Doctor.tsx`
- Modify: `src/locales/zh.json`
- Modify: `src/locales/en.json`
- Test: `src/lib/__tests__/doctor-engine-params.test.ts`

**Step 1: Write the failing test**

```ts
import { describe, it, expect } from "vitest";
import { buildDoctorStartPayload } from "../use-doctor-agent";

describe("doctor engine payload", () => {
  it("defaults to zeroclaw", () => {
    const payload = buildDoctorStartPayload({ context: "ctx", sessionKey: "s1", agentId: "main" });
    expect(payload.engine).toBe("zeroclaw");
  });

  it("respects selected engine", () => {
    const payload = buildDoctorStartPayload({ context: "ctx", sessionKey: "s1", agentId: "main", engine: "openclaw" });
    expect(payload.engine).toBe("openclaw");
  });
});
```

**Step 2: Run test to verify it fails**

Run: `npm run test -- doctor-engine-params.test.ts`  
Expected: FAIL（`buildDoctorStartPayload` 不存在）

**Step 3: Write minimal implementation**

- `types.ts` 增加 `DoctorEngine = "openclaw" | "zeroclaw"`
- `api.ts` 的 `doctorStartDiagnosis/doctorSendMessage` 增加 `engine`
- `use-doctor-agent.ts` 维护当前 engine 状态并透传
- `Doctor.tsx` 增加引擎选择器（默认 zeroclaw）
- i18n 增加引擎标签文案

**Step 4: Run test to verify it passes**

Run: `npm run test -- doctor-engine-params.test.ts`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/lib/types.ts src/lib/api.ts src/lib/use-doctor-agent.ts src/pages/Doctor.tsx src/locales/zh.json src/locales/en.json src/lib/__tests__/doctor-engine-params.test.ts
git commit -m "feat: add doctor engine selector and api params"
```

### Task 4: 错误分类与可恢复提示

**Files:**
- Modify: `src-tauri/src/doctor/zeroclaw_runtime.rs`
- Modify: `src-tauri/src/doctor_commands.rs`
- Modify: `src/pages/Doctor.tsx`
- Modify: `src/locales/zh.json`
- Modify: `src/locales/en.json`
- Test: `src-tauri/tests/doctor_engine_errors.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn classify_model_not_found_error() {
    let msg = r#"Anthropic API error (404 Not Found): {"message":"model: claude-3-5-sonnet-latest"}"#;
    let code = clawpal::doctor::classify_engine_error(msg);
    assert_eq!(code, "MODEL_UNAVAILABLE");
}

#[test]
fn classify_missing_key_error() {
    let msg = "OpenRouter API key not set";
    let code = clawpal::doctor::classify_engine_error(msg);
    assert_eq!(code, "CONFIG_MISSING");
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test doctor_engine_errors -- --nocapture`  
Expected: FAIL（分类函数不存在）

**Step 3: Write minimal implementation**

- 新增 `classify_engine_error` 映射函数
- `doctor:error` 事件增加结构化字段：`engine`、`code`、`message`、`actionHint`
- Doctor 页面按错误码展示恢复操作提示（如切引擎、补 key）

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test doctor_engine_errors -- --nocapture`  
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/doctor/zeroclaw_runtime.rs src-tauri/src/doctor_commands.rs src/pages/Doctor.tsx src/locales/zh.json src/locales/en.json src-tauri/tests/doctor_engine_errors.rs
git commit -m "fix: add doctor engine error classification and recovery hints"
```

### Task 5: 端到端回归验证与文档同步

**Files:**
- Modify: `docs/mvp-checklist.md`
- Modify: `docs/plans/2026-02-25-doctor-dual-engine-zeroclaw-design.md`
- Create: `docs/plans/2026-02-25-doctor-dual-engine-zeroclaw-implementation.md`

**Step 1: Write the failing checklist item**

在 `docs/mvp-checklist.md` 增加未完成项：

- [ ] Doctor 支持 OpenClaw / zeroclaw 双引擎切换
- [ ] 两引擎都能完成一次 invoke 审批闭环
- [ ] engine 失败可给出可执行恢复动作

**Step 2: Run verification commands**

Run:

```bash
npm run typecheck
cd src-tauri && cargo test --test doctor_runtime_selection -- --nocapture
cd src-tauri && cargo test --test doctor_engine_commands -- --nocapture
cd src-tauri && cargo test --test doctor_engine_errors -- --nocapture
```

Expected: 全部 PASS

**Step 3: Write implementation record**

在 `docs/plans/2026-02-25-doctor-dual-engine-zeroclaw-implementation.md` 记录：

- 已实现能力
- 未实现项（如 `autoFailover`）
- 验证命令与结果

**Step 4: Mark checklist as done**

将相关条目标记为已完成（如实现满足）。

**Step 5: Commit**

```bash
git add docs/mvp-checklist.md docs/plans/2026-02-25-doctor-dual-engine-zeroclaw-design.md docs/plans/2026-02-25-doctor-dual-engine-zeroclaw-implementation.md
git commit -m "docs: update doctor dual-engine verification and implementation notes"
```

