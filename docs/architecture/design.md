# ClawPal Design Document

> OpenClaw 配置助手 — 让普通用户也能玩转高级配置

## 1. 产品定位

### 问题
- OpenClaw 配置功能强大但复杂
- 官方 Web UI 是"配置项罗列"，用户看晕
- 用户让 Agent 自己配置，经常出错
- 配置出错时 Gateway 起不来，陷入死循环

### 解决方案
**场景驱动的配置助手**
- 不是"列出所有配置项"，而是"你想实现什么场景？"
- 用户选场景 → 填几个参数 → 一键应用
- 独立运行，不依赖 Gateway（配置坏了也能修）

### 核心价值
1. **降低门槛** — 普通用户也能用上高级功能
2. **最佳实践** — 社区沉淀的配置方案，一键安装
3. **急救工具** — 配置出问题时的救命稻草
4. **版本控制** — 改坏了一键回滚

## 2. 产品架构

```
┌─────────────────────────────────────────────────────────┐
│                    clawpal.dev (官网)                    │
│                                                         │
│   ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐   │
│   │ Recipe  │  │ Recipe  │  │ Recipe  │  │ Recipe  │   │
│   │  Card   │  │  Card   │  │  Card   │  │  Card   │   │
│   └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘   │
│        │            │            │            │         │
│        └────────────┴─────┬──────┴────────────┘         │
│                           │                             │
│                    [一键安装按钮]                         │
│                           │                             │
└───────────────────────────┼─────────────────────────────┘
                            │
                            │ clawpal://install/recipe-id
                            ▼
┌─────────────────────────────────────────────────────────┐
│                 ClawPal App (本地)                       │
│                                                         │
│   ┌──────────────────────────────────────────────────┐  │
│   │                   首页                            │  │
│   │  ┌─────────┐  当前配置健康状态: ✅ 正常           │  │
│   │  │  状态   │  OpenClaw 版本: 2026.2.13           │  │
│   │  │  卡片   │  活跃 Agents: 4                     │  │
│   │  └─────────┘                                     │  │
│   └──────────────────────────────────────────────────┘  │
│                                                         │
│   ┌──────────────────────────────────────────────────┐  │
│   │                  场景库                           │  │
│   │  ┌─────────┐  ┌─────────┐  ┌─────────┐          │  │
│   │  │ Discord │  │ Telegram│  │  模型   │          │  │
│   │  │ 人设    │  │ 配置    │  │  切换   │          │  │
│   │  └─────────┘  └─────────┘  └─────────┘          │  │
│   └──────────────────────────────────────────────────┘  │
│                                                         │
│   ┌──────────────────────────────────────────────────┐  │
│   │                 历史记录                          │  │
│   │  ● 2026-02-15 21:30 应用了 "Discord 人设"        │  │
│   │  ● 2026-02-15 20:00 手动编辑                     │  │
│   │  ● 2026-02-14 15:00 应用了 "性能优化"            │  │
│   │                              [回滚到此版本]        │  │
│   └──────────────────────────────────────────────────┘  │
│                                                         │
└──────────────────────────┬──────────────────────────────┘
                           │
                           │ 直接读写（不依赖 Gateway）
                           ▼
                 ~/.openclaw/openclaw.json
```

## 3. 核心功能

### 3.1 场景库 (Recipes)

每个 Recipe 是一个"配置方案"，包含：
- 标题、描述、标签
- 需要用户填的参数
- 配置补丁模板

**示例 Recipe：Discord 频道专属人设**

```yaml
id: discord-channel-persona
name: "Discord 频道专属人设"
description: "给特定 Discord 频道注入专属 system prompt，让 Agent 在不同频道表现不同"
author: "zhixian"
version: "1.0.0"
tags: ["discord", "persona", "beginner"]
difficulty: "easy"

# 用户需要填的参数
params:
  - id: guild_id
    label: "服务器 ID"
    type: string
    placeholder: "右键服务器 → 复制服务器 ID"
    
  - id: channel_id
    label: "频道 ID"
    type: string
    placeholder: "右键频道 → 复制频道 ID"
    
  - id: persona
    label: "人设描述"
    type: textarea
    placeholder: "在这个频道里，你是一个..."

# 配置补丁（JSON Merge Patch 格式）
patch: |
  {
    "channels": {
      "discord": {
        "guilds": {
          "{{guild_id}}": {
            "channels": {
              "{{channel_id}}": {
                "systemPrompt": "{{persona}}"
              }
            }
          }
        }
      }
    }
  }
```

### 3.2 引导式安装流程

```
[选择场景] → [填写参数] → [预览变更] → [确认应用] → [完成]
     │            │            │            │
     │            │            │            └── 自动备份当前配置
     │            │            └── Diff 视图，清晰展示改了什么
     │            └── 表单 + 实时校验
     └── 卡片式浏览，带搜索/筛选
```

### 3.3 版本控制 & 回滚

```
~/.openclaw/
├── openclaw.json              # 当前配置
└── .clawpal/
    ├── history/
    │   ├── 2026-02-15T21-30-00_discord-persona.json
    │   ├── 2026-02-15T20-00-00_manual-edit.json
    │   └── 2026-02-14T15-00-00_performance-tuning.json
    └── metadata.json          # 历史记录元数据
```

**回滚流程**
1. 选择历史版本
2. 展示 Diff（当前 vs 目标版本）
3. 确认回滚
4. 当前版本也存入历史（防止误操作）

### 3.4 配置诊断 (Doctor)

当 Gateway 起不来时，ClawPal 可以独立运行诊断：

**检查项**
- [ ] JSON 语法是否正确
- [ ] 必填字段是否存在
- [ ] 字段类型是否正确
- [ ] 端口是否被占用
- [ ] 文件权限是否正确
- [ ] Token/密钥格式是否正确

**自动修复**
- 语法错误：尝试修复常见问题（尾逗号、引号）
- 缺失字段：填充默认值
- 格式错误：自动转换

## 4. 官网设计

### 4.1 首页

```
┌─────────────────────────────────────────────────────────┐
│                      ClawPal                            │
│         让 OpenClaw 配置变得简单                         │
│                                                         │
│              [下载 App]  [浏览 Recipes]                  │
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │            热门 Recipes                          │   │
│  │  ┌─────┐  ┌─────┐  ┌─────┐  ┌─────┐  ┌─────┐   │   │
│  │  │ 🎭  │  │ ⚡  │  │ 🔔  │  │ 🤖  │  │ 📝  │   │   │
│  │  │人设 │  │性能 │  │提醒 │  │模型 │  │日记 │   │   │
│  │  └─────┘  └─────┘  └─────┘  └─────┘  └─────┘   │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │            提交你的 Recipe                       │   │
│  │  分享你的最佳实践，帮助更多人                      │   │
│  │                    [提交]                        │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

### 4.2 Recipe 详情页

```
┌─────────────────────────────────────────────────────────┐
│  ← 返回                                                 │
│                                                         │
│  Discord 频道专属人设                           v1.0.0  │
│  by zhixian                                             │
│                                                         │
│  ⬇️ 1,234 安装   ⭐ 4.8 (56 评价)                       │
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │  给特定 Discord 频道注入专属 system prompt，     │   │
│  │  让 Agent 在不同频道表现不同。                   │   │
│  │                                                  │   │
│  │  适用场景：                                      │   │
│  │  • 工作频道严肃，闲聊频道轻松                    │   │
│  │  • 不同频道不同语言                              │   │
│  │  • 特定频道禁用某些功能                          │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  需要填写的参数：                                       │
│  • 服务器 ID                                           │
│  • 频道 ID                                             │
│  • 人设描述                                            │
│                                                         │
│              [在 ClawPal 中安装]                        │
│                                                         │
│  ─────────────────────────────────────────────────     │
│                                                         │
│  配置预览                                               │
│  ┌─────────────────────────────────────────────────┐   │
│  │ channels:                                        │   │
│  │   discord:                                       │   │
│  │     guilds:                                      │   │
│  │       "{{guild_id}}":                           │   │
│  │         channels:                                │   │
│  │           "{{channel_id}}":                     │   │
│  │             systemPrompt: "{{persona}}"         │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

### 4.3 Deep Link 协议

```
clawpal://install/{recipe-id}
clawpal://install/{recipe-id}?source=web&version=1.0.0
```

App 收到 deep link 后：
1. 下载 recipe 元数据
2. 打开安装向导
3. 引导用户填写参数
4. 应用配置

## 5. 技术栈

### 5.1 本地 App

```
ClawPal App (Tauri)
├── src-tauri/           # Rust 后端（轻量，主要用 Tauri API）
│   ├── src/
│   │   └── main.rs      # 入口 + 少量原生逻辑
│   └── tauri.conf.json  # Tauri 配置
│
└── src/                 # Web 前端
    ├── App.tsx
    ├── pages/
    │   ├── Home.tsx         # 首页 + 状态
    │   ├── Recipes.tsx      # 场景库
    │   ├── Install.tsx      # 安装向导
    │   ├── History.tsx      # 历史记录
    │   └── Doctor.tsx       # 诊断修复
    ├── components/
    │   ├── RecipeCard.tsx
    │   ├── ParamForm.tsx
    │   ├── DiffViewer.tsx
    │   └── ...
    └── lib/
        ├── config.ts        # 配置读写（用 Tauri fs API）
        ├── recipe.ts        # Recipe 解析/应用
        ├── backup.ts        # 版本控制
        └── doctor.ts        # 诊断逻辑
```

### 5.2 技术选型

| 组件 | 选型 | 理由 |
|------|------|------|
| App 框架 | Tauri 2.0 | 轻量(5-10MB)，JS 为主 |
| 前端框架 | React + TypeScript | 生态成熟 |
| UI 组件 | shadcn/ui | 好看，可定制 |
| 状态管理 | React Context + useReducer | 先用原生，后续再引入 Zustand |
| 配置解析 | json5 | 支持注释 |
| Diff 展示 | monaco-editor diff | 可控性强，定制成本低 |

### 5.3 RecipeEngine 核心接口

```typescript
interface RecipeEngine {
  // 校验 recipe 定义 + 用户参数
  validate(recipe: Recipe, params: Record<string, unknown>): ValidationResult;
  
  // 预览变更（不实际修改）
  preview(recipe: Recipe, params: Record<string, unknown>): PreviewResult;
  
  // 应用配置（自动备份）
  apply(recipe: Recipe, params: Record<string, unknown>): ApplyResult;
  
  // 回滚到指定快照
  rollback(snapshotId: string): RollbackResult;
  
  // 从损坏状态恢复
  recover(): RecoverResult;
}

interface PreviewResult {
  diff: string;                    // 配置 Diff
  impactLevel: 'low' | 'medium' | 'high';  // 影响级别
  affectedPaths: string[];         // 受影响的配置路径
  canRollback: boolean;            // 是否可回滚
  overwritesExisting: boolean;     // 是否覆盖现有配置
  warnings: string[];              // 警告信息
}
```

### 5.3 官网

| 组件 | 选型 | 理由 |
|------|------|------|
| 框架 | Next.js | SSR/SSG，SEO 友好 |
| 部署 | Vercel / Cloudflare Pages | 免费，CDN |
| 数据库 | Supabase / PlanetScale | Recipe 存储 |
| 认证 | GitHub OAuth | 用户提交 recipe |

## 6. MVP 范围（精简版）

> 先做 3 个高价值核心功能，离线可用，快速验证

### MVP 核心功能

#### 1. 安装向导
- [ ] 参数校验（schema 验证）
- [ ] 变更预览（Diff 视图）
- [ ] 应用配置
- [ ] 自动备份

#### 2. 版本快照与回滚
- [ ] 每次修改前自动快照
- [ ] 历史记录列表
- [ ] 一键回滚
- [ ] 回滚前预览 Diff

#### 3. 配置诊断
- [ ] JSON 语法检查
- [ ] 必填字段验证
- [ ] 端口占用检测
- [ ] 文件权限检查
- [ ] 一键修复 + 显示变更原因

### MVP 不做的事
- ❌ 官网
- ❌ 用户系统 / OAuth
- ❌ 评分/评论体系
- ❌ 在线 Recipe 仓库

### 后续阶段
- Phase 2: 官网 + Recipe 在线分发
- Phase 3: 社区功能（评分、评论、用户提交）

## 7. 初始 Recipe 列表

MVP 内置的 Recipes：

1. **Discord 频道专属人设** — 不同频道不同性格
2. **Telegram 群组配置** — 群聊 mention 规则
3. **定时任务配置** — Heartbeat + Cron 基础设置
4. **模型切换** — 快速切换默认模型
5. **性能优化** — contextPruning + compaction 最佳实践

---

## 8. 风险点 & 注意事项

### 8.1 Schema 版本兼容
- OpenClaw 配置 schema 会随版本变化
- 需要锁定版本兼容层（v1/v2 schema migration）
- Recipe 需标注兼容的 OpenClaw 版本范围

### 8.2 安全性
- **深度链接可信源校验**：防止恶意 recipe 写入本地配置
- **敏感路径白名单**：限制 recipe 可修改的配置路径
- **危险操作提醒**：涉及 token、密钥、敏感路径时 must-have 确认

### 8.3 平台兼容
- Tauri 2.0 在 Windows/macOS 路径权限表现有差异
- 需要测试不同平台的文件读写行为
- 路径处理使用 Tauri 的跨平台 API

### 8.4 WSL2 支持（Windows 重点）

很多 Windows 用户通过 WSL2 安装 OpenClaw，配置文件在 Linux 文件系统里。

**检测逻辑**
1. 检查 Windows 原生路径 `%USERPROFILE%\.openclaw\`
2. 如果不存在，扫描 `\\wsl$\*\home\*\.openclaw\`
3. 找到多个时让用户选择

**路径映射**
```
WSL2 路径:     /home/user/.openclaw/openclaw.json
Windows 访问:  \\wsl$\Ubuntu\home\user\.openclaw\openclaw.json
```

**UI 处理**
- 首次启动检测安装方式
- 设置页可手动切换/指定路径
- 显示当前使用的路径来源（Windows / WSL2-Ubuntu / 自定义）

### 8.5 JSON5 风格保持
- 用户手写的注释和缩进不能被破坏
- 写回时需保持原有格式风格
- 考虑使用 AST 级别的修改而非 stringify

---

## 9. Recipe 校验规则

### 9.1 参数 Schema
```yaml
params:
  - id: guild_id
    type: string
    required: true
    pattern: "^[0-9]+$"           # 正则校验
    minLength: 17
    maxLength: 20
```

### 9.2 路径白名单
```yaml
# 只允许修改这些路径
allowedPaths:
  - "channels.*"
  - "agents.defaults.*"
  - "agents.list[*].identity"
  
# 禁止修改
forbiddenPaths:
  - "gateway.auth.*"              # 认证相关
  - "*.token"                     # 所有 token
  - "*.apiKey"                    # 所有 API key
```

### 9.3 危险操作标记
```yaml
dangerousOperations:
  - path: "gateway.port"
    reason: "修改端口可能导致连接中断"
    requireConfirm: true
  - path: "channels.*.enabled"
    reason: "禁用频道会影响消息收发"
    requireConfirm: true
```

---

## 10. 体验细节

### 10.1 影响级别展示
安装按钮显示"预估影响级别"：

| 级别 | 条件 | 展示 |
|------|------|------|
| 🟢 低 | 只添加新配置，不修改现有 | "添加新配置" |
| 🟡 中 | 修改现有配置，可回滚 | "修改配置（可回滚）" |
| 🔴 高 | 涉及敏感路径或大范围修改 | "重要变更（请仔细检查）" |

### 10.2 可回滚提示
每个 Recipe 显示：
- ✅ 可回滚 / ⚠️ 部分可回滚 / ❌ 不可回滚
- 是否会覆盖现有配置（高亮显示冲突项）

### 10.3 历史记录增强
- 关键词筛选
- 仅显示可回滚节点
- 按 Recipe 类型分组

### 10.4 Doctor 一键修复
```
发现 2 个问题：

1. ❌ JSON 语法错误（第 42 行）
   → 多余的逗号
   [一键修复] 删除第 42 行末尾的逗号

2. ❌ 必填字段缺失
   → agents.defaults.workspace 未设置
   [一键修复] 设置为默认值 "~/.openclaw/workspace"

[全部修复] [仅修复语法] [查看变更详情]
```

---

## 11. 落地步骤（推荐顺序）

### Step 1: RecipeEngine 核心
1. 定义 RecipeEngine 接口
2. 实现 `validate` → `preview` → `apply` → `rollback` → `recover`
3. 编写单元测试

### Step 2: 端到端流程验证
1. 实现一个真实 Recipe（Discord 人设）
2. 完整走通：选择 → 填参数 → 预览 → 应用 → 回滚
3. 验证 JSON5 风格保持

### Step 3: 损坏恢复演练
1. 模拟配置损坏场景
2. 测试 Doctor 诊断流程
3. 验证一键修复功能

### Step 4: 扩展 & 发布
1. 添加 2-3 个 Recipe
2. 完善 UI 细节
3. 打包发布（macOS / Windows / Linux）

---

## 附录

### A. 隐藏但有用的配置能力

这些是 OpenClaw 支持但用户不一定知道的功能：

| 功能 | 配置路径 | 说明 |
|------|----------|------|
| Channel 级 systemPrompt | `channels.*.guilds.*.channels.*.systemPrompt` | 频道专属人设 |
| Context Pruning | `agents.defaults.contextPruning` | 上下文裁剪策略 |
| Compaction | `agents.defaults.compaction` | Session 压缩 |
| Bindings | `bindings[]` | 按条件路由到不同 Agent |
| Media Audio | `tools.media.audio` | 语音转录配置 |
| Memory Search | `agents.defaults.memorySearch` | 记忆搜索配置 |

### B. 文件路径

| 文件 | 路径 |
|------|------|
| OpenClaw 配置 | `~/.openclaw/openclaw.json` |
| ClawPal 历史 | `~/.openclaw/.clawpal/history/` |
| ClawPal 元数据 | `~/.openclaw/.clawpal/metadata.json` |

---

*Last updated: 2026-02-15*
