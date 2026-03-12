# 如何编写一个 ClawPal Recipe

这份文档描述的是当前仓库里真实可执行的 Recipe DSL，而不是早期草案。

目标读者：
- 需要新增预置 Recipe 的开发者
- 需要维护 `examples/recipe-library/` 外部 Recipe 库的人
- 需要理解 `Recipe Source -> ExecutionSpec -> runner` 这条链路的人

## 1. 先理解运行时模型

当前 ClawPal 的 Recipe 有两种入口：

1. 作为预置 Recipe 随 App 打包，并在启动时 seed 到 workspace
2. 作为外部 Recipe library 在运行时导入

无论入口是什么，最终运行时载体都是 workspace 里的单文件 JSON：

`~/.clawpal/recipes/workspace/<slug>.recipe.json`

也就是说：
- source authoring 可以是目录结构
- import/seed 之后会变成自包含单文件
- runner 永远不直接依赖外部 `assets/` 目录

## 2. 推荐的作者目录结构

新增一个可维护的 Recipe，推荐放在独立目录里，而不是直接写进 `src-tauri/recipes.json`。

当前仓库采用的结构是：

```text
examples/recipe-library/
  dedicated-agent/
    recipe.json
  agent-persona-pack/
    recipe.json
    assets/
      personas/
        coach.md
        researcher.md
  channel-persona-pack/
    recipe.json
    assets/
      personas/
        incident.md
        support.md
```

规则：
- 每个 Recipe 一个目录
- 目录里必须有 `recipe.json`
- 如需预设 markdown 文本，放到 `assets/`
- import 时只扫描 library 根目录下的一级子目录

## 3. 顶层文档形状

对于 library 里的 `recipe.json`，推荐写成单个 recipe 对象。

当前加载器支持三种形状：

```json
{ "...": "single recipe object" }
```

```json
[
  { "...": "recipe 1" },
  { "...": "recipe 2" }
]
```

```json
{
  "recipes": [
    { "...": "recipe 1" },
    { "...": "recipe 2" }
  ]
}
```

但有一个关键区别：
- `Load` 文件或 URL 时，可以接受三种形状
- `Import` 外部 recipe library 时，`recipe.json` 必须是单个对象

因此，写新的 library recipe 时，直接使用单对象。

## 4. 一个完整 Recipe 的推荐结构

当前推荐写法：

```json
{
  "id": "dedicated-agent",
  "name": "Dedicated Agent",
  "description": "Create an independent agent and set its identity and persona",
  "version": "1.0.0",
  "tags": ["agent", "identity", "persona"],
  "difficulty": "easy",
  "presentation": {
    "resultSummary": "Created dedicated agent {{name}} ({{agent_id}})"
  },
  "params": [],
  "steps": [],
  "bundle": {},
  "executionSpecTemplate": {},
  "clawpalImport": {}
}
```

字段职责：
- `id / name / description / version / tags / difficulty`
  Recipe 元信息
- `presentation`
  面向用户的结果文案
- `params`
  Configure 阶段的参数表单
- `steps`
  面向用户的步骤文案
- `bundle`
  声明 capability、resource claim、execution kind 的白名单
- `executionSpecTemplate`
  真正要编译成什么 `ExecutionSpec`
- `clawpalImport`
  仅用于 library import 阶段的扩展元数据，不会保留在最终 workspace recipe 里

## 5. 参数字段怎么写

`params` 是数组，每项形状如下：

```json
{
  "id": "agent_id",
  "label": "Agent ID",
  "type": "string",
  "required": true,
  "placeholder": "e.g. ops-bot",
  "pattern": "^[a-z0-9-]+$",
  "minLength": 3,
  "maxLength": 32,
  "defaultValue": "main",
  "dependsOn": "independent",
  "options": [
    { "value": "coach", "label": "Coach" }
  ]
}
```

当前前端支持的 `type`：
- `string`
- `number`
- `boolean`
- `textarea`
- `discord_guild`
- `discord_channel`
- `model_profile`
- `agent`

UI 规则：
- `options` 非空时，优先渲染为下拉
- `discord_guild` 从当前环境加载 guild 列表
- `discord_channel` 从当前环境加载 channel 列表
- `agent` 从当前环境加载 agent 列表
- `model_profile` 从当前环境加载可用 model profiles
- `dependsOn` 当前仍是简单门控，不要依赖复杂表达式

实用建议：
- 长文本输入用 `textarea`
- 固定预设优先用 `options`
- `model_profile` 如果希望默认跟随环境，可用 `__default__`

## 6. `steps` 和 `executionSpecTemplate.actions` 必须一一对应

`steps` 是给用户看的，`executionSpecTemplate.actions` 是给编译器和 runner 看的。

当前校验要求：
- `steps.len()` 必须等于 `executionSpecTemplate.actions.len()`
- 每一步的 `action` 应与对应 action 的 `kind` 保持一致

也就是说，`steps` 不是装饰层，它是用户理解“这次会做什么”的主入口。

## 7. 当前支持的 action surface

当前 Recipe DSL 的 action 分三组。

### 7.1 业务动作

- `create_agent`
- `delete_agent`
- `setup_identity`
- `bind_channel`
- `unbind_channel`
- `set_agent_model`
- `set_agent_persona`
- `clear_agent_persona`
- `set_channel_persona`
- `clear_channel_persona`

推荐：
- 新的业务 recipe 优先使用业务动作
- `setup_identity` 作为兼容动作保留，新的 recipe 一般不再优先依赖它

### 7.2 文档动作

- `upsert_markdown_document`
- `delete_markdown_document`

这是高级/底座动作，适合：
- 写 agent 默认 markdown 文档
- 直接控制 section upsert 或 whole-file replace

### 7.3 环境动作

- `ensure_model_profile`
- `delete_model_profile`
- `ensure_provider_auth`
- `delete_provider_auth`

这组动作负责：
- 确保目标环境存在可用 profile
- 必要时同步 profile 依赖的 auth/secret
- 清理不再需要的 auth/profile

### 7.4 Escape hatch

- `config_patch`

保留用于低层配置改写，但不建议作为 bundled recipe 的主路径。

## 8. 各类 action 的常见输入

### `create_agent`

```json
{
  "kind": "create_agent",
  "args": {
    "agentId": "{{agent_id}}",
    "modelProfileId": "{{model}}",
    "independent": true
  }
}
```

### `set_agent_persona`

```json
{
  "kind": "set_agent_persona",
  "args": {
    "agentId": "{{agent_id}}",
    "persona": "{{presetMap:persona_preset}}"
  }
}
```

### `set_channel_persona`

```json
{
  "kind": "set_channel_persona",
  "args": {
    "channelType": "discord",
    "guildId": "{{guild_id}}",
    "peerId": "{{channel_id}}",
    "persona": "{{presetMap:persona_preset}}"
  }
}
```

### `upsert_markdown_document`

```json
"args": {
  "target": {
    "scope": "agent",
    "agentId": "{{agent_id}}",
    "path": "IDENTITY.md"
  },
  "mode": "replace",
  "content": "- Name: {{name}}\n\n## Persona\n{{persona}}\n"
}
```

支持的 `target.scope`：
- `agent`
- `home`
- `absolute`

支持的 `mode`：
- `replace`
- `upsertSection`

`upsertSection` 需要额外提供：
- `heading`
- 可选 `createIfMissing`

### `delete_markdown_document`

```json
"args": {
  "target": {
    "scope": "agent",
    "agentId": "{{agent_id}}",
    "path": "PLAYBOOK.md"
  },
  "missingOk": true
}
```

### `ensure_model_profile`

```json
{
  "kind": "ensure_model_profile",
  "args": {
    "profileId": "{{model}}"
  }
}
```

### `ensure_provider_auth`

```json
{
  "kind": "ensure_provider_auth",
  "args": {
    "provider": "openrouter",
    "authRef": "openrouter:default"
  }
}
```

### destructive 动作

以下动作默认会做引用检查，仍被引用时会失败：
- `delete_agent`
- `delete_model_profile`
- `delete_provider_auth`

显式 override：
- `delete_agent.force`
- `delete_agent.rebindChannelsTo`
- `delete_provider_auth.force`
- `delete_model_profile.deleteAuthRef`

## 9. `bundle` 写什么

`bundle` 的作用是声明：
- 允许使用哪些 capability
- 允许触碰哪些 resource kind
- 支持哪些 execution kind

例如：

```json
"bundle": {
  "apiVersion": "strategy.platform/v1",
  "kind": "StrategyBundle",
  "metadata": {
    "name": "dedicated-agent",
    "version": "1.0.0",
    "description": "Create a dedicated agent"
  },
  "compatibility": {},
  "inputs": [],
  "capabilities": {
    "allowed": ["agent.manage", "document.write", "model.manage", "secret.sync"]
  },
  "resources": {
    "supportedKinds": ["agent", "document", "modelProfile"]
  },
  "execution": {
    "supportedKinds": ["job"]
  },
  "runner": {},
  "outputs": [{ "kind": "recipe-summary", "recipeId": "dedicated-agent" }]
}
```

当前常见 capability：
- `agent.manage`
- `agent.identity.write`
- `binding.manage`
- `config.write`
- `document.write`
- `document.delete`
- `model.manage`
- `auth.manage`
- `secret.sync`

当前常见 resource claim kind：
- `agent`
- `channel`
- `file`
- `document`
- `modelProfile`
- `authProfile`

## 10. `executionSpecTemplate` 写什么

它定义编译后真正的 `ExecutionSpec`，通常至少要包含：

```json
"executionSpecTemplate": {
  "apiVersion": "strategy.platform/v1",
  "kind": "ExecutionSpec",
  "metadata": {
    "name": "dedicated-agent"
  },
  "source": {},
  "target": {},
  "execution": {
    "kind": "job"
  },
  "capabilities": {
    "usedCapabilities": ["model.manage", "secret.sync", "agent.manage", "document.write"]
  },
  "resources": {
    "claims": [
      { "kind": "modelProfile", "id": "{{model}}" },
      { "kind": "agent", "id": "{{agent_id}}" },
      { "kind": "document", "path": "agent:{{agent_id}}/IDENTITY.md" }
    ]
  },
  "secrets": {
    "bindings": []
  },
  "desiredState": {
    "actionCount": 3
  },
  "actions": [],
  "outputs": [{ "kind": "recipe-summary", "recipeId": "dedicated-agent" }]
}
```

当前 `execution.kind` 支持：
- `job`
- `service`
- `schedule`
- `attachment`

对大多数业务 recipe：
- 一次性业务动作优先用 `job`
- 配置附着类动作可用 `attachment`

## 11. 模板变量

当前支持两类最常用模板。

### 11.1 参数替换

```json
"agentId": "{{agent_id}}"
```

### 11.2 preset map 替换

```json
"persona": "{{presetMap:persona_preset}}"
```

这类变量只在 import 后的 workspace recipe 里使用编译好的 map，不会在运行时继续去读外部 `assets/`。

## 12. `clawpalImport` 和 `assets/`

如果 recipe 需要把外部 markdown 资产编译进最终 recipe，可以使用：

```json
"clawpalImport": {
  "presetParams": {
    "persona_preset": [
      { "value": "coach", "label": "Coach", "asset": "assets/personas/coach.md" },
      { "value": "researcher", "label": "Researcher", "asset": "assets/personas/researcher.md" }
    ]
  }
}
```

import 阶段会做三件事：
- 校验 `asset` 是否存在
- 为目标 param 注入 `options`
- 把 `{{presetMap:param_id}}` 编译成内嵌文本映射

最终写入 workspace 的 recipe：
- 不再保留 `clawpalImport`
- 不再依赖原始 `assets/` 目录
- 会带 `clawpalPresetMaps`

## 13. `presentation` 怎么用

如果希望 `Done`、`Recent Recipe Runs`、`Orchestrator` 显示更业务化的结果，给 recipe 增加：

```json
"presentation": {
  "resultSummary": "Updated persona for agent {{agent_id}}"
}
```

原则：
- 写给非技术用户看
- 描述“得到什么结果”，不要描述执行细节
- 没写时会退回到通用 summary

## 14. OpenClaw-first 原则

作者在写 Recipe 时要默认遵循：

- 能用业务动作表达的，不要退回 `config_patch`
- 能用 OpenClaw 原语表达的，让 runner 优先走 OpenClaw
- 文档动作只在 OpenClaw 还没有对应原语时作为底座

例如：
- `set_channel_persona` 优于手写 `config_patch`
- `ensure_model_profile` 优于假定目标环境已经有 profile
- `upsert_markdown_document` 适合写 agent 默认 markdown 文档

更详细的边界见：[recipe-runner-boundaries.md](./recipe-runner-boundaries.md)

## 15. 最小验证流程

新增或修改 recipe 后，至少做这几步：

1. 校验 Rust 侧 recipe 测试

```bash
cargo test recipe_ --lib --manifest-path src-tauri/Cargo.toml
```

2. 校验前端类型和关键 UI

```bash
bun run typecheck
```

3. 如改了导入规则或预置 recipe，验证 import/seed 结果

```bash
cargo test import_recipe_library_accepts_repo_example_library --manifest-path src-tauri/Cargo.toml
```

4. 如改了业务闭环，优先补 Docker OpenClaw e2e

## 16. 常见坑

- `steps` 和 `actions` 数量不一致会直接校验失败
- `Import` library 时，`recipe.json` 不能是数组
- `upsert_markdown_document` 的 `upsertSection` 模式必须带 `heading`
- `target.scope=agent` 时必须带 `agentId`
- 相对路径里不允许 `..`
- destructive action 默认会被引用检查挡住
- recipe 不能内嵌明文 secret；环境动作只能引用 ClawPal 已能解析到的 secret/auth

如果你需要理解 runner 负责什么、不负责什么，再看：[recipe-runner-boundaries.md](./recipe-runner-boundaries.md)
    "version": "1.0.0",
    "description": "Create a dedicated agent"
  },
  "compatibility": {},
  "inputs": [],
  "capabilities": {
    "allowed": ["agent.manage", "agent.identity.write"]
  },
  "resources": {
    "supportedKinds": ["agent"]
  },
  "execution": {
    "supportedKinds": ["job"]
  },
  "runner": {},
  "outputs": [
    { "kind": "recipe-summary", "recipeId": "dedicated-agent" }
  ]
}
```

当前资源 claim kind 白名单是：

- `path`
- `file`
- `service`
- `channel`
- `agent`
- `identity`

## 10. `executionSpecTemplate` 写什么

`executionSpecTemplate` 是真正会被渲染参数的执行模板。

一个常见例子：

```json
"executionSpecTemplate": {
  "apiVersion": "strategy.platform/v1",
  "kind": "ExecutionSpec",
  "metadata": {
    "name": "dedicated-agent"
  },
  "source": {},
  "target": {},
  "execution": {
    "kind": "job"
  },
  "capabilities": {
    "usedCapabilities": ["agent.manage", "agent.identity.write"]
  },
  "resources": {
    "claims": [
      { "kind": "agent", "id": "{{agent_id}}" }
    ]
  },
  "secrets": {
    "bindings": []
  },
  "desiredState": {
    "actionCount": 2
  },
  "actions": [
    {
      "kind": "create_agent",
      "name": "Create dedicated agent",
      "args": {
        "agentId": "{{agent_id}}",
        "modelProfileId": "{{model}}",
        "independent": true
      }
    },
    {
      "kind": "setup_identity",
      "name": "Set agent identity",
      "args": {
        "agentId": "{{agent_id}}",
        "name": "{{name}}",
        "emoji": "{{emoji}}",
        "persona": "{{persona}}"
      }
    }
  ],
  "outputs": [
    { "kind": "recipe-summary", "recipeId": "dedicated-agent" }
  ]
}
```

实用规则：
- `metadata.name` 通常用 recipe id
- `source` 和 `target` 可以先留空，运行时会补上下文
- `desiredState.actionCount` 应和 actions 数量一致
- `resources.claims` 要能说明这次会碰到什么对象

## 11. 模板变量怎么渲染

当前支持两类占位符：

### 普通参数

```text
{{agent_id}}
{{channel_id}}
{{name}}
```

它会用参数值直接替换。

### 预设映射

```text
{{presetMap:persona_preset}}
```

它会根据当前参数值，从 `clawpalPresetMaps.persona_preset` 里取对应内容。

这个机制通常用于：
- persona preset
- system prompt preset
- 一组较长的 markdown 文案

注意：
- 占位符不仅能出现在值里，也能出现在对象 key 里
- `config_patch` 常会用这一点渲染 `guild_id` / `channel_id` 这种动态路径

## 12. 如何写预设资源型 Recipe

如果一个参数需要从 `assets/*.md` 这种资源文件里选预设，不建议手写 `clawpalPresetMaps`。

推荐写法是 `clawpalImport`：

```json
"clawpalImport": {
  "presetParams": {
    "persona_preset": [
      { "value": "coach", "label": "Coach", "asset": "assets/personas/coach.md" },
      { "value": "researcher", "label": "Researcher", "asset": "assets/personas/researcher.md" }
    ]
  }
}
```

导入器会做三件事：
- 把这些 asset 文件读进来
- 自动给对应 param 注入 `options`
- 自动生成 `clawpalPresetMaps`

所以作者只需要写：
- `params` 里保留 `persona_preset`
- `actions` 里引用 `{{presetMap:persona_preset}}`

不需要把大段 markdown 直接内联到 `recipe.json`。

## 13. `presentation.resultSummary` 是给谁看的

这个字段会直接影响：
- `Done` 页的结果摘要
- `Recent Recipe Runs`
- 其他结果导向的 UI

例如：

```json
"presentation": {
  "resultSummary": "Updated persona for agent {{agent_id}}"
}
```

建议：
- 用业务结果句式
- 不要写技术实现细节
- 不要写 “via local runner” / “2 actions applied” 这种内部表达

好的例子：
- `Created dedicated agent {{name}} ({{agent_id}})`
- `Updated persona for agent {{agent_id}}`
- `Updated persona for channel {{channel_id}}`

## 14. 当前推荐的作者流程

### 方案 A：写一个预置 Recipe

1. 在 `examples/recipe-library/<your-recipe>/` 新建目录
2. 写 `recipe.json`
3. 如果需要 preset 资产，放进 `assets/`
4. 重启 app，让启动 seed 把它写进 workspace
5. 在 `Recipes` 页面直接验证

### 方案 B：写一个外部导入 Recipe

1. 在任意目录按相同结构组织 recipe library
2. 在 `Recipes` 页面用 `Import` 导入根目录
3. 导入后从 workspace 里打开 `Studio` 或 `Cook`

## 15. 最小验证命令

至少做这几类验证：

```bash
cargo test recipe_ --lib --manifest-path src-tauri/Cargo.toml
```

```bash
bun run typecheck
```

如果改了 `Cook / RecipePlanPreview / Orchestrator / ParamForm` 一类前端行为，再补对应前端测试。

## 16. 常见坑

### 1. `steps` 和 `actions` 数量不一致

这是当前最常见的 schema 错误之一。

### 2. 写了 UI 参数，但没在模板里用

这种参数不会产生实际效果，也容易误导用户。

### 3. `clawpalImport` 引用了不存在的 asset

导入时会直接失败。

### 4. 在 `bundle` 里没放 capability 或 resource kind

即使 `executionSpecTemplate` 写对了，也会被 bundle 校验挡住。

### 5. 把业务结果写成技术结果

`presentation.resultSummary` 应该描述“效果”，不是描述“执行细节”。

## 17. 建议从现有 3 个例子开始

当前最值得参考的例子在：

- [dedicated-agent/recipe.json](/Users/ChenYu/Documents/Github/clawpal/.worktrees/feat/recipe-import-library/examples/recipe-library/dedicated-agent/recipe.json)
- [agent-persona-pack/recipe.json](/Users/ChenYu/Documents/Github/clawpal/.worktrees/feat/recipe-import-library/examples/recipe-library/agent-persona-pack/recipe.json)
- [channel-persona-pack/recipe.json](/Users/ChenYu/Documents/Github/clawpal/.worktrees/feat/recipe-import-library/examples/recipe-library/channel-persona-pack/recipe.json)

它们分别覆盖了：
- 纯参数型 recipe
- 预设 persona 导入到 agent
- 预设 persona 导入到 channel

如果你要新增第四个 recipe，最稳的做法通常不是从零开始，而是从这三个里挑一个最接近的复制出来改。
