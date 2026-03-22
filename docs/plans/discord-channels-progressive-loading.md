# Plan: Discord Channels 页面渐进式加载

## 问题

当前 Channels 页面 Discord 区域的加载体验差：

1. 用户进入 Channels 页，`refreshDiscordChannelsCache()` 触发后端 `refresh_discord_guild_channels()`
2. 后端串行执行：**解析 config → Discord REST 获取缺失频道 → CLI `channels resolve` 获取频道名 → REST 获取 guild 名**
3. 整个管线完成前 (~2-5s，remote 更慢)，UI 只显示一行 `"Loading Discord..."`
4. 用户看到空白等待，无法预知有多少内容、何时完成

## 目标

**先展示结构，再补充细节。** 用户进入页面后立刻看到 guild/channel 列表骨架，每个 item 带加载状态（"获取中..."），Discord 数据到达后逐步补充名称。

## 方案

### Phase 1: 快速列表（Backend）

复用 `feat/recipe-import-library` 分支已有的 `list_discord_guild_channels_fast` 思路（仅解析 config + 读取磁盘缓存，不调 Discord REST / CLI）。

> **注意**: 该函数在 `feat/recipe-import-library` 分支中，尚未合入 `develop`。此 PR 需自己实现或等 #118 合入后 rebase。

新增/调整后端命令：

| 命令 | 行为 | 耗时 |
|------|------|------|
| `list_discord_guild_channels_fast` | 解析 config + 读取 `discord-guild-channels.json` 缓存 | <50ms |
| `remote_list_discord_guild_channels_fast` | SSH 读取 remote config + 缓存文件 | <500ms |
| `refresh_discord_guild_channels` (现有) | 完整解析 + REST + CLI，写入缓存 | 2-5s |

**`_fast` 返回数据特点：**
- guild/channel ID 始终可用（来自 config 和 bindings）
- guild/channel 名称**可能是 ID**（缓存中没有的）
- 每个 entry 附带 `nameResolved: bool` 标记名称是否已解析

### Phase 2: 前端分层加载

#### 2a. `App.tsx` 新增快速预加载

```
进入 channels 路由 → 并发触发:
  ├─ refreshDiscordChannelsCacheFast()  → 立即更新 state (< 50ms)
  └─ refreshDiscordChannelsCache()      → 到达后覆盖 state (2-5s)
```

新增 `InstanceContext` 字段：

```typescript
interface InstanceContextValue {
  // 现有
  discordGuildChannels: DiscordGuildChannel[] | null;
  discordChannelsLoading: boolean;
  // 新增
  discordChannelsResolved: boolean;  // 名称是否全部解析完毕
}
```

#### 2b. `Channels.tsx` 渐进式 UI

**Stage 0 — 首次进入（无缓存）:**
```
┌─────────────────────────────────┐
│ Discord                [Refresh]│
│ Loading Discord...              │  ← 现有行为，保留
└─────────────────────────────────┘
```

**Stage 1 — fast 数据到达（< 50ms）:**
```
┌─────────────────────────────────┐
│ Discord                [Refresh]│
│                                 │
│ ┌ Guild: 12345678901234 ⟳ ───┐ │  ← guild 名未解析，显示 ID + spinner
│ │  #1098765432101234  ⟳      │ │  ← channel 名未解析
│ │  #general                   │ │  ← 缓存命中，名称已知
│ │  #1098765432109999  ⟳      │ │
│ └────────────────────────────┘ │
│                                 │
│ ┌ Guild: My Server ──────────┐ │  ← config 里有 slug/name
│ │  #bot-test                  │ │
│ │  #1098765432105555  ⟳      │ │
│ └────────────────────────────┘ │
└─────────────────────────────────┘
```

**Stage 2 — full 数据到达（2-5s）:**
```
┌─────────────────────────────────┐
│ Discord                [Refresh]│
│                                 │
│ ┌ Guild: OpenClaw Community ──┐ │  ← guild 名已解析
│ │  #general                   │ │
│ │  #bot-commands              │ │  ← 所有名称补全
│ │  #announcements             │ │
│ └────────────────────────────┘ │
│                                 │
│ ┌ Guild: My Server ──────────┐ │
│ │  #bot-test                  │ │
│ │  #dev-chat                  │ │
│ └────────────────────────────┘ │
└─────────────────────────────────┘
```

#### 2c. UI 组件细节

**未解析的 guild/channel 名称:**
```tsx
<span className="text-sm font-medium">
  {guild.guildName}
  {!discordChannelsResolved && guild.guildName === guild.guildId && (
    <Loader2 className="ml-1.5 inline h-3 w-3 animate-spin text-muted-foreground" />
  )}
</span>
```

**未解析的 channel 名称:**
```tsx
<div className="text-sm font-medium">
  {ch.channelName === ch.channelId ? (
    <span className="text-muted-foreground font-mono text-xs">
      {ch.channelId}
      <Loader2 className="ml-1 inline h-3 w-3 animate-spin" />
    </span>
  ) : (
    ch.channelName
  )}
</div>
```

### Phase 3: Agent Select 同步优化

`Channels.tsx` 里的 agent 下拉列表来自 `getChannelsRuntimeSnapshot()`，也需要等待。优化：

1. Agent 列表从 `readPersistedReadCache("listAgents", [])` 初始化（与 ParamForm 同理）
2. `getChannelsRuntimeSnapshot()` 到达后覆盖

## 改动范围预估

| 文件 | 改动类型 | 预估行数 |
|------|----------|----------|
| `src-tauri/src/commands/discovery.rs` | 新增 `_fast` 命令（如果基于 develop） | +60 |
| `src-tauri/src/lib.rs` | 注册新命令 | +4 |
| `src/lib/api.ts` | 新增 `_fast` 前端 API | +10 |
| `src/lib/instance-context.tsx` | 新增 `discordChannelsResolved` | +3 |
| `src/lib/use-api.ts` | 新增 `_fast` dispatchCached | +10 |
| `src/App.tsx` | 快速预加载 + resolved 状态 | +20 |
| `src/pages/Channels.tsx` | 渐进式 UI + spinner | +30 |
| `src/pages/__tests__/Channels.test.tsx` | 测试更新 | +10 |
| **总计** | | **~+150** |

## 依赖关系

- **选项 A**: 等 PR #118 (`feat/recipe-import-library`) 合入 `develop` 后基于 `develop` 开发。`_fast` 后端 + `discordChannelsResolved` context 已实现，直接复用。
- **选项 B**: 直接基于 `develop` 重新实现 `_fast` 后端。代码量不大（~60 行）。

**建议选 A**，避免重复工作。

## 不在此 PR 范围

- 其他平台（Telegram/Feishu/QBot）的渐进加载 — 它们不走 Discord REST，当前加载已足够快
- Channel/Guild 缓存的 TTL 策略调整 — 保持现有行为
- Discord REST 并发优化（多 guild 并行获取）— 可后续单独做
