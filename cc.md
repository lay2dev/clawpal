# Code Review Notes (Claude → Codex)

Last updated: 2026-02-28

This file contains review findings and action items. Codex should check this file periodically and work through the items.

---

## Context

重构目标：**所有用户侧异常都应由小龙虾（zeroclaw）兜底**。

当前架构有两条小龙虾介入路径：
- **路径 A（自动 guidance）**：`dispatch()` → `explainAndWrapError()` → 弹出建议面板
- **路径 B（Doctor 诊断）**：用户手动打开 Doctor → 交互式诊断

`dispatch()` 在 `use-api.ts:246-296` 对 local/docker/remote 三种传输都包裹了 `explainAndWrapError`，覆盖约 60+ 个业务操作。但以下缺口导致小龙虾无法兜底。

---

## Outstanding Issues

### P0: App.tsx 直接调用 api.* 绕过 dispatch()

实例生命周期管理（连接、断开、删除、切换）在 App.tsx 级别直接调 `api.*`，不经过 `dispatch()` 包裹，失败时小龙虾完全不知道。这是用户最高频的操作路径。

| 操作 | 代码位置 | 当前处理 |
|------|---------|---------|
| `api.listSshHosts()` | App.tsx:214 | `console.error` |
| `api.listRegisteredInstances()` | App.tsx:218 | 静默失败，空列表 |
| `api.connectDockerInstance()` | App.tsx:245,257 | 可能无提示 |
| `api.sshConnect()` / `sshConnectWithPassphrase()` | App.tsx:490,497 | 弹密码框或 toast |
| `api.ensureAccessProfile()` | App.tsx:382 | `console.error` |
| `api.deleteSshHost()` | App.tsx:1000 | 未知 |
| `api.deleteRegisteredInstance()` | App.tsx:271 | 未知 |
| `api.setActiveOpenclawHome()` | App.tsx:604,609 | `.catch(() => {})` |
| `api.remoteListChannelsMinimal()` | App.tsx:692 | 缓存加载失败 |
| `api.remoteGetWatchdogStatus()` | App.tsx:734 | 状态加载失败 |

### P0: SSH 首次连接失败无 guidance

SSH 连接流程（App.tsx:490-500）在失败时只弹密码框或 showToast，不触发小龙虾分析。首次使用+网络不稳定是用户最容易碰到异常的场景。

### P1: 静默吞错 `.catch(() => {})`

以下操作失败时用户完全不知道，小龙虾也不介入：

| 操作 | 位置 |
|------|------|
| Cron jobs/runs 加载 | Cron.tsx:141,143 |
| Watchdog 状态 | Cron.tsx:142 |
| Config 读取 | Cook.tsx:106 |
| Queued commands count | Home.tsx:99 |
| 日志内容加载 | Doctor.tsx:258 |
| Recipes 列表 | Recipes.tsx:31 |
| SSH 状态轮询 | App.tsx:304,314,315 |

注意：这些操作经过 `dispatch()`，`explainAndWrapError` 会在 throw 前 emit guidance 事件，但 throttle (90s/签名) 意味着轮询场景下只有首次失败触发 guidance。如果用户没注意到首次弹出的面板，后续完全无感知。

### P2: toast + guidance 双信号割裂

页面组件用 `.catch((e) => showToast(String(e), "error"))` 截获了错误后自己显示 toast，同时 `explainAndWrapError` 又 emit 了 guidance 面板。用户同时看到两个信息源，体验割裂。

涉及：Home.tsx (agent/model 操作)、Channels.tsx (binding 操作)、History.tsx、SessionAnalysisPanel.tsx、Doctor.tsx (backup 操作)。

### P2: 小龙虾自身启动失败无二级兜底

当 zeroclaw 二进制缺失、API key 未配置、模型不可用时，`rules_fallback()` 只覆盖 3 种硬编码模式（ownerDisplay、openclaw missing、SSH connection）。其他场景下 guidance 请求本身失败，用户只看到原始错误字符串。

---

## Next Actions (for Codex)

### Action 1: App.tsx 生命周期操作接入 guidance

在 App.tsx 中为所有直接调用 `api.*` 的操作加上 guidance 包裹。有两种方案，选其一：

**方案 A（推荐）**：在 App.tsx 中创建一个轻量 `withGuidance` 包裹函数，复用 `api.explainOperationError` 的逻辑：

```typescript
// App.tsx 或提取到 lib/guidance.ts
async function withGuidance<T>(
  fn: () => Promise<T>,
  method: string,
  instanceId: string,
): Promise<T> {
  try {
    return await fn();
  } catch (error) {
    // emit guidance event (same logic as explainAndWrapError in use-api.ts)
    try {
      const guidance = await api.explainOperationError(instanceId, method, transport, String(error), language);
      window.dispatchEvent(new CustomEvent("clawpal:agent-guidance", { detail: { ...guidance, operation: method, instanceId } }));
    } catch { /* guidance itself failed, ignore */ }
    throw error;
  }
}
```

然后包裹关键调用：
```typescript
// 替换：
api.sshConnect(hostId).catch(e => showToast(String(e), "error"))
// 为：
withGuidance(() => api.sshConnect(hostId), "sshConnect", instanceId).catch(e => showToast(String(e), "error"))
```

**方案 B**：将生命周期操作也移入 `useApi()` 返回的方法集，让 `dispatch()` 自动包裹。但这需要改 `useApi` 接口，改动范围更大。

优先覆盖这些操作（按用户影响排序）：
1. `api.sshConnect()` / `api.sshConnectWithPassphrase()` — SSH 首次连接
2. `api.connectDockerInstance()` — Docker 连接
3. `api.listRegisteredInstances()` — 实例列表
4. `api.listSshHosts()` — SSH 主机列表
5. `api.deleteRegisteredInstance()` / `api.deleteSshHost()` — 删除操作

验证：`npx tsc --noEmit` 通过。手动测试：断开 SSH 后重连，应看到小龙虾 guidance 面板弹出。

### Action 2: 静默吞错改为"通知小龙虾但不弹 toast"

将 `.catch(() => {})` 改为在失败时静默 emit guidance 事件（不弹 toast），让小龙虾面板至少有机会出现：

```typescript
// 替换：
ua.listCronJobs().then(setJobs).catch(() => {});
// 为：
ua.listCronJobs().then(setJobs).catch(() => {
  // guidance event already emitted by dispatch() before this catch
  // nothing extra needed — just don't swallow silently if we want user awareness
});
```

实际上 `dispatch()` 内的 `explainAndWrapError` 已经在 throw 之前 emit 了 guidance 事件。所以问题不在于 `.catch(() => {})`（guidance 已经发出），而在于：
- throttle 90s 内相同签名不重复 emit — 这是对的，不需要改
- 用户可能没注意到 guidance 面板 — 这是 UX 问题

**改进方向**：当 guidance 面板有未读消息时，在侧边栏小龙虾图标上加一个红点/badge，提醒用户查看。这样即使 toast 消失了，用户仍然知道有建议等待处理。

实现：在 `App.tsx` 的 guidance 事件监听处，增加一个 `unreadGuidance` 状态，在小龙虾按钮上显示 badge。用户打开 guidance 面板后清除 badge。

验证：`npx tsc --noEmit` 通过。

### Action 3: 统一 toast + guidance 信号

目标：避免用户同时看到 toast 错误消息和 guidance 面板两个信号源。

原则：**如果 guidance 面板已弹出，页面组件不再显示 error toast**。

实现思路：`explainAndWrapError` 在 emit guidance 事件时，在 error 对象上标记 `_guidanceEmitted = true`。页面组件的 `.catch()` 检查这个标记，有标记则不弹 toast：

```typescript
// use-api.ts explainAndWrapError 中：
const wrapped = new Error(message);
(wrapped as any)._guidanceEmitted = true;
throw wrapped;

// 页面组件中：
.catch((e) => {
  if (!(e as any)?._guidanceEmitted) {
    showToast(String(e), "error");
  }
});
```

涉及文件：use-api.ts, Home.tsx, Channels.tsx, Doctor.tsx, SessionAnalysisPanel.tsx。

验证：`npx tsc --noEmit` 通过。

---

## Execution History

| Item | Status | Notes |
|------|--------|-------|
| SSH session reuse pool (P0) | **Done** | `46b2509` — persistent handle per host |
| Login shell unification | **Done** | `0f3c88f`, `0235e38` |
| Frontend perf (lazy load + transitions) | **Done** | `9e418a2`, `a15533a` |
| SSH error UX | **Done** | `ba08aed`, `a7864e3` |
| Remote domain migration (E2-E6) | **Done** | See cc-ssh-refactor-v1.md |
| commands.rs split | **Done** | mod.rs 9115 → 6005 lines |
