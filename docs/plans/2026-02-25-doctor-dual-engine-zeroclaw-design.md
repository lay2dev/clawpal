# ClawPal Doctor 双引擎设计（OpenClaw + zeroclaw）

日期：2026-02-25  
状态：已确认（可进入实现计划）  
作者：Codex + 用户共创

## 1. 背景与目标

当前 Doctor 具备完整的对话与工具权限控制链路（流式 chat、invoke 审批、local/remote 路由）。  
本设计目标是在不破坏现有 Doctor UX 与安全边界的前提下，引入 zeroclaw 作为可选智能引擎，实现双引擎并行：

- 引擎 A：OpenClaw Agent（现有实现）
- 引擎 B：zeroclaw sidecar（新增实现）

核心原则：

1. UI 不分叉：Doctor 继续只有一套交互与审批流程。
2. 执行权不外放：所有命令执行仍由 ClawPal 节点执行，runtime 仅负责决策与对话。
3. 可观测可回退：失败可定位、可切换引擎、可保留运行证据。

## 2. 方案选型

已评估方案：

1. UI 侧双路分发
2. 后端统一 Doctor Runtime（采用）
3. 会话级桥接代理

选择方案 2（后端统一 Runtime）的原因：

- 前端改动最小，避免业务分支扩散。
- 可复用现有事件协议与权限模型。
- 安装编排与 Doctor 后续可共享同一 runtime 协议，便于架构收敛。

## 3. 架构设计

### 3.1 统一 Runtime 接口

新增 `DoctorRuntime` 抽象层，提供一致的生命周期与消息能力：

- `connect(...)`
- `disconnect(...)`
- `start_diagnosis(...)`
- `send_message(...)`
- `on_chat_delta/on_chat_final`
- `on_invoke_requested`
- `on_invoke_result/on_invoke_error`

实现体：

- `OpenClawDoctorRuntime`：复用现有 `NodeClient + BridgeClient`。
- `ZeroClawDoctorRuntime`：使用 sidecar（zeroclaw）进行决策，输出与现有一致的事件。

### 3.2 前端边界

Doctor 前端仅新增会话参数：

- `engine: "openclaw" | "zeroclaw"`

默认值：

- 默认 `zeroclaw`

事件协议保持不变：

- `doctor:chat-delta`
- `doctor:chat-final`
- `doctor:invoke`
- `doctor:invoke-result`
- `doctor:error`

因此 `DoctorChat` 与审批组件无需重写。

## 4. 状态机与数据流

统一会话状态：

- `idle -> connecting -> active -> closing -> idle`

主流程：

1. `doctor_start_diagnosis(engine, target, agentId, sessionKey)` 创建会话并激活 runtime
2. `doctor_send_message(...)` 将用户输入路由到当前 runtime
3. runtime 输出 chat 或 invoke 事件
4. 用户审批 invoke 后，命令由 ClawPal 节点执行（local/ssh）
5. 执行结果回传 runtime，驱动下一步决策
6. 断线/退出统一回收状态

关键约束：

- runtime 不直接执行系统命令。
- 审批边界保持在 Doctor（读/写批准、全自动开关）层。
- 目标执行位置依旧由现有 target 路由控制。

## 5. 错误处理与降级

### 5.1 错误分类

- `CONFIG_MISSING`：缺 provider key / 未完成引擎配置
- `MODEL_UNAVAILABLE`：模型不存在或不可用（如 404）
- `RUNTIME_UNREACHABLE`：sidecar 缺失或不可执行
- `DECISION_INVALID`：返回协议无效
- `TARGET_UNREACHABLE`：目标执行链路失败（local/ssh）

### 5.2 降级策略

- 默认：不自动切换引擎，只提示“一键切换到备用引擎”。
- 可选：`autoFailover`（默认关闭），开启后允许自动切换一次。
- 任何错误都必须带 `engine` 维度记录日志，避免混淆。

### 5.3 用户可见反馈

Doctor 顶部展示：

- 当前引擎
- 当前目标
- 最近一次错误与恢复建议

恢复建议应可执行（如“去访问能力档案补 key”），而非纯描述。

### 5.4 密钥安全

- 对话与日志输出统一掩码疑似密钥。
- 发现疑似泄露时显示“请尽快轮换密钥”的风险提示。

## 6. 分期实施

### Phase 1：最小可用（先打通）

范围：

1. 抽象 `DoctorRuntime`，将现有 OpenClaw 路径适配为 runtime
2. 新增 zeroclaw runtime（chat + invoke 协议）
3. `doctor_start_diagnosis/doctor_send_message` 增加 `engine` 参数
4. Doctor UI 增加引擎选择器（默认 zeroclaw）

验收：

1. 同目标可切换两引擎并对话
2. 两引擎都可触发 invoke，审批后会话继续
3. 错误与日志能区分 `engine`

### Phase 2：稳定性与可观测

范围：

1. 全量错误分类码落地
2. `autoFailover` 开关落地
3. 展示当前 provider/model 与来源信息

验收：

1. 缺 key / 模型不可用 / sidecar 缺失均可清晰恢复
2. 开启 `autoFailover` 时可自动切换一次并继续

### Phase 3：安装编排收敛

范围：

1. 安装编排与 Doctor 共用 runtime 协议
2. 成功路径经验库复用（保留 top N，避免无限增长）
3. ssh/wsl/docker 差异收敛到 runtime 决策层

验收：

1. “安装到 docker/remote”可由 runtime 主导推进
2. 经验复用可减少重复探测与失败重试

## 7. 风险与约束

1. 双引擎并存阶段需要严格日志隔离，防止错误归因混乱。
2. zeroclaw 模型与 provider 兼容矩阵可能波动，必须做显式可见。
3. 全自动能力必须绑定审批策略，不能绕过现有安全边界。

## 8. 完成定义（Design DoD）

以下项满足即视为设计完成：

1. 已明确统一 runtime 抽象与双实现边界
2. 已明确状态机、工具执行权限与安全边界
3. 已明确错误分类、降级策略与用户反馈
4. 已明确分期范围与可验证验收项
5. 已获得用户确认并可进入 implementation planning
