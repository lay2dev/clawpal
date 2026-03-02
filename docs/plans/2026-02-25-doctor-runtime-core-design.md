# ClawPal Doctor Runtime Core 设计（zeroclaw-first）

日期：2026-02-25  
状态：已确认（进入实现前基线）  
范围：仅 Doctor（暂不扩展 Install / Connectivity）

## 1. 目标

将当前 Doctor 中分散的 zeroclaw 接入逻辑收敛为统一 Runtime Core，确保：

1. 会话与记忆以 zeroclaw 为单一真相源（不在 ClawPal 重复造 memory）
2. ClawPal 只负责 UI、权限审批、执行桥接、审计与可观测
3. 后续扩展到 Install/Connectivity 时，不再重复重构

## 2. 核心原则

1. **Single Source of Truth**
- 会话、上下文、记忆、摘要均由 zeroclaw 管理
- ClawPal 不持久化对话正文，只保存 UI 投影和索引

2. **Decision vs Execution 分离**
- zeroclaw 做决策（chat / tool intent）
- ClawPal 做执行（本地/ssh/docker 命令）与审批

3. **Instance Isolation**
- 不同实例会话严格隔离：`local`、`docker:local`、`wsl2:local`、`ssh:<hostId>`

## 3. 模块边界

### 3.1 Runtime Core

新增目录：

- `src-tauri/src/runtime/mod.rs`
- `src-tauri/src/runtime/types.rs`
- `src-tauri/src/runtime/zeroclaw/adapter.rs`
- `src-tauri/src/runtime/zeroclaw/process.rs`
- `src-tauri/src/runtime/zeroclaw/session.rs`
- `src-tauri/src/runtime/zeroclaw/sanitize.rs`

职责：

- 统一 runtime trait 与事件模型
- 统一 zeroclaw 会话启动/发送/错误处理
- 统一输出净化与结构化解析

### 3.2 Doctor Bridge

新增：

- `src-tauri/src/doctor_runtime_bridge.rs`

职责：

- 将 `RuntimeEvent` 映射为现有 `doctor:*` Tauri 事件
- 保持前端 Doctor UI 协议稳定，降低迁移成本

### 3.3 Doctor Commands

调整：

- `src-tauri/src/doctor_commands.rs`

职责收敛为：

- 参数校验
- engine/domain/session 解析
- 调用 runtime bridge

不再内嵌 zeroclaw CLI 细节。

## 4. 会话模型

会话键：

`engine + domain + instance_id + agent_id + session_id`

规则：

1. `instance_id` 强隔离，不共享上下文
2. `start_diagnosis` 新建会话，`stop/disconnect` 结束会话
3. 支持同实例并行多会话（后续 UI 可拓展）

## 5. 存储与数据责任

### 5.1 zeroclaw 持久化（主存储）

- 会话上下文
- 记忆/摘要
- provider/model 路由状态

### 5.2 ClawPal 投影数据（辅助）

- 当前运行会话映射（`doctorSessionKey -> runtimeSessionId`）
- 编排/诊断事件索引（用于 UI 查询）
- 安全审计标记（不含明文）

## 6. 执行模型

1. Doctor 发起请求 -> Runtime Core 转发到 zeroclaw 会话接口
2. zeroclaw 返回 chat/tool 意图 -> Bridge 转换为 `doctor:*` 事件
3. 用户审批后由 ClawPal 执行命令（local/ssh/docker）
4. 执行结果回传 zeroclaw，继续下一步决策

说明：不再使用“每条消息单次 `agent -m`”作为长期方案。

## 7. 错误模型

统一错误码：

- `RUNTIME_UNREACHABLE`
- `CONFIG_MISSING`
- `MODEL_UNAVAILABLE`
- `SESSION_INVALID`
- `TARGET_UNREACHABLE`

每个错误必须伴随恢复动作（重连、切模型、补 key、重建会话等）。

## 8. 安全与脱敏

1. 所有 runtime 输出进入 UI 前做脱敏（key/token/cookie）
2. 检测疑似密钥泄露时提示用户轮换密钥
3. 审批边界保持在 ClawPal，不因 runtime 切换而变化

## 9. 分期迁移（Doctor-only）

### D1 Runtime Core 落地（功能等价）

- 新建 runtime 模块与 bridge
- Doctor commands 改为调用 bridge

验收：Doctor 功能不回归

### D2 切换到 zeroclaw 原生会话托管

- 去除 ClawPal 内部拼接式会话记忆
- 统一走 zeroclaw session

验收：连续追问不丢上下文，跨实例不串话

### D3 工具调用闭环统一

- tool intent -> 审批 -> 执行 -> 回传 runtime

验收：多步诊断链路可持续推进

### D4 可观测与安全完善

- Runtime 事件进入编排/诊断面板
- 错误恢复动作可视化

验收：失败可定位、敏感信息不泄露

### D5 技术债清理

- 删除 Doctor 中零散 zeroclaw 直调
- 更新架构文档与调试文档

验收：Doctor 仅依赖 Runtime Core

## 10. 不在本阶段做

1. Install 全量迁移到 Runtime Core
2. SSH/WSL/Docker Connectivity 的 runtime 化编排
3. 远程实例全自动凭据双向同步策略完整落地

这些将在 Doctor 稳定后进入下一阶段设计。
