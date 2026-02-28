# Zeroclaw 异常兜底扩展 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extend zeroclaw's error fallback coverage from business-logic errors to software-level anomalies (auth, registry, transport, instance state), with actionable guidance cards and lightweight prechecks.

**Architecture:** Extend the existing `agent_fallback.rs` + `guidance.ts` pipeline. Add a `precheck` module in `clawpal-core` for lightweight pre-operation checks. Upgrade `ErrorGuidance` actions from plain strings to structured `GuidanceAction` with inline-fix and doctor-handoff capabilities. All fix actions execute through CLI tool intent pipeline.

**Tech Stack:** Rust (clawpal-core, src-tauri), TypeScript/React (frontend), existing zeroclaw + tool-intent infrastructure.

---

### Task 1: Extend RuntimeErrorCode enum

**Files:**
- Modify: `src-tauri/src/runtime/types.rs:54-75`

**Step 1: Write the failing test**

Add test in `src-tauri/src/runtime/types.rs` (or a new test file) that asserts the new error codes exist and serialize correctly:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_error_codes_have_correct_string_repr() {
        assert_eq!(RuntimeErrorCode::AuthExpired.as_str(), "AUTH_EXPIRED");
        assert_eq!(RuntimeErrorCode::AuthMisconfigured.as_str(), "AUTH_MISCONFIGURED");
        assert_eq!(RuntimeErrorCode::RegistryCorrupt.as_str(), "REGISTRY_CORRUPT");
        assert_eq!(RuntimeErrorCode::InstanceOrphaned.as_str(), "INSTANCE_ORPHANED");
        assert_eq!(RuntimeErrorCode::TransportStale.as_str(), "TRANSPORT_STALE");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test runtime::types::tests::new_error_codes_have_correct_string_repr -- --nocapture`
Expected: FAIL — variants don't exist yet.

**Step 3: Add new variants to RuntimeErrorCode**

In `src-tauri/src/runtime/types.rs`, add to the `RuntimeErrorCode` enum (after `TargetUnreachable`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorCode {
    RuntimeUnreachable,
    ConfigMissing,
    ModelUnavailable,
    SessionInvalid,
    TargetUnreachable,
    AuthExpired,
    AuthMisconfigured,
    RegistryCorrupt,
    InstanceOrphaned,
    TransportStale,
    Unknown,
}
```

Update `as_str()`:

```rust
impl RuntimeErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RuntimeUnreachable => "RUNTIME_UNREACHABLE",
            Self::ConfigMissing => "CONFIG_MISSING",
            Self::ModelUnavailable => "MODEL_UNAVAILABLE",
            Self::SessionInvalid => "SESSION_INVALID",
            Self::TargetUnreachable => "TARGET_UNREACHABLE",
            Self::AuthExpired => "AUTH_EXPIRED",
            Self::AuthMisconfigured => "AUTH_MISCONFIGURED",
            Self::RegistryCorrupt => "REGISTRY_CORRUPT",
            Self::InstanceOrphaned => "INSTANCE_ORPHANED",
            Self::TransportStale => "TRANSPORT_STALE",
            Self::Unknown => "UNKNOWN",
        }
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test runtime::types::tests -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/runtime/types.rs
git commit -m "feat: add software-level anomaly error codes to RuntimeErrorCode"
```

---

### Task 2: Extend classify_engine_error with new patterns

**Files:**
- Modify: `src-tauri/src/doctor.rs:29-50`

**Step 1: Write the failing tests**

Add to existing tests in `src-tauri/src/doctor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_auth_expired_401() {
        assert_eq!(classify_engine_error("HTTP 401 unauthorized"), "AUTH_EXPIRED");
    }

    #[test]
    fn classify_auth_expired_403() {
        assert_eq!(classify_engine_error("403 forbidden: quota exceeded"), "AUTH_EXPIRED");
    }

    #[test]
    fn classify_auth_expired_invalid_key() {
        assert_eq!(classify_engine_error("invalid api key provided"), "AUTH_EXPIRED");
    }

    #[test]
    fn classify_registry_corrupt() {
        assert_eq!(classify_engine_error("registry parse error: invalid json at line 5"), "REGISTRY_CORRUPT");
    }

    #[test]
    fn classify_instance_orphaned_container() {
        assert_eq!(classify_engine_error("Error: no such container: abc123"), "INSTANCE_ORPHANED");
    }

    #[test]
    fn classify_instance_orphaned_not_found() {
        assert_eq!(classify_engine_error("container def456 not found"), "INSTANCE_ORPHANED");
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test doctor::tests::classify_auth -- --nocapture`
Expected: FAIL — returns "ENGINE_ERROR" or "RUNTIME_UNREACHABLE" instead.

**Step 3: Extend classify_engine_error**

In `src-tauri/src/doctor.rs`, update `classify_engine_error()`. Insert new checks **before** the existing `"not found"` check (which is too broad and would swallow container errors):

```rust
pub fn classify_engine_error(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();

    // AUTH_EXPIRED: 401/403, invalid key, quota exceeded
    if lower.contains("unauthorized")
        || lower.contains("invalid api key")
        || lower.contains("invalid_api_key")
        || (lower.contains("403") && (lower.contains("forbidden") || lower.contains("quota")))
        || (lower.contains("401") && !lower.contains("model:"))
    {
        return "AUTH_EXPIRED";
    }

    // REGISTRY_CORRUPT: registry parse/json errors
    if (lower.contains("registry") || lower.contains("instances.json"))
        && (lower.contains("parse") || lower.contains("invalid json") || lower.contains("deserialize"))
    {
        return "REGISTRY_CORRUPT";
    }

    // INSTANCE_ORPHANED: container not found
    if lower.contains("no such container")
        || (lower.contains("container") && lower.contains("not found") && !lower.contains("openclaw"))
    {
        return "INSTANCE_ORPHANED";
    }

    // Existing checks below (unchanged)
    if lower.contains("api key not set")
        || lower.contains("no compatible api key")
        || lower.contains("no auth profiles configured")
    {
        return "CONFIG_MISSING";
    }
    if lower.contains("not_found_error")
        || (lower.contains("model:") && lower.contains("404"))
    {
        return "MODEL_UNAVAILABLE";
    }
    if lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("failed to start")
        || lower.contains("permission denied")
    {
        return "RUNTIME_UNREACHABLE";
    }
    "ENGINE_ERROR"
}
```

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test doctor::tests -- --nocapture`
Expected: All PASS

**Step 5: Commit**

```bash
git add src-tauri/src/doctor.rs
git commit -m "feat: extend classify_engine_error with auth/registry/container patterns"
```

---

### Task 3: Add GuidanceAction struct and upgrade ErrorGuidance

**Files:**
- Modify: `src-tauri/src/agent_fallback.rs:1-15`

**Step 1: Write the failing test**

```rust
#[test]
fn guidance_action_serializes_inline_fix() {
    let action = GuidanceAction {
        label: "重连 SSH".to_string(),
        action_type: "inline_fix".to_string(),
        tool: Some("clawpal".to_string()),
        args: Some("ssh connect --host test-host".to_string()),
        invoke_type: Some("read".to_string()),
        context: None,
    };
    let json = serde_json::to_value(&action).unwrap();
    assert_eq!(json["actionType"], "inline_fix");
    assert_eq!(json["tool"], "clawpal");
}

#[test]
fn guidance_action_serializes_doctor_handoff() {
    let action = GuidanceAction {
        label: "让小龙虾修复".to_string(),
        action_type: "doctor_handoff".to_string(),
        tool: None,
        args: None,
        invoke_type: None,
        context: Some("Container abc not found".to_string()),
    };
    let json = serde_json::to_value(&action).unwrap();
    assert_eq!(json["actionType"], "doctor_handoff");
    assert!(json["tool"].is_null());
    assert_eq!(json["context"], "Container abc not found");
}
```

**Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test agent_fallback::tests::guidance_action -- --nocapture`
Expected: FAIL — `GuidanceAction` doesn't exist.

**Step 3: Add GuidanceAction and update ErrorGuidance**

In `src-tauri/src/agent_fallback.rs`, add the new struct and update `ErrorGuidance`:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuidanceAction {
    pub label: String,
    pub action_type: String,        // "inline_fix" | "doctor_handoff"
    pub tool: Option<String>,       // "clawpal" | "openclaw"
    pub args: Option<String>,       // CLI args
    pub invoke_type: Option<String>,// "read" | "write"
    pub context: Option<String>,    // doctor_handoff context
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorGuidance {
    pub message: String,
    pub summary: String,
    pub actions: Vec<String>,
    pub structured_actions: Vec<GuidanceAction>,
    pub source: String,
}
```

Note: Keep `actions: Vec<String>` for backward compatibility. `structured_actions` is the new field with executable actions. Existing code that only produces string actions will have `structured_actions: vec![]`.

Update `explain_operation_error` to include `structured_actions: vec![]` in the return:

```rust
Ok(ErrorGuidance {
    message,
    summary: guidance.summary,
    actions: guidance.actions,
    structured_actions: vec![],
    source,
})
```

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test agent_fallback::tests -- --nocapture`
Expected: All PASS

**Step 5: Commit**

```bash
git add src-tauri/src/agent_fallback.rs
git commit -m "feat: add GuidanceAction struct with inline_fix and doctor_handoff types"
```

---

### Task 4: Extend rules_fallback with structured actions for new error types

**Files:**
- Modify: `src-tauri/src/agent_fallback.rs:59-132`

**Step 1: Write the failing tests**

```rust
#[test]
fn rules_fallback_handles_auth_expired() {
    let result = rules_fallback(
        "HTTP 401 unauthorized: invalid api key",
        "remote_ssh",
        "listAgents",
        None,
    );
    assert!(result.summary.contains("API") || result.summary.contains("密钥") || result.summary.contains("认证"));
    assert!(!result.structured_actions.is_empty());
    // Should have a doctor_handoff action
    assert!(result.structured_actions.iter().any(|a| a.action_type == "doctor_handoff"));
}

#[test]
fn rules_fallback_handles_container_not_found() {
    let result = rules_fallback(
        "Error: no such container: abc123",
        "docker_local",
        "getAgents",
        None,
    );
    assert!(result.summary.contains("容器") || result.summary.contains("container"));
    assert!(result.structured_actions.iter().any(|a| a.action_type == "doctor_handoff"));
}

#[test]
fn rules_fallback_ssh_stale_has_inline_fix() {
    let result = rules_fallback(
        "not connected to remote host",
        "remote_ssh",
        "getStatus",
        None,
    );
    assert!(result.structured_actions.iter().any(|a| a.action_type == "inline_fix" && a.tool.as_deref() == Some("clawpal")));
}
```

**Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test agent_fallback::tests::rules_fallback_handles_auth -- --nocapture`
Expected: FAIL — `structured_actions` doesn't exist on `GuidanceBody`.

**Step 3: Extend GuidanceBody and rules_fallback**

First, update `GuidanceBody` to include structured actions:

```rust
#[derive(Debug, Clone)]
struct GuidanceBody {
    summary: String,
    actions: Vec<String>,
    structured_actions: Vec<GuidanceAction>,
}
```

Then extend `rules_fallback` — add new pattern matches **before** the generic SSH check. Also add `structured_actions` to all existing return paths:

```rust
fn rules_fallback(
    error_text: &str,
    transport: &str,
    operation: &str,
    probe: Option<&OpenclawProbe>,
) -> GuidanceBody {
    let lower = error_text.to_lowercase();

    // --- Existing: ownerdisplay config mismatch ---
    if lower.contains("ownerdisplay")
        && (lower.contains("unknown field")
            || lower.contains("invalid field")
            || lower.contains("failed to parse")
            || lower.contains("deserialize"))
    {
        return GuidanceBody {
            summary: "检测到 openclaw 配置字段不兼容（ownerDisplay）。系统已尝试自动修复并建议复测。".to_string(),
            actions: vec![
                "重新进入该实例并等待 1-2 秒后自动刷新。".to_string(),
                "若仍失败，打开 Doctor 让 Agent继续执行更细粒度修复。".to_string(),
            ],
            structured_actions: vec![
                GuidanceAction {
                    label: "让小龙虾修复".to_string(),
                    action_type: "doctor_handoff".to_string(),
                    tool: None, args: None, invoke_type: None,
                    context: Some(format!("配置字段不兼容（ownerDisplay）: {}", error_text)),
                },
            ],
        };
    }

    // --- NEW: Auth expired (401/403/invalid key) ---
    if lower.contains("unauthorized")
        || lower.contains("invalid api key")
        || lower.contains("invalid_api_key")
        || (lower.contains("401") && !lower.contains("model:"))
        || (lower.contains("403") && (lower.contains("forbidden") || lower.contains("quota")))
    {
        return GuidanceBody {
            summary: "API 认证失败，密钥可能已过期或无效。".to_string(),
            actions: vec![
                "检查当前实例使用的能力档案（Profile）中的 API Key 是否仍然有效。".to_string(),
                "如需更换密钥，前往能力档案页面更新对应的 Provider 配置。".to_string(),
            ],
            structured_actions: vec![
                GuidanceAction {
                    label: "让小龙虾修复".to_string(),
                    action_type: "doctor_handoff".to_string(),
                    tool: None, args: None, invoke_type: None,
                    context: Some(format!("API 认证失败: {}", error_text)),
                },
            ],
        };
    }

    // --- NEW: Container not found (orphaned Docker instance) ---
    if lower.contains("no such container")
        || (lower.contains("container") && lower.contains("not found") && !lower.contains("openclaw"))
    {
        return GuidanceBody {
            summary: "实例对应的 Docker 容器已不存在，可能已被手动删除。".to_string(),
            actions: vec![
                "重新安装该实例，或从实例列表中移除。".to_string(),
                "打开 Doctor 页面让小龙虾诊断并修复。".to_string(),
            ],
            structured_actions: vec![
                GuidanceAction {
                    label: "让小龙虾修复".to_string(),
                    action_type: "doctor_handoff".to_string(),
                    tool: None, args: None, invoke_type: None,
                    context: Some(format!("Docker 容器不存在: {}", error_text)),
                },
            ],
        };
    }

    // --- Existing: openclaw binary missing ---
    if looks_like_openclaw_binary_missing(error_text) {
        // ... (keep existing logic, add structured_actions: vec![] or a doctor_handoff)
        let mut summary = "目标实例缺少 openclaw 命令，或登录 shell 的 PATH 未包含该命令。".to_string();
        let mut actions = Vec::new();
        if let Some(result) = probe {
            if let Some(path) = result.openclaw_path.as_deref() {
                summary = format!(
                    "探测到 openclaw 路径为 `{path}`，但当前业务调用仍报命令不存在，通常是登录 shell 初始化不一致。"
                );
                actions.push("检查远程登录 shell 配置（如 `.bashrc` / `.zshrc`）是否在非交互会话加载 PATH。".to_string());
                actions.push("在远程执行 `openclaw --version` 验证同一会话可直接运行。".to_string());
            } else {
                actions.push("自动探测已执行：`command -v openclaw` 未返回可执行路径。".to_string());
                actions.push("在目标实例安装/修复 openclaw 后，重新登录 SSH 会话。".to_string());
            }
            if let Some(path_env) = result.path.as_deref() {
                actions.push(format!("当前远程 PATH：`{path_env}`"));
            }
        }
        if actions.is_empty() {
            actions.push("在目标实例执行 openclaw 安装/修复脚本，并重新登录 shell。".to_string());
            actions.push("确认 `command -v openclaw` 可返回路径后，再重试当前操作。".to_string());
        }
        actions.push("进入 Doctor 页面并点击诊断，让内置 Agent 继续自动排查。".to_string());
        return GuidanceBody {
            summary,
            actions,
            structured_actions: vec![
                GuidanceAction {
                    label: "让小龙虾修复".to_string(),
                    action_type: "doctor_handoff".to_string(),
                    tool: None, args: None, invoke_type: None,
                    context: Some(format!("openclaw 命令缺失: {}", error_text)),
                },
            ],
        };
    }

    // --- Existing: SSH / connection refused (with inline_fix) ---
    if lower.contains("not connected to remote")
        || lower.contains("connection refused")
        || (lower.contains("ssh") && (lower.contains("failed") || lower.contains("error") || lower.contains("断开")))
    {
        return GuidanceBody {
            summary: "当前远程连接不可用，导致操作失败。".to_string(),
            actions: vec![
                "先在实例页重新连接 SSH，并确认网络可达。".to_string(),
                "执行一次健康检查，确认网关和配置目录可访问。".to_string(),
                "若仍失败，打开 Doctor 页面执行自动诊断并按建议修复。".to_string(),
            ],
            structured_actions: vec![
                GuidanceAction {
                    label: "重连 SSH".to_string(),
                    action_type: "inline_fix".to_string(),
                    tool: Some("clawpal".to_string()),
                    args: Some("ssh connect".to_string()),
                    invoke_type: Some("write".to_string()),
                    context: None,
                },
                GuidanceAction {
                    label: "让小龙虾修复".to_string(),
                    action_type: "doctor_handoff".to_string(),
                    tool: None, args: None, invoke_type: None,
                    context: Some(format!("SSH 连接失败: {}", error_text)),
                },
            ],
        };
    }

    // --- Existing: Generic fallback ---
    GuidanceBody {
        summary: format!("操作 `{operation}` 在 `{transport}` 环境执行失败，建议先做诊断再继续。"),
        actions: vec![
            "打开 Doctor 页面运行诊断，获取可执行修复步骤。".to_string(),
            "按诊断结果优先处理阻塞项后，再重试当前操作。".to_string(),
        ],
        structured_actions: vec![
            GuidanceAction {
                label: "让小龙虾修复".to_string(),
                action_type: "doctor_handoff".to_string(),
                tool: None, args: None, invoke_type: None,
                context: Some(format!("{operation} 执行失败: {error_text}")),
            },
        ],
    }
}
```

Also update `explain_operation_error` to pass `structured_actions` through:

```rust
Ok(ErrorGuidance {
    message,
    summary: guidance.summary,
    actions: guidance.actions,
    structured_actions: guidance.structured_actions,
    source,
})
```

**Step 4: Run all tests**

Run: `cd src-tauri && cargo test agent_fallback::tests -- --nocapture`
Expected: All PASS

**Step 5: Commit**

```bash
git add src-tauri/src/agent_fallback.rs
git commit -m "feat: extend rules_fallback with auth/container/ssh structured actions"
```

---

### Task 5: Create precheck module in clawpal-core

**Files:**
- Create: `clawpal-core/src/precheck.rs`
- Modify: `clawpal-core/src/lib.rs`

**Step 1: Write the failing tests**

Create `clawpal-core/src/precheck.rs` with tests first:

```rust
use serde::Serialize;
use std::path::Path;

use crate::instance::{Instance, InstanceRegistry, InstanceType};
use crate::profile::ModelProfile;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrecheckIssue {
    pub code: String,
    pub severity: String,       // "error" | "warn"
    pub message: String,
    pub auto_fixable: bool,
}

/// Check that each profile has provider + model and references a valid auth_ref.
pub fn precheck_auth(profiles: &[ModelProfile]) -> Vec<PrecheckIssue> {
    todo!()
}

/// Check that registry JSON is valid and instance home paths exist.
pub fn precheck_registry(registry_path: &Path) -> Vec<PrecheckIssue> {
    todo!()
}

/// Check that an instance's config directory exists and is readable.
pub fn precheck_instance_state(instance: &Instance) -> Vec<PrecheckIssue> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::InstanceType;

    #[test]
    fn precheck_auth_detects_missing_provider() {
        let profiles = vec![ModelProfile {
            id: "test".into(),
            name: "Test".into(),
            provider: "".into(),
            model: "claude-sonnet".into(),
            auth_ref: None,
            api_key: None,
            base_url: None,
            description: None,
            enabled: true,
        }];
        let issues = precheck_auth(&profiles);
        assert!(issues.iter().any(|i| i.code == "AUTH_MISCONFIGURED"));
    }

    #[test]
    fn precheck_auth_passes_valid_profiles() {
        let profiles = vec![ModelProfile {
            id: "ok".into(),
            name: "OK".into(),
            provider: "anthropic".into(),
            model: "claude-sonnet".into(),
            auth_ref: Some("key-1".into()),
            api_key: None,
            base_url: None,
            description: None,
            enabled: true,
        }];
        let issues = precheck_auth(&profiles);
        assert!(issues.is_empty());
    }

    #[test]
    fn precheck_registry_detects_missing_file() {
        let issues = precheck_registry(Path::new("/nonexistent/registry.json"));
        // Missing file is not an error (fresh install), should return empty
        assert!(issues.is_empty());
    }

    #[test]
    fn precheck_registry_detects_corrupt_json() {
        let dir = std::env::temp_dir().join(format!("precheck-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("instances.json");
        std::fs::write(&path, "{ corrupt json!!!").unwrap();
        let issues = precheck_registry(&path);
        assert!(issues.iter().any(|i| i.code == "REGISTRY_CORRUPT"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn precheck_instance_state_detects_missing_home() {
        let inst = Instance {
            id: "test".into(),
            instance_type: InstanceType::Local,
            label: "Test".into(),
            openclaw_home: Some("/nonexistent/path/openclaw".into()),
            clawpal_data_dir: None,
            ssh_host_config: None,
        };
        let issues = precheck_instance_state(&inst);
        assert!(issues.iter().any(|i| i.code == "INSTANCE_ORPHANED"));
    }

    #[test]
    fn precheck_instance_state_passes_when_no_home() {
        let inst = Instance {
            id: "remote".into(),
            instance_type: InstanceType::RemoteSsh,
            label: "Remote".into(),
            openclaw_home: None,
            clawpal_data_dir: None,
            ssh_host_config: None,
        };
        let issues = precheck_instance_state(&inst);
        // Remote instances without local home are fine
        assert!(issues.is_empty());
    }
}
```

**Step 2: Add module export and run tests to verify they fail**

Add to `clawpal-core/src/lib.rs`:

```rust
pub mod precheck;
```

Run: `cd clawpal-core && cargo test precheck::tests -- --nocapture`
Expected: FAIL — functions are `todo!()`.

**Step 3: Implement the precheck functions**

Replace the `todo!()` bodies:

```rust
pub fn precheck_auth(profiles: &[ModelProfile]) -> Vec<PrecheckIssue> {
    let mut issues = Vec::new();
    for p in profiles {
        if !p.enabled {
            continue;
        }
        if p.provider.is_empty() || p.model.is_empty() {
            issues.push(PrecheckIssue {
                code: "AUTH_MISCONFIGURED".into(),
                severity: "error".into(),
                message: format!(
                    "能力档案 '{}' 缺少 provider 或 model 配置",
                    p.name
                ),
                auto_fixable: false,
            });
        }
    }
    issues
}

pub fn precheck_registry(registry_path: &Path) -> Vec<PrecheckIssue> {
    if !registry_path.exists() {
        return vec![];
    }
    let text = match std::fs::read_to_string(registry_path) {
        Ok(t) => t,
        Err(e) => {
            return vec![PrecheckIssue {
                code: "REGISTRY_CORRUPT".into(),
                severity: "error".into(),
                message: format!("无法读取实例注册表: {e}"),
                auto_fixable: false,
            }];
        }
    };
    if serde_json::from_str::<serde_json::Value>(&text).is_err() {
        return vec![PrecheckIssue {
            code: "REGISTRY_CORRUPT".into(),
            severity: "error".into(),
            message: "实例注册表 JSON 格式损坏".into(),
            auto_fixable: false,
        }];
    }
    vec![]
}

pub fn precheck_instance_state(instance: &Instance) -> Vec<PrecheckIssue> {
    // Only check local paths — remote instances are checked via SSH at runtime
    if matches!(instance.instance_type, InstanceType::RemoteSsh) {
        return vec![];
    }
    if let Some(home) = &instance.openclaw_home {
        if !Path::new(home).exists() {
            return vec![PrecheckIssue {
                code: "INSTANCE_ORPHANED".into(),
                severity: "error".into(),
                message: format!("实例 '{}' 的 Home 目录不存在: {}", instance.label, home),
                auto_fixable: false,
            }];
        }
    }
    vec![]
}
```

**Step 4: Run tests to verify they pass**

Run: `cd clawpal-core && cargo test precheck::tests -- --nocapture`
Expected: All PASS

**Step 5: Commit**

```bash
git add clawpal-core/src/precheck.rs clawpal-core/src/lib.rs
git commit -m "feat: add precheck module for auth, registry, and instance state checks"
```

---

### Task 6: Add precheck Tauri commands

**Files:**
- Create: `src-tauri/src/commands/precheck.rs`
- Modify: `src-tauri/src/commands/mod.rs` (add module declaration)
- Modify: `src-tauri/src/main.rs` or wherever Tauri commands are registered

**Step 1: Create the Tauri command module**

```rust
// src-tauri/src/commands/precheck.rs
use clawpal_core::precheck::{self, PrecheckIssue};
use std::path::PathBuf;

#[tauri::command]
pub async fn precheck_registry() -> Result<Vec<PrecheckIssue>, String> {
    let registry_path = clawpal_core::instance::registry_path_public();
    Ok(precheck::precheck_registry(&registry_path))
}

#[tauri::command]
pub async fn precheck_instance(instance_id: String) -> Result<Vec<PrecheckIssue>, String> {
    let registry = clawpal_core::instance::InstanceRegistry::load()
        .map_err(|e| e.to_string())?;
    let instance = registry.get(&instance_id)
        .ok_or_else(|| format!("Instance not found: {instance_id}"))?;
    Ok(precheck::precheck_instance_state(instance))
}
```

Note: `registry_path_public()` needs to be exposed from `clawpal_core::instance`. The existing `registry_path()` is private. Either make it `pub` or add a public wrapper. The simplest change: make `registry_path()` public in `clawpal-core/src/instance.rs:175`.

**Step 2: Wire up in mod.rs**

Add to `src-tauri/src/commands/mod.rs`:

```rust
pub mod precheck;
```

**Step 3: Register commands in Tauri app builder**

Find where `.invoke_handler(tauri::generate_handler![...])` is called and add the new commands. This is typically in `src-tauri/src/main.rs` or `src-tauri/src/lib.rs`.

Add:
```rust
commands::precheck::precheck_registry,
commands::precheck::precheck_instance,
```

**Step 4: Compile and verify**

Run: `cd src-tauri && cargo check`
Expected: Compiles successfully.

**Step 5: Commit**

```bash
git add src-tauri/src/commands/precheck.rs src-tauri/src/commands/mod.rs clawpal-core/src/instance.rs src-tauri/src/main.rs
git commit -m "feat: add precheck Tauri commands for registry and instance validation"
```

---

### Task 7: Add frontend types for GuidanceAction

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`

**Step 1: Add GuidanceAction type**

In `src/lib/types.ts`, add:

```typescript
export interface GuidanceAction {
  label: string;
  actionType: "inline_fix" | "doctor_handoff";
  tool?: string;       // "clawpal" | "openclaw"
  args?: string;       // CLI args
  invokeType?: string; // "read" | "write"
  context?: string;    // doctor_handoff context
}

export interface PrecheckIssue {
  code: string;
  severity: "error" | "warn";
  message: string;
  autoFixable: boolean;
}
```

**Step 2: Update api.ts**

Update `explainOperationError` return type:

```typescript
explainOperationError: (
    instanceId: string,
    operation: string,
    transport: "local" | "docker_local" | "remote_ssh",
    error: string,
    language?: string,
  ): Promise<{
    message: string;
    summary: string;
    actions: string[];
    structuredActions: GuidanceAction[];
    source: string;
  }> => invoke("explain_operation_error", { ... }),
```

Add precheck API calls:

```typescript
precheckRegistry: (): Promise<PrecheckIssue[]> =>
    invoke("precheck_registry"),
precheckInstance: (instanceId: string): Promise<PrecheckIssue[]> =>
    invoke("precheck_instance", { instanceId }),
```

**Step 3: Commit**

```bash
git add src/lib/types.ts src/lib/api.ts
git commit -m "feat: add GuidanceAction and PrecheckIssue frontend types"
```

---

### Task 8: Update AgentGuidanceItem and App.tsx guidance card

**Files:**
- Modify: `src/App.tsx:71-81` (AgentGuidanceItem interface)
- Modify: `src/App.tsx:1185-1234` (guidance card rendering)

**Step 1: Update AgentGuidanceItem**

In `src/App.tsx`, update the interface:

```typescript
interface AgentGuidanceItem {
  message: string;
  summary: string;
  actions: string[];
  structuredActions?: GuidanceAction[];
  source: string;
  operation: string;
  instanceId: string;
  transport: string;
  rawError: string;
  createdAt: number;
}
```

Import `GuidanceAction` from `./lib/types`.

**Step 2: Upgrade the guidance card rendering**

Replace the existing card section (around lines 1185-1234) with a version that renders structured actions as buttons:

```tsx
{/* Existing text actions */}
{agentGuidance.actions.length > 0 && (
  <ol className="text-sm space-y-1.5 list-decimal pl-5">
    {agentGuidance.actions.map((action, idx) => (
      <li key={`${idx}-${action}`}>{action}</li>
    ))}
  </ol>
)}

{/* Structured action buttons */}
<div className="flex flex-wrap items-center gap-2 pt-1">
  {(agentGuidance.structuredActions ?? []).map((sa, idx) => (
    sa.actionType === "inline_fix" ? (
      <Button
        key={`sa-${idx}`}
        size="sm"
        variant="outline"
        onClick={async () => {
          // Execute inline fix via existing tool intent infrastructure
          // For now, just invoke the clawpal CLI command
          try {
            // TODO: wire to tool intent execution
            showToast(`正在执行: ${sa.label}`, "success");
          } catch (e) {
            showToast(`${sa.label} 失败: ${e}`, "error");
          }
        }}
      >
        {sa.label}
      </Button>
    ) : (
      <Button
        key={`sa-${idx}`}
        size="sm"
        onClick={() => {
          setAgentGuidanceOpen(false);
          setDoctorLaunchByInstance((prev) => ({
            ...prev,
            [agentGuidance.instanceId]: {
              ...agentGuidance,
              rawError: sa.context || agentGuidance.rawError,
            },
          }));
          setInStart(false);
          navigateRoute("doctor");
        }}
      >
        {sa.label}
      </Button>
    )
  ))}
  {/* Fallback buttons when no structured actions */}
  {(!agentGuidance.structuredActions || agentGuidance.structuredActions.length === 0) && (
    <>
      <Button
        size="sm"
        onClick={() => {
          setAgentGuidanceOpen(false);
          setDoctorLaunchByInstance((prev) => ({
            ...prev,
            [agentGuidance.instanceId]: agentGuidance,
          }));
          setInStart(false);
          navigateRoute("doctor");
        }}
      >
        打开 Doctor
      </Button>
    </>
  )}
  <Button
    size="sm"
    variant="outline"
    onClick={() => { setAgentGuidanceOpen(false); setUnreadGuidance(false); }}
  >
    稍后处理
  </Button>
</div>
```

**Step 3: Compile and verify**

Run: `npm run build` (or `pnpm build` / `bun build` depending on project setup)
Expected: Compiles without type errors.

**Step 4: Commit**

```bash
git add src/App.tsx
git commit -m "feat: upgrade guidance card with structured action buttons"
```

---

### Task 9: Add precheck integration in frontend

**Files:**
- Modify: `src/App.tsx` (add precheck on startup)
- Modify: appropriate instance-switching code (add precheck before switch)

**Step 1: Add startup precheck**

In `src/App.tsx`, inside the initial `useEffect` (app startup), add:

```typescript
// Startup precheck: validate registry
api.precheckRegistry().then((issues) => {
  const errors = issues.filter((i) => i.severity === "error");
  if (errors.length > 0) {
    showToast(errors[0].message, "error");
  }
}).catch(() => { /* ignore — precheck failure should not block app */ });
```

**Step 2: Add instance switch precheck**

In the instance switching handler, add precheck before switching:

```typescript
// Before switching to instance
const issues = await api.precheckInstance(instanceId).catch(() => []);
const blocking = issues.filter((i) => i.severity === "error");
if (blocking.length > 0) {
  showToast(blocking[0].message, "error");
  // Don't block — just warn
}
```

**Step 3: Compile and verify**

Run: `npm run build`
Expected: Compiles without errors.

**Step 4: Commit**

```bash
git add src/App.tsx
git commit -m "feat: add precheck calls on app startup and instance switch"
```

---

### Task 10: Extend guidance.ts error pattern matching

**Files:**
- Modify: `src/lib/guidance.ts`

**Step 1: Add new error filter patterns**

Add these functions to `src/lib/guidance.ts`:

```typescript
export function isRegistryCorruptError(errorText: string): boolean {
  const text = errorText.toLowerCase();
  return (
    (text.includes("registry") || text.includes("instances.json"))
    && (text.includes("parse") || text.includes("corrupt") || text.includes("invalid json"))
  );
}

export function isContainerOrphanedError(errorText: string): boolean {
  const text = errorText.toLowerCase();
  return (
    text.includes("no such container")
    || (text.includes("container") && text.includes("not found"))
  );
}
```

These are informational — they don't filter out errors (unlike cooldown/transient), they just help with classification if needed in the future.

**Step 2: Commit**

```bash
git add src/lib/guidance.ts
git commit -m "feat: add registry corrupt and container orphaned error patterns"
```

---

### Task 11: Update operation-fallback prompt schema

**Files:**
- Modify: `prompts/error-guidance/operation-fallback.md`

**Step 1: Update the prompt**

Update the JSON schema in the prompt to include `structured_actions`:

```markdown
> 使用位置：`src-tauri/src/agent_fallback.rs::explain_operation_error`
> 使用时机：业务调用失败后，生成小龙虾的结构化解释与下一步行动建议。

```prompt
You are ClawPal's internal diagnosis assistant.
Given a failed business call, output JSON only:
{"summary":"one-sentence root cause","actions":["step 1","step 2","step 3"],"structuredActions":[{"label":"button text","actionType":"inline_fix|doctor_handoff","tool":"clawpal|openclaw","args":"cli args","invokeType":"read|write","context":"error context for doctor"}]}

Requirements:
1) Use {{language_rule}}
2) Do not output markdown.
3) actions: at most 3, each actionable (plain text descriptions).
4) structuredActions: 1-2 executable button actions. Use "inline_fix" for simple reconnect/refresh commands. Use "doctor_handoff" for complex diagnosis needing Doctor page.
5) For inline_fix: tool must be "clawpal" or "openclaw", args is the CLI subcommand, invokeType is "read" or "write".
6) For doctor_handoff: context should summarize the error for the Doctor agent.
7) Prefer actionable steps through existing ClawPal tools first, then manual fallback.
8) If openclaw-related, you may prioritize:
   - clawpal doctor probe-openclaw
   - openclaw doctor --fix
   - clawpal doctor fix-openclaw-path
9) Even when auto-fix cannot be completed, provide clear next step.
10) New error categories to recognize: AUTH_EXPIRED (401/403/invalid key), REGISTRY_CORRUPT (JSON parse failures), INSTANCE_ORPHANED (container/path missing), TRANSPORT_STALE (SSH/Docker disconnected).

Context:
instance_id={{instance_id}}
transport={{transport}}
operation={{operation}}
error={{error}}
probe={{probe}}
language={{language}}
```
```

**Step 2: Update parse_guidance_json to handle structuredActions**

In `src-tauri/src/agent_fallback.rs`, update `parse_guidance_json` to extract `structuredActions` from agent output:

```rust
fn parse_guidance_json(raw: &str) -> Option<GuidanceBody> {
    for cand in extract_json_objects(raw) {
        let Ok(v) = serde_json::from_str::<Value>(&cand) else {
            continue;
        };
        let Some(summary) = v.get("summary").and_then(Value::as_str) else {
            continue;
        };
        let actions = v
            .get("actions")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
        let structured_actions = v
            .get("structuredActions")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| serde_json::from_value::<GuidanceAction>(item.clone()).ok())
                    .collect::<Vec<GuidanceAction>>()
            })
            .unwrap_or_default();
        return Some(GuidanceBody {
            summary: summary.trim().to_string(),
            actions,
            structured_actions,
        });
    }
    None
}
```

Note: `GuidanceAction` needs `Deserialize` in addition to `Serialize`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuidanceAction {
    // ... fields unchanged
}
```

Add `use serde::Deserialize;` to imports.

**Step 3: Run tests**

Run: `cd src-tauri && cargo test agent_fallback::tests -- --nocapture`
Expected: All PASS

**Step 4: Commit**

```bash
git add prompts/error-guidance/operation-fallback.md src-tauri/src/agent_fallback.rs
git commit -m "feat: update operation-fallback prompt and parser for structuredActions"
```

---

### Task 12: Final integration test and cleanup

**Step 1: Full build check**

Run: `cd src-tauri && cargo test`
Run: `npm run build` (or equivalent frontend build)

Expected: All tests pass, no compile errors.

**Step 2: Manual smoke test checklist**

- [ ] App starts without errors
- [ ] Registry precheck runs on startup (check console)
- [ ] Guidance card appears with structured action buttons when an error occurs
- [ ] "让小龙虾修复" button navigates to Doctor with context
- [ ] "重连 SSH" button (when SSH error) triggers reconnect
- [ ] "稍后处理" button dismisses the card

**Step 3: Final commit**

```bash
git add -A
git commit -m "feat: zeroclaw anomaly fallback integration complete"
```
