# 自适应修复“可凝固”设计

> 日期：2026-02-28  
> 状态：设计稿（待实现）

## 1. 背景与理念

在 GUI-CLI-Agent 三层架构下，异常处理不能只停留在“当次建议”。

目标是把一次成功修复沉淀为可复用的“补洞材料”：
- 下次同类环境与同类故障出现时，可直接触发确定性修复；
- 修复过程可审计、可回滚、可失效；
- 不把系统变成不可解释的隐式魔法。

一句话：**从“经验描述”升级为“经验可执行规则”。**

## 2. 设计目标

1. 将一次修复结果结构化为规则，而非仅文本记录。  
2. 规则命中后输出确定性执行计划（参数覆盖 / 行为切换 / Doctor 交接）。  
3. 保留安全边界：高风险写操作仍需用户确认。  
4. 支持版本、回滚、TTL（过期）与命中统计。  
5. 与现有 guidance + precheck + doctor 管线对齐，不引入并行新通道。

## 3. 设计边界

### 3.1 本期包含
- 规则化参数覆盖（最小可用）
- 规则命中与执行日志
- 规则生命周期管理（enabled/version/ttl）

### 3.2 本期不包含
- 全自动高风险写入修复（删改配置、重装）
- 黑盒 ML 学习策略
- 无约束的全局行为替换

## 4. 分层模型

### 4.1 事实层（Fact Layer）
记录“发生了什么”，用于追溯和分析：
- 环境指纹（OS、实例类型、openclaw/clawpal 版本）
- 错误码与原始错误摘要
- 执行动作、结果、耗时、是否回滚

该层不直接改变运行行为。

### 4.2 策略层（Policy Layer）
将经验沉淀为“参数覆盖规则”：
- 超时、重试、路径优先级、默认 base_url 等
- 命中条件明确，不做自由推理

该层优先落地，风险最低。

### 4.3 行为层（Behavior Layer）
在可控前提下切换默认行为：
- 仅对高频、低风险且可验证的场景
- 必须具备回滚与 TTL

例如：某平台 PATH 修复链路从 A 切换为 B。

## 5. 规则模型（repair_rules.v1）

```json
{
  "id": "rule-path-darwin-finder-v1",
  "enabled": true,
  "version": 1,
  "priority": 100,
  "match": {
    "os": ["macos"],
    "instanceType": ["local"],
    "errorCode": ["RUNTIME_UNREACHABLE"],
    "contains": ["openclaw", "not found"]
  },
  "action": {
    "type": "param_override",
    "target": "path_resolution",
    "payload": {
      "preferShellFix": true,
      "extraPaths": ["/opt/homebrew/bin", "~/.npm-global/bin"]
    }
  },
  "verify": {
    "kind": "command",
    "tool": "openclaw",
    "args": "--version"
  },
  "lifecycle": {
    "ttlHours": 720,
    "createdAt": "2026-02-28T00:00:00Z"
  }
}
```

## 6. 执行流程

1. 触发点（precheck 失败 / 操作失败）进入 guidance 管线。  
2. 根据环境 + 错误码匹配规则（按 priority）。  
3. 生成确定性执行计划（非自由文本）。  
4. 低风险动作可 inline_fix；高风险动作转 doctor_handoff。  
5. 执行后写入事实层日志；失败则回滚并上报。

## 6.1 案例：命令语法自愈（仅设计，不默认开启）

典型场景：Agent 先执行 `openclaw agent list --json`，被 CLI 拒绝（应为 `agents` 复数），随后自动修正为 `openclaw agents list --json` 并成功。

可沉淀为策略层规则：
- `match`：
  - `errorCode=ENGINE_ERROR`
  - `contains=["unsupported openclaw args", "allowed top-level commands"]`
  - `tool=openclaw`
  - `argsPrefix=["agent list"]`
- `action`：
  - `type=command_rewrite`
  - `rewrite={"from":"^agent\\s+list","to":"agents list"}`
  - `maxRetry=1`
  - `risk=read_only`
- `verify`：
  - exitCode 必须为 0 且 stdout 可解析为 JSON

安全约束：
- 仅允许只读命令自动重写；
- 仅重试一次；
- 每次重写必须记录审计事件（原始命令/重写后命令/结果）。

## 7. 安全与回滚

- 规则白名单：仅允许既定 action type。  
- 高风险动作必须显式确认。  
- 每次执行自动生成 rollback point（能回滚就必须回滚）。  
- 规则默认带 TTL，避免陈旧策略长期污染。

## 8. 与现有架构对齐

- GUI：仅展示计划与确认，不直接承载修复逻辑。  
- Agent（zeroclaw）：用于复杂异常推理与 doctor 会话，不替代确定性规则引擎。  
- CLI/core：仍是最终执行通道。

该设计与 `2026-02-26-gui-cli-agent-layers-design.md`、`2026-02-28-zeroclaw-anomaly-fallback-design.md` 一致，属于“经验持久化”能力补齐。

## 9. 渐进落地建议

1. M1：实现 param_override 规则 + 命中统计 + TTL。  
2. M2：接入 guidance 卡片“规则命中说明”。  
3. M3：引入 behavior_toggle（仅 1-2 个低风险场景）。  
4. M4：把高价值规则沉淀为默认内置规则集。

## 10. 成功标准

- 同类问题二次出现时，人工介入率明显下降。  
- 修复成功率上升且回归率可控。  
- 每次自动修复都有可追溯记录与回滚证据。
