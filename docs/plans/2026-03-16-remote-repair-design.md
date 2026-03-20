# ClawPal Remote Repair Design

日期：2026-03-16

## 1. 目标

为远程 OpenClaw 实例提供受控的远程修复能力，链路如下：

1. ClawPal 检测到远程实例 A 异常
2. 用户在 ClawPal 中点击“修复”
3. ClawPal 将诊断结果发送给开放修复 bot B
4. B 返回结构化 repair plan
5. ClawPal 在本地校验 plan 安全性
6. ClawPal 通过 SSH 到 A 执行 plan
7. ClawPal 将执行结果回传给 B
8. ClawPal 再次对 A 运行 Doctor
9. 若仍异常，则进入下一轮，直到修复成功或达到停止条件

本设计要求：

- B 只能产出受控 DSL，不允许直接执行
- ClawPal 持有 SSH 凭据并保留最终执行否决权
- Doctor 复检结果是唯一成功判定来源
- 所有执行步骤均可审计、可回显、可中断

## 2. 非目标

本期明确不做：

- 任意 shell 脚本/bundle 上传执行
- 无限制的任意命令透传
- 后台长期自愈 daemon
- 跨实例并发修复编排
- 无用户确认的高风险自动执行

## 3. 方案对比

### 3.1 薄代理方案

B 只返回高层修复建议，ClawPal 本地映射为 SSH 操作。

优点：

- 本地安全边界最清晰
- B 无需理解主机细节

缺点：

- ClawPal 需要维护越来越多的动作映射
- 扩展新修复场景时双端耦合更重

### 3.2 结构化 DSL 方案

B 返回结构化 repair DSL，ClawPal 负责校验、执行、回传、复检。

优点：

- 扩展性最佳
- B 可以按执行结果逐轮调整策略
- ClawPal 角色稳定为安全执行层

缺点：

- 需要设计 DSL、状态机、策略校验和审计结构

### 3.3 远端脚本方案

B 返回脚本或 bundle，ClawPal 上传到 A 并执行。

优点：

- 表达复杂修复逻辑简单

缺点：

- 安全性和审计性最差
- 与受控动作约束冲突

### 3.4 结论

采用“结构化 DSL + 本地执行器 + 多轮 Doctor 复检”方案。

## 4. 核心架构

### 4.1 角色边界

- A：被修复目标，即用户自己的远程 OpenClaw
- B：开放 repair bot，只负责出 plan 和根据结果调整下一轮策略
- ClawPal：协调器、执行器、策略裁决者、审计落点

### 4.2 真相源

- “是否仍有问题”由 ClawPal 对 A 运行的 Doctor 结果判定
- B 的 `stop` 或“修复成功”只能视为建议，不是最终成功信号

### 4.3 运行闭环

1. ClawPal 对 A 运行 Doctor，得到 `diagnosis`
2. ClawPal 组装 `diagnosis + host facts + prior rounds` 发给 B
3. B 返回一轮 `repair plan`
4. ClawPal 执行本地 `policy validation`
5. 校验通过后通过 SSH 在 A 上逐步执行
6. ClawPal 汇总每一步 `step result`
7. ClawPal 将结果回传给 B
8. ClawPal 对 A 再次运行 Doctor
9. 若未恢复则下一轮；否则结束

## 5. Repair DSL

### 5.1 最小动作集合

第一版仅支持以下动作：

- `read_file`
- `write_file`
- `run_command`
- `restart_service`
- `collect_logs`
- `health_check`
- `stop`

### 5.2 Plan 结构

```json
{
  "planId": "rp_20260316_xxx",
  "round": 1,
  "goal": "restore gateway health",
  "summary": "Restart gateway and verify readiness",
  "steps": [
    {
      "id": "step_1",
      "type": "run_command",
      "command": ["systemctl", "restart", "openclaw"],
      "cwd": "/home/ubuntu",
      "timeoutSec": 20,
      "allowlistTag": "service_control",
      "onFailure": "continue"
    },
    {
      "id": "step_2",
      "type": "health_check",
      "check": "doctor_gateway_ready",
      "onFailure": "stop_round"
    }
  ],
  "stopPolicy": {
    "maxRounds": 5,
    "stopOnUnsafeAction": true,
    "stopOnRepeatedFailure": 2
  }
}
```

### 5.3 设计原则

- 不用自然语言描述执行意图
- 每一步必须是结构化字段
- `run_command` 必须附带 `allowlistTag`
- 默认面向幂等或可安全重试动作

## 6. 状态机

修复会话状态：

- `idle`
- `diagnosing`
- `planning`
- `validating_plan`
- `executing`
- `reporting`
- `rechecking`
- `completed`
- `blocked`
- `failed`

说明：

- `blocked` 表示命中安全策略或等待用户确认
- `failed` 表示本轮或会话已不可继续
- `completed` 仅在 Doctor 复检健康时成立

## 7. 安全边界

### 7.1 凭据与执行权

- SSH 凭据只保存在 ClawPal 本地
- B 不直接接触 A，也不直接执行命令
- ClawPal 对每一步拥有最终执行否决权

### 7.2 本地策略校验

在 `validating_plan` 阶段执行以下检查：

- 动作类型必须在 DSL 白名单内
- `run_command` 必须命中命令 allowlist
- `write_file` 只能写允许目录
- 禁止写敏感路径，例如 SSH key、系统认证配置、shell profile
- 每一步必须声明超时或使用默认超时
- 整轮存在最大步数、最大轮次、重复失败阈值

### 7.3 第一版建议

第一版进一步收紧：

- `run_command` 不直接执行任意 argv
- `allowlistTag` 需要映射到 ClawPal 内置模板或受控前缀
- 默认展示 plan 摘要后由用户确认执行

## 8. 数据模型

### 8.1 修复会话

```ts
interface RemoteRepairSession {
  id: string;
  instanceId: string;
  startedAt: string;
  status: "idle" | "running" | "completed" | "blocked" | "failed";
  currentRound: number;
  lastDiagnosisId?: string;
  lastPlanId?: string;
}
```

### 8.2 Repair Plan

```ts
interface RemoteRepairPlan {
  planId: string;
  round: number;
  goal: string;
  summary: string;
  steps: RemoteRepairStep[];
  stopPolicy: {
    maxRounds: number;
    stopOnUnsafeAction: boolean;
    stopOnRepeatedFailure: number;
  };
}
```

### 8.3 Repair Step

```ts
interface RemoteRepairStep {
  id: string;
  type:
    | "read_file"
    | "write_file"
    | "run_command"
    | "restart_service"
    | "collect_logs"
    | "health_check"
    | "stop";
  allowlistTag?: string;
  command?: string[];
  path?: string;
  content?: string;
  timeoutSec?: number;
  onFailure: "continue" | "stop_round" | "stop_session";
}
```

### 8.4 Step Result

```ts
interface RemoteRepairStepResult {
  stepId: string;
  status: "passed" | "failed" | "blocked" | "skipped";
  startedAt: string;
  finishedAt: string;
  exitCode?: number;
  stdoutPreview?: string;
  stderrPreview?: string;
  changedFiles?: string[];
  message: string;
}
```

## 9. 与现有代码的落点

### 9.1 前端

- `src/pages/Doctor.tsx`
  - 增加“远程修复”入口、修复轮次状态、时间线
- `src/lib/api.ts`
  - 增加启动、轮询、取消远程修复 API
- `src/lib/types.ts`
  - 新增 repair DSL / session / result 类型
- 新增 UI 组件
  - `RemoteRepairTimeline`
  - `RemoteRepairPlanDialog`
  - `RemoteRepairSessionBanner`

### 9.2 Tauri / Rust

- `src-tauri/src/commands/doctor_assistant.rs`
  - 抽出可复用的多轮修复 orchestrator 经验
- `src-tauri/src/commands/doctor.rs`
  - 继续作为 Doctor 真相源
- `src-tauri/src/ssh.rs`
  - 复用 SSH 执行能力并增加 DSL 执行适配
- 新增目录 `src-tauri/src/remote_repair/`
  - `types.rs`
  - `planner_client.rs`
  - `executor.rs`
  - `policy.rs`
  - `session.rs`
  - `orchestrator.rs`

## 10. UI 交互

第一版交互流程：

1. Doctor 检测到远程实例异常
2. 展示“请求修复计划”
3. 返回后展示计划摘要、动作数、风险标签
4. 用户确认后执行本轮
5. 执行中展示步骤级进度和输出摘要
6. 每轮结束自动重新 Doctor
7. 成功则显示“已恢复”
8. 若 blocked/failed，则展示阻塞原因和最近一步输出

第一版不做完全静默自动修复。

## 11. 错误模型

需要清晰区分以下失败态：

- `planning_failed`
  - B 未返回合法 plan 或通信失败
- `policy_blocked`
  - 本地安全策略拒绝执行
- `execution_failed`
  - SSH 执行失败、超时、文件写入失败
- `diagnosis_still_failing`
  - 执行完成但 Doctor 仍异常
- `session_exhausted`
  - 达到最大轮次或重复失败上限
- `cancelled`
  - 用户取消

要求：

- 所有失败都需带轮次、步骤、最后原因
- 仅当 Doctor 复检通过时标记成功

## 12. 测试策略

### 12.1 Rust 单元测试

- DSL 解析
- policy 校验
- allowlist 拦截
- stopPolicy 触发
- step result 汇总

### 12.2 Rust 集成测试

- mock planner 返回多轮 plan
- mock SSH 结果
- 验证状态机轮转和结束条件

### 12.3 前端测试

- Doctor 页远程修复入口显示条件
- 计划预览与确认
- 执行进度展示
- blocked / failed / completed 三态 UI

## 13. 推荐实施顺序

1. 先实现本地 orchestrator 和 DSL/policy，不接真实 B
2. 使用 mock planner 跑通多轮 loop
3. 接入真实 B 的 plan API
4. 接入 Doctor 页面 UI 和审计展示
5. 补充多轮失败、取消、恢复边界测试

## 14. 验收标准

- 远程实例异常时可从 Doctor 页面进入修复流程
- ClawPal 能向 B 请求并接收结构化 repair plan
- plan 在本地执行前经过策略校验
- ClawPal 可通过 SSH 在 A 上执行 plan 并回传结果
- 至少支持多轮修复直到 Doctor 健康或达到停止条件
- blocked/failed 原因对用户可见
- 所有步骤存在本地审计记录
