# ClawPal Remote Doctor Service Design

日期：2026-03-17

## 1. 目标

为 ClawPal Doctor 页新增一条“远程 doctor 修复”路径。该路径通过 OpenClaw gateway websocket 协议连接远程 doctor agent，由 agent 生成检测/修复流程，ClawPal 本地执行命令并将结果回传，直到检测结果无问题或达到最大轮次。

本期同时保留现有“本地修复”路径，两条修复方式都可以作用于 local 或 ssh OpenClaw 实例。修复方式与实例位置解耦。

## 2. 用户行为

用户进入 Doctor 页，看到两个修复入口：

- 本地修复
- 远程 Doctor 修复

两者共享同一份诊断结果，但执行策略不同：

- 本地修复：继续复用当前 `doctor assistant` / `rescue` 逻辑
- 远程 Doctor 修复：通过 gateway doctor agent 获取 plan 并循环执行

## 3. 运行闭环

远程 Doctor 修复严格按以下顺序运行：

1. ClawPal 向远程 doctor agent 请求“检测 OpenClaw 问题的流程”
2. ClawPal 执行检测流程中的命令
3. ClawPal 将检测结果回传给远程 doctor agent，并请求“修复问题的流程”
4. ClawPal 执行修复流程中的命令
5. ClawPal 再次请求“检测 OpenClaw 问题的流程”
6. 若检测结果仍有问题，则继续下一轮
7. 若检测结果无问题，则结束并标记成功
8. 累计轮次超过 50 次则报错

这里的“轮次”定义为一次 plan 请求与执行完成。完整会话会在 detect / repair 两种 plan 之间交替推进。

## 4. 协议边界

### 4.1 传输层

- 使用 `openclaw-gateway-client` 建立新的 websocket client
- 不复用现有 `bridge_client`
- 为 remote doctor 会话单独维护连接、请求和事件订阅

### 4.2 目标标识

每次发给 doctor agent 的请求必须显式说明目标是本地还是远程 OpenClaw：

- `targetLocation: "local_openclaw" | "remote_openclaw"`
- `instanceId`
- `hostId`（如果是 ssh 实例）

注意：这里的“本地/远程”描述的是被修复目标 OpenClaw 的位置，不代表所选修复方式。

### 4.3 Plan 结构

第一版不引入完整 DSL，而采用更贴近现有执行器的结构化命令 plan：

```json
{
  "planId": "doctor_plan_xxx",
  "planKind": "detect",
  "summary": "Check gateway and config health",
  "done": false,
  "commands": [
    {
      "argv": ["openclaw", "doctor", "--json"],
      "timeoutSec": 20,
      "purpose": "collect diagnosis",
      "continueOnFailure": false
    }
  ]
}
```

约束：

- 第一版只支持命令数组
- 每条命令必须包含 `argv`
- `timeoutSec` 缺省时由 ClawPal 写入安全默认值
- `done: true` 仅表示 agent 建议停止，最终成功仍由检测结果决定

## 5. 状态机

远程 Doctor 会话状态：

- `idle`
- `planning_detect`
- `executing_detect`
- `reporting_detect`
- `planning_repair`
- `executing_repair`
- `reporting_repair`
- `completed`
- `failed`

状态切换规则：

- detect 执行完成后一定进入 repair 请求，除非 detect 结果已明确无问题
- repair 执行完成后一定回到 detect 请求
- 任一阶段出错直接进入 `failed`
- 轮次超过 50 次进入 `failed`

## 6. 成功判定

最终成功只取决于最新一次检测结果：

- 检测结果无问题 => `completed`
- doctor agent 声称 success，但检测结果仍异常 => 继续循环
- doctor agent 不再返回修复 plan，但检测结果仍异常 => 失败

## 7. 日志与审计

必须记录完整远程 Doctor 闭环日志，包括：

- session id
- 当前实例 id / host id
- 修复方式：`remote_doctor`
- target location：`local_openclaw` 或 `remote_openclaw`
- 当前阶段：detect / repair
- 当前轮次
- 发给 agent 的请求摘要
- agent 返回的 plan 摘要
- 每条命令的 `argv`
- 每条命令的退出码、耗时、stdout、stderr、是否超时
- agent 回传摘要
- 最终结束原因：success / exhausted / planner_error / execution_error

实时进度事件沿用 Doctor 页现有模式，新增专用 event，供页面展示当前轮次和最近一条命令。

## 8. UI 方案

Doctor 页从单一修复按钮改为两个入口：

- `本地修复`
- `远程 Doctor 修复`

规则：

- 两个按钮都基于当前实例上下文执行
- 若当前实例是 ssh，二者都要求 SSH 已连接
- 本地修复走现有 `repairDoctorAssistant`
- 远程 Doctor 修复走新的 `startRemoteDoctorRepair`

页面显示：

- 当前运行中的修复方式
- 当前轮次
- 当前阶段
- 最新进度行
- 完成/失败结果

## 9. 非目标

本期不做：

- 远程 Doctor 修复的取消/恢复
- plan 可视化编辑
- 任意 shell 脚本上传执行
- 同时并发多个远程 Doctor 会话
- 对现有本地修复链路做行为重构
