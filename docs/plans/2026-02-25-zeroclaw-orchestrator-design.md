# ZeroClaw Orchestrator Design (ClawPal)

## 当前任务目标
- 将 ClawPal 从“硬编码流程驱动”升级为“智能编排驱动”。
- 通过内置 `zeroclaw` sidecar 负责计划与决策，ClawPal 负责执行、安全、可观测。

## 预期验收项
- 安装与 Doctor 保留原有用户入口，不增加用户认知负担。
- 默认半自动执行（低风险自动，高风险确认），支持用户切换全自动。
- 支持按实例自适应发现访问链路（local/ssh/docker/wsl），并本地缓存成功链路。
- 任何密钥明文读取/展示触发安全告警，并提示轮换。

## 完成后状态
- 状态：待实现（本设计文档已完成）

---

## 1. 产品定位与边界

### 1.1 目标定位
- `安装`、`Doctor` 等业务入口保持不变。
- 新增编排内核负责“如何做”，业务页面负责“做什么”。
- 新增编排详情视图（高级控制台）用于步骤、风险、审计可视化。

### 1.2 责任分层
- ZeroClaw sidecar：产出结构化计划（Plan）与重规划（Replan）。
- ClawPal Orchestrator：执行计划步骤、风控门禁、权限确认、失败恢复。
- ClawPal Data Plane：稳定硬编码能力（配置读写、实例状态、受限命令、日志采集）。

### 1.3 非目标（v1）
- 不要求用户部署 zeroclaw。
- 不将 zeroclaw 深度链接进 ClawPal 主进程（先 sidecar）。
- 不一次性替换全部旧流程。

---

## 2. 集成形态

### 2.1 选择：内置 sidecar 二进制
- 在 ClawPal 安装包中内置 `zeroclaw` 可执行文件。
- ClawPal 启动时按需拉起 sidecar（守护/短时均可），并执行健康检查。

### 2.2 选择原因
- 进程隔离，sidecar 崩溃不拖垮主后端。
- 版本可控，升级回滚清晰。
- 为未来替换/并行多个智能引擎保留接口。

---

## 3. 执行模型（默认半自动）

### 3.1 运行模式
- `Semi-Auto`（默认）：
  - `low` 风险步骤自动执行
  - `medium/high` 风险步骤需用户确认
- `Full-Auto`（用户可选）：
  - 自动执行全部可执行步骤
  - 仍受硬性安全策略限制

### 3.2 风险分级
- `low`：只读探测、幂等校验
- `medium`：写配置、重启、实例切换
- `high`：删除/覆盖、迁移、网络暴露、凭据写入

### 3.3 硬门禁（两种模式都生效）
- 禁止危险命令与无范围 destructive 操作。
- 禁止未允许命令前缀执行。
- `high` 风险步骤必须有 `rollback_hint`。
- 密钥相关步骤必须脱敏记录并进入审计。

---

## 4. 计划协议（Planner-Executor）

### 4.1 `ActionPlan`（sidecar -> ClawPal）
- `plan_id`
- `task_type`: `install | doctor | repair`
- `target`: `local | docker | remote | wsl`
- `steps[]`:
  - `id`, `title`
  - `executor`: `tool | shell`
  - `payload`
  - `risk`: `low | medium | high`
  - `requires_confirm`
  - `rollback_hint`（high 必填）
- `success_criteria[]`

### 4.2 `ActionResult`（ClawPal -> sidecar）
- `plan_id`, `step_id`
- `ok`
- `summary`
- `artifacts`（结构化输出）
- `security_events[]`（含 `secret_exposed`）

### 4.3 `Tool Contract`（ClawPal Data Plane）
- `read_config`, `write_config`
- `run_command_restricted`
- `get_instance_status`
- `switch_instance`
- `read_logs`
- `verify_health`

---

## 5. AAD：自适应访问发现（核心能力）

### 5.1 目标
- 在 `local/ssh/docker/wsl` 场景下自动摸索可用访问链路。
- 将成功链路缓存为实例能力画像，后续优先复用。

### 5.2 能力画像（仅本地）
- `instance_id`
- `transport`
- `probes[]`（尝试记录）
- `working_chain`（成功链路）
- `env_contract`（必要环境变量）
- `verified_at`, `ttl`

### 5.3 运行策略
- 优先走 `working_chain`。
- 失败自动回退到重探测。
- 新成功链路覆盖旧画像并写审计。

### 5.4 探测层级
1. 可执行文件定位（PATH/常见路径）
2. `--version` 可用性
3. 配置/状态目录可读性
4. `status/config get` 冒烟
5. 任务专用校验（install/doctor/chat）

---

## 6. 安全设计

### 6.1 密钥暴露告警（强制）
- 检测到密钥明文读取/展示：
  - 触发 `security_alert`
  - UI 高优先级告警
  - 明确提示“立即轮换密钥”
  - 后续日志自动打码

### 6.2 日志策略
- 所有计划执行日志默认脱敏。
- 保留结构化审计事件：谁、何时、执行了什么、结果如何。

---

## 7. 迁移策略（渐进式）

### M1（访问层）
- 引入 AAD 与 capability profile。
- 先替换连接与命令发现，不改业务入口。

### M2（安装流程）
- Install 改为 plan-driven。
- 保留旧安装逻辑作为 fallback。

### M3（Doctor 流程）
- Doctor 改为 plan-driven。
- 保留静态快速检查作为兜底。

### M4（统一编排中心）
- 汇总任务执行状态、风险与审计。
- 提供半自动/全自动模式切换与可视化控制。

---

## 8. 用户体验原则
- 业务入口不变，编排细节默认隐藏。
- 普通用户以“任务完成”为目标，高级用户可进入编排详情定位问题。
- 所有失败都应带“下一步建议”（重试/跳过/重规划）。

---

## 9. 风险与缓解
- 风险：Agent 规划不稳定。
  - 缓解：Orchestrator 严格执行协议校验与硬门禁。
- 风险：全自动误操作。
  - 缓解：默认半自动，且高风险门禁不可绕过。
- 风险：环境差异导致探测抖动。
  - 缓解：capability profile + TTL + 有证据重探测。

