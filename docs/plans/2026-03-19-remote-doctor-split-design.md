# Remote Doctor Module Split Design

日期：2026-03-19

## 1. 目标

将 [`src-tauri/src/remote_doctor.rs`](/Users/zz/clawpal/src-tauri/src/remote_doctor.rs) 从单文件重构为目录模块，降低文件长度和认知负担，同时允许内部命名与模块边界优化，但不改变外部行为、Tauri command 签名、前端事件名或协议语义。

本次工作的核心是“结构重组优先，行为保持稳定”。重构完成后，`start_remote_doctor_repair` 的调用方式、远程修复流程、日志格式、fallback 顺序和现有测试语义都应保持一致。

## 2. 当前问题

[`src-tauri/src/remote_doctor.rs`](/Users/zz/clawpal/src-tauri/src/remote_doctor.rs) 目前约 4525 行，混杂了以下多种职责：

- 协议和结果类型定义
- gateway 配置与 device identity 持久化
- session 日志和进度事件
- target config 读写与 rescue 诊断
- agent prompt 构造与解析
- plan 请求、命令校验与执行
- 三条 repair loop 与 protocol fallback 编排
- 单元测试、集成测试和 live e2e 测试

这导致几个直接问题：

- 很难定位某类逻辑的唯一落点
- 内部 helper 命名越来越泛，职责感弱
- 测试与实现耦在同一巨型文件内，阅读和维护成本高
- 后续继续新增 protocol 或 plan 变种时，冲突概率很高

## 3. 设计原则

- 不修改对外入口：继续从 `crate::remote_doctor::start_remote_doctor_repair` 导出
- 不修改前端依赖的事件名：继续发出 `doctor:remote-repair-progress`
- 不修改 session log 基本结构与协议 fallback 行为
- 优先按职责拆模块，不在本轮引入额外抽象层级
- 允许内部命名收紧，使函数名和所在模块匹配
- 测试跟随职责迁移，但测试覆盖目标不下降

## 4. 方案对比

### 方案 A：最小拆分

只把测试和少量工具函数移出，主循环仍留在单文件中。

优点：

- 改动最小
- 编译错误面最小

缺点：

- 入口文件仍然偏大
- 主流程、agent、config、执行器仍耦合
- 只能短期缓解长度问题

### 方案 B：职责拆分目录模块（推荐）

将 `remote_doctor.rs` 重构为 `remote_doctor/` 目录，按数据定义、基础设施、planning、repair orchestration 和测试拆分。

优点：

- 模块边界与现有代码天然一致
- 风险可控，不需要引入大范围对象化重写
- 后续新增 protocol 或测试会更容易落位

缺点：

- 需要一次性调整较多 `use` 和可见性
- 测试迁移时要小心 helper 暴露范围

### 方案 C：激进对象化

在方案 B 基础上再引入 `RemoteDoctorContext`、`ProtocolRunner`、`PlanExecutor` 等结构体，把大多数函数改为方法。

优点：

- 长期可读性最好
- 依赖关系最容易显式表达

缺点：

- 本轮改动面过大
- 行为回归风险明显增加
- 容易把“拆文件”演变成“架构重写”

## 5. 推荐方案

采用方案 B。

理由：

- 当前文件已经天然分成几段：基础类型与 config、agent/planning、repair loops、tests
- 用户允许内部命名调整，但没有要求业务重写，说明本轮应控制行为变化
- 方案 B 足以把文件长度和职责问题解决掉，同时保留当前函数式实现风格，降低回归风险

## 6. 目标模块结构

重构后使用目录模块：

- `src-tauri/src/remote_doctor/mod.rs`
- `src-tauri/src/remote_doctor/types.rs`
- `src-tauri/src/remote_doctor/session.rs`
- `src-tauri/src/remote_doctor/config.rs`
- `src-tauri/src/remote_doctor/agent.rs`
- `src-tauri/src/remote_doctor/plan.rs`
- `src-tauri/src/remote_doctor/repair_loops.rs`
- `src-tauri/src/remote_doctor/tests/`

各模块职责如下。

### 6.1 `mod.rs`

只负责：

- 声明子模块
- 汇总必要的 `use`
- 暴露 `start_remote_doctor_repair`
- 保留少量顶层常量和跨模块 glue code

`mod.rs` 不再承载大块业务逻辑。

### 6.2 `types.rs`

存放纯数据结构和小型无副作用 helper：

- `TargetLocation`
- `PlanKind`
- `PlanCommand`
- `PlanResponse`
- `CommandResult`
- `RemoteDoctorProtocol`
- `ClawpalServerPlanResponse`
- `ClawpalServerPlanStep`
- `RemoteDoctorRepairResult`
- `RemoteDoctorProgressEvent`
- `ConfigExcerptContext`
- `RepairRoundObservation`
- `StoredRemoteDoctorIdentity`
- `parse_target_location`

目标是让“协议定义”和“运行逻辑”解耦。

### 6.3 `session.rs`

存放 session 级别基础设施：

- log 目录解析
- JSONL session log 追加
- progress event 发射
- 通用 completion result helper

典型命名调整：

- `append_remote_doctor_log` -> `append_session_log`
- `emit_progress` -> `emit_session_progress`

### 6.4 `config.rs`

存放配置、identity 和 target I/O：

- gateway 配置读取
- auth token 对应的 gateway credentials 构建
- remote doctor identity 加载或生成
- target config read/write/restart
- rescue diagnosis 与相关 context 提取

典型命名调整：

- `remote_doctor_gateway_config` -> `load_gateway_config`
- `remote_doctor_gateway_credentials` -> `build_gateway_credentials`
- `load_or_create_remote_doctor_identity` 保留语义，但放入 config 模块

### 6.5 `agent.rs`

存放 agent planner 专属逻辑：

- protocol 相关 helper
- agent workspace bootstrap
- prompt 生成
- agent JSON response 解析
- bridge 辅助请求

典型命名调整：

- `ensure_local_remote_doctor_agent_ready` -> `ensure_agent_workspace_ready`
- `remote_doctor_agent_workspace_files` -> `agent_workspace_bootstrap_files`

### 6.6 `plan.rs`

存放通用 planning / command execution 逻辑：

- plan request
- clawpal-server plan request/result reporting
- invoke payload 解析
- 命令 argv 校验
- shell command 构造
- plan command 执行
- 本地 / 远程命令执行入口

这个模块是“planner 输出”和“实际执行器”之间的边界。

### 6.7 `repair_loops.rs`

存放高层编排逻辑：

- `run_remote_doctor_repair_loop`
- `run_clawpal_server_repair_loop`
- `run_agent_planner_repair_loop`
- `start_remote_doctor_repair_impl`

这里集中处理：

- detect / investigate / repair 的轮询
- protocol fallback
- loop 终止条件
- 跨模块 orchestration

## 7. 依赖方向

保持单向依赖，避免循环引用：

- `types` 被其他所有模块依赖
- `session` 只依赖 `types`
- `config` 依赖 `types` 和少量 crate 级命令
- `agent` 依赖 `types`、`config`、`session`
- `plan` 依赖 `types`、`session`
- `repair_loops` 依赖 `types`、`session`、`config`、`agent`、`plan`
- `mod.rs` 只负责组装和导出

## 8. 测试拆分

测试也按职责迁移，避免继续留在单个超大 `mod tests` 中。

建议拆法：

- `tests/types.rs`：枚举、序列化、轻量 helper
- `tests/agent.rs`：prompt、agent response、workspace bootstrap
- `tests/plan.rs`：argv 校验、invoke 解析、shell escape、plan parsing
- `tests/repair_loops.rs`：round 控制、stall detection、fallback
- `tests/live_e2e.rs`：live gateway / docker / SSH 相关测试

目标不是改测试语义，而是把测试落到更清晰的位置。

## 9. 风险与控制

### 9.1 可见性膨胀

拆模块后容易把原本文件内私有 helper 大量变成 `pub(crate)`。

控制方式：

- 默认保持私有
- 只在跨模块确有需要时提升到 `pub(super)` 或 `pub(crate)`
- 优先通过模块内组合减少暴露面

### 9.2 命名漂移

内部重命名可能让搜索历史和现有 mental model 断裂。

控制方式：

- 仅调整明显不贴职责的名字
- 保留对外稳定名
- 在迁移期让新名字和模块职责一一对应

### 9.3 测试迁移回归

大块测试拆分时最容易遗漏 helper 或 feature flag 条件。

控制方式：

- 先机械迁移，后清理重复 helper
- 先跑 targeted tests，再跑整个 `remote_doctor` 相关测试集
- live e2e 测试继续按环境变量守卫，不改变 skip 条件

## 10. 验收标准

重构完成后应满足：

- `src-tauri/src/remote_doctor.rs` 不再存在为巨型单文件，改为目录模块
- `mod.rs` 保持精简，主逻辑分散到职责模块
- `start_remote_doctor_repair` 外部调用点无需改动
- 现有 `remote_doctor` 单元测试和 e2e 测试保持通过或保持原有 skip 行为
- 内部命名比现状更贴合模块职责

## 11. 非目标

本轮不做：

- 修改远程 doctor 协议
- 改变 progress event payload 结构
- 重写 repair loop 算法
- 引入新的面向对象执行框架
- 调整前端 Doctor 页行为
