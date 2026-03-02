# Practice Recipe v2 设计文档

## 日期
2026-02-24

## 背景与目标

当前 ClawPal 的 recipe 已支持步骤化执行，但核心仍偏向 OpenClaw 配置写入与 Discord 特定参数。  
本方案目标是将 recipe 升级为可分享的“最佳实践编排包”，覆盖：

- OpenClaw 配置最佳实践
- 外部平台配套动作（Discord/Telegram 等）
- 主机侧自动化动作（本机/SSH 远程）

核心原则：

1. Recipe 语义平台无关，不把 Discord/Telegram 细节写死在 recipe 层。
2. 本机与远程统一为“执行目标”，不在 recipe 概念层分叉。
3. 强自动化优先，但所有高风险动作都必须可见、可审计、可补偿。

## 现状约束（基于当前代码）

已具备能力（本地与远程）：

- OpenClaw 配置读写、patch、快照、回滚
- Agent / Binding / Model Profile 管理
- Doctor/Fix、Upgrade、日志、Cron、Watchdog
- 远程 SSH Exec + SFTP 读写删（能力面较宽）
- Discord REST 读取能力（guild/channel 解析）

当前不足：

- 参数体系仍有 Discord 硬编码类型（如 `discord_guild`、`discord_channel`）
- Recipe 模板仍以渠道配置路径为主，不适合表达通用实践目标
- 平台写操作（如“创建频道”）尚未进入统一动作层

## 概念模型

### 1) Practice Recipe（用户可分享单元）

描述“目标状态”与“执行剧本”，不直接绑定平台实现细节。

建议结构：

- 元数据：`id/name/version/tags/risk_level`
- 输入参数：与平台无关的 `resource_selector` / `account_ref` / `scope`
- 执行步骤：调用 capability action
- 验证步骤：声明成功判据
- 补偿步骤：平台侧回退动作（可选）

### 2) Capability Action（统一动作协议）

Recipe 仅调用通用动作名，例如：

- `openclaw.ensure_agent`
- `openclaw.ensure_binding`
- `platform.ensure_channel`
- `platform.ensure_permission`
- `host.exec`
- `host.write_file`
- `verify.assert_binding_effective`

每个 action 约定统一输入输出：

- 输入：`inputs` + `execution_target` + `credential_refs`
- 输出：`artifacts`（如 channel_id、file_path）+ `evidence`
- 错误：标准错误码（权限不足、资源不存在、速率限制、网络超时）
- 幂等：必须声明 `idempotent: true|false`

### 3) Connector（平台/环境适配器）

按 provider 实现 action：

- 平台：Discord / Telegram / Slack ...
- 运行环境：Local / SSH Remote

Recipe 不关心具体 API 路径，只依赖 capability contract。

## 执行目标抽象（Local/Remote）

统一引入 `execution_target`：

- `local`: 当前桌面用户上下文
- `remote:<host_id>`: SSH 指定主机上下文

设计要点：

1. 不假设本机权限小于远程权限。权限由执行身份决定。
2. Recipe 本身不区分 local/remote；运行时路由到对应 executor。
3. 每个 action 执行前做 target-aware preflight（环境工具、路径、网络可达）。

## 权限与凭证模型

### 权限分级

- `safe`: 读操作、无副作用验证
- `elevated`: 可能改配置/创建资源
- `destructive`: 删除/覆盖/不可逆操作

### 凭证分级

- `config_credentials`: OpenClaw 相关密钥引用
- `platform_credentials`: Discord/Telegram bot token 等
- `host_credentials`: SSH key/password/sudo 相关

执行要求：

1. Recipe 清单必须声明所需权限与凭证类型。
2. 默认仅自动执行 `safe`；`elevated/destructive` 需要显式确认。
3. Token 可本地持久化（用户已接受），但需加密存储、掩码展示、可轮换、可撤销。

## 标准执行流程

1. Preflight
- 检查 capability 支持矩阵
- 检查权限与凭证完整性
- 检查目标资源与网络可达性

2. Plan
- 生成可读执行计划（将创建/修改/删除什么）
- 标注每步风险等级与预期产物

3. Apply
- 按步骤执行，保存中间 artifacts
- 每步记录 structured evidence

4. Verify
- 按 recipe 声明的判据做后置验证
- 失败时输出可操作修复建议

5. Rollback / Compensation
- OpenClaw 配置改动：使用快照回滚
- 平台侧改动：执行补偿动作（如删除新建频道、撤销权限）

## 可编排动作清单（首批）

### OpenClaw

- `openclaw.ensure_agent`
- `openclaw.ensure_identity`
- `openclaw.ensure_binding`
- `openclaw.apply_patch`
- `openclaw.run_doctor`
- `openclaw.rollback_snapshot`

### Platform（provider-agnostic）

- `platform.ensure_workspace`
- `platform.ensure_channel`
- `platform.ensure_bot_permission`
- `platform.ensure_webhook`
- `platform.post_message_template`
- `platform.resolve_channel`

### Host（local/remote）

- `host.exec`
- `host.read_file`
- `host.write_file`
- `host.manage_cron`
- `host.manage_watchdog`
- `host.read_logs`

### Verify

- `verify.assert_config_path`
- `verify.assert_binding_effective`
- `verify.assert_platform_resource_exists`
- `verify.assert_command_exit_zero`

## Recipe Schema（MVP 方向）

建议新增字段：

- `executionTargets`: `["local", "remote"]`
- `requiredCapabilities`: capability 列表
- `requiredCredentials`: 凭证引用列表
- `riskPolicy`: `allow_safe | allow_elevated | allow_destructive`
- `steps[].onFailure`: `stop | continue | compensate`
- `steps[].produces`: artifacts 声明

并保留现有 steps 思路，兼容增量迁移。

## 示例实践模板（跨平台）

### 模板 A：社区问答机器人上线路径

目标：
- 创建/复用 Agent
- 创建/复用平台频道
- 绑定频道到 Agent
- 注入欢迎消息与基础运营提示

特点：
- Discord/Telegram 仅在 connector 层差异化
- Recipe 级别不出现平台特定 JSON 路径

### 模板 B：值班告警通道标准化

目标：
- 建立告警频道与权限
- 设置 OpenClaw 告警代理路由
- 配置 watchdog + cron 健康巡检
- 验证告警消息可达

特点：
- 同时编排 platform + openclaw + host 三类动作

## 失败与恢复策略

1. 不追求全局事务；采用“配置回滚 + 平台补偿”双轨模型。
2. 所有 destructive 动作必须单独确认并记录 evidence。
3. 每次执行生成审计记录：
- recipe 版本
- execution target
- 凭证引用（不含明文）
- 变更摘要
- 失败步骤与错误码

## 实施路线（建议）

Phase 1（最小可用）：

1. 引入 capability registry 与 connector 接口
2. 抽象 `execution_target` 与 preflight
3. 新增 6-8 个通用 action（优先 openclaw + host 基础）

Phase 2（平台增强）：

1. 增加 Discord 写操作（create channel/permission/webhook）
2. 增加 Telegram connector 的对等 capability
3. 增加补偿动作与风险确认 UI

Phase 3（分享生态）：

1. Recipe manifest 标准化与签名校验（可选）
2. 官方“最佳实践包”仓库与版本兼容策略
3. 社区 recipe 上报与质量评分机制（后续）

## 验收标准

1. 同一份 recipe 可在 local/remote 两种 target 下执行（由路由决定，不改 recipe 内容）。
2. 同一份实践模板可切换 Discord/Telegram connector，无需改 recipe 步骤语义。
3. 执行结果必须包含可审计 evidence，失败有明确补偿或回滚路径。
4. 用户可一键导入并运行“最佳实践包”，看到 preflight、风险提示、执行与验证结果。
