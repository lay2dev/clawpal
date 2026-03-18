# Remote Doctor Agent Investigation Design

## Summary

当前远程修复在 `clawpal_server` 路径下依赖 `remote_repair_plan.*` 专用 planner。真实联调已经证明，这条路径对 `primary.config.unreadable` 只会返回 `doctorRediagnose`，不会生成坏 JSON 的诊断或修复步骤。目标是把这类场景切换为由 ClawPal 直接通过标准 gateway `agent` 会话生成步骤，ClawPal 只负责执行、回传结果和循环控制。

## Goals

- 去掉 `remote_repair_plan.request` 作为远程修复默认路径
- 对 `primary.config.unreadable` 场景不再硬编码修复逻辑
- 让 agent 先生成诊断步骤，再基于调查结果生成修复步骤
- 保留现有命令执行器、日志、事件、轮次上限和 stall 检测

## Non-Goals

- 本轮不切到完整 `node.invoke` 工具流
- 本轮不保留 `clawpal_server` 作为默认远程修复协议
- 本轮不重新引入自动 `manage_rescue_bot activate rescue`

## Architecture

远程修复改为三态状态机：

- `diagnose`
- `investigate`
- `repair`

ClawPal 在每轮拿到 rescue diagnosis 后：

- 如果存在 `primary.config.unreadable`，下一轮进入 `investigate`
- 否则进入 `repair`
- 每轮执行完命令后重新诊断，直到健康或达到上限

agent 通过标准 gateway `agent` 方法返回 JSON 计划。ClawPal 继续使用现有 `PlanResponse` / `PlanCommand` 执行链，不再依赖 `remote_repair_plan.*`。

## Agent Prompt Model

### Shared Context

每次请求 agent 时都提供：

- `targetLocation`
- `instanceId`
- `diagnosis`
- `configExcerpt`
- `configExcerptRaw`
- `configParseError`
- `previousResults`

### Diagnose Prompt

用途是获取下一步高层方向。通常只在初始轮次或修复后复诊时使用。

### Investigate Prompt

用于 `primary.config.unreadable` 场景，约束如下：

- 只允许返回诊断命令
- 不允许直接写文件、删文件或覆盖配置
- 必须先要求备份方案
- 目标是解释配置为何不可解析，并收集最小修复所需证据

### Repair Prompt

用于调查完成后的修复阶段，约束如下：

- 必须引用前一轮调查结果
- 写配置前必须先备份原文件
- 变更尽量最小
- 修复后必须要求 JSON 校验和重新诊断

## Execution Model

ClawPal 继续负责：

- 执行命令
- 收集 `stdout/stderr/exitCode`
- 记录日志
- 向 agent 回传 `previousResults`
- 维护轮次和停滞检测

ClawPal 不再硬编码坏 JSON 的具体修法。

## Logging

保留现有 session 日志，新增或调整以下内容：

- `plan_received` 支持 `planKind: investigate`
- `command_result` 记录调查命令结果
- stall 检测要覆盖“连续无效 investigate”场景
- `config_recovery_context` 继续记录 `configExcerptRaw` 是否存在和 parse error

## Error Handling

- 如果 agent 在 investigate 阶段返回写命令，ClawPal 直接拒绝执行并记录协议错误
- 如果 agent 连续多轮只返回空调查或无效调查，触发 stall
- 如果 50 轮内仍未恢复健康，错误中保留最后一次 diagnosis 和最后一步类型

## Testing

### Unit Tests

- `primary.config.unreadable` 时状态机会先进入 `investigate`
- `investigate` prompt 明确只读约束
- `repair` prompt 明确要求引用调查结果

### Live E2E

- 启动 Docker OpenClaw 目标机
- 故意写坏 `openclaw.json`
- 通过真实 gateway `agent` 路径运行远程修复
- 断言配置恢复为合法 JSON，诊断变为健康

## Migration

- `ClawpalServer` 从默认协议移除
- `remote_repair_plan.*` 路径降级为兼容分支或后续删除
- 默认远程修复协议改为 agent 驱动
