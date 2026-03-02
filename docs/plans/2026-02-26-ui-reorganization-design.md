# ClawPal UI Reorganization Design

## Background

ClawPal is integrating zeroclaw as a built-in runtime. Existing features are becoming zeroclaw-driven, with the app shifting to a data/tool support role. This requires a UI reorganization to reflect the new architecture.

## Core Problems

1. **Start tab overloaded**: 4 sidebar nav items (Install / Connect / Profiles / Settings) make it a mini-app instead of a welcome gateway. Install and Connect are split but represent the same user intent.
2. **Instance Dashboard is a flat dump**: Status, Agents, Recipes, Backups stacked vertically with no hierarchy. No way to see all instances' health at a glance.
3. **Tab bar conflates workspace with registry**: Removing a tab means deleting the instance. No lightweight "close" operation.
4. **Sidebar context switching is jarring**: Same sidebar shows completely different nav items in Start vs Instance mode.
5. **Chat button appears everywhere**: Including Start page where it has no purpose.

## Design Decisions

### 1. Tab Bar: From Registry to Workspace

**Current model**: Tab bar = instance registry. Tab exists = instance exists. Remove tab = delete instance.

**New model**: Tab bar = open workspace (like browser tabs).

- **Start tab**: Fixed leftmost, not closeable. Separator between Start and instance tabs (keep current `border-r` design).
- **Instance tabs**: Each has a `×` close button (visible on hover). Closing = remove from workspace only, instance data untouched.
- **Status dots**: Retained. Green = healthy, red = error, gray = offline.
- **No more `⋯` popover menu on tabs**: Rename/delete operations move to Start page instance cards. Tabs only show name + status dot + close button.
- **Auto-navigate to Start**: When all instance tabs are closed, switch to Start.
- **SSH/Docker management dialogs removed from InstanceTabBar**: All instance CRUD operations move to Start page.

```
┌─────────────────────────────────────────────────────────────┐
│ [Start]  ║  [● Local  ×] [● Docker Local  ×] [○ VPS-1  ×]  │
└─────────────────────────────────────────────────────────────┘
```

### 2. Start Page: Welcome Gateway

Start page becomes a single-page dashboard with instance cards and quick actions. No sub-navigation.

```
┌─────────────────────────────────────────────────┐
│  sidebar(220px)  │      main content            │
│                  │                               │
│  [logo] ClawPal  │  Welcome heading / subtitle   │
│                  │                               │
│  ─────────────   │  ┌──────┐ ┌──────┐ ┌──────┐  │
│  Profiles        │  │Local │ │Docker│ │SSH-1 │  │
│  Settings        │  │  ●   │ │  ●   │ │  ○   │  │
│                  │  │健康  │ │健康  │ │离线  │  │
│                  │  │2 agt │ │1 agt │ │      │  │
│                  │  └──────┘ └──────┘ └──────┘  │
│                  │                               │
│                  │  ┌ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┐  │
│                  │  │  + New / Connect Instance│  │
│                  │  └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘  │
│                  │                               │
│  ─────────────   │                               │
│  website · @zhx  │                               │
└─────────────────────────────────────────────────┘
```

**Instance cards**:
- Show: name, type icon (local/Docker/SSH), health status dot, agent count
- Click card → open instance in tab bar (or switch to existing tab)
- `⋯` menu on card: Rename / Edit (SSH config) / Delete
- Subtle "opened" indicator for instances already in tab bar

**"+ New / Connect Instance" card**:
- Dashed border placeholder card
- Click → opens InstallHub as Dialog
- Single unified flow (no install/connect mode split)

**Sidebar (Start mode)**:
- Only: Profiles, Settings
- No PendingChangesBar (not relevant in Start context)
- Bottom: website / @author links

### 3. InstallHub: Unified Setup Flow via A2UI

InstallHub merges install and connect into a single flow. Presented as a Dialog overlay, not embedded in main content. Uses A2UI pattern (structured UI elements from zeroclaw) instead of chat.

```
┌──────────────────────────────────────────┐
│  Set Up Instance                     [×] │
│                                          │
│  Describe your target environment        │
│  ┌────────────────────────────────────┐  │
│  │                                    │  │
│  └────────────────────────────────────┘  │
│                                          │
│  Quick options:                          │
│  [Local] [Docker] [Remote SSH] [Connect] │
│                                          │
│  ──────────────────────────────────────  │
│                                          │
│  (after zeroclaw decides target)         │
│                                          │
│  ● precheck ✓                            │
│  ● install  ✓                            │
│  ● init     → running...                 │
│  ○ verify                                │
│                                          │
│  ┌─ Blocker ───────────────────────┐     │
│  │ No API key detected             │     │
│  │ [Configure Profiles] [Retry]    │     │
│  └─────────────────────────────────┘     │
│                                          │
│                            [Cancel]      │
└──────────────────────────────────────────┘
```

**Key changes from current InstallHub**:
- No install/connect mode split: user describes intent, zeroclaw auto-determines the approach
- Quick option chips replace the dual mode switch + textarea hints
- Minimal stepper for the 4 steps; details only expand on error
- Blocker recovery inline: action buttons in the dialog (jump to Profiles, jump to Doctor)
- **Install Copilot chat removed**: zeroclaw communicates via A2UI — stepper states, blocker cards, action buttons. More structured and predictable than chat bubbles.
- On completion: Dialog closes, instance card appears on Start page, auto-opens in tab bar

### 4. Instance Home (Simplified)

The current Home page is simplified. Recipes, Backups, and the embedded InstallHub are removed. What remains is a focused config panel.

```
┌──────────────────────────────────────────────┐
│                                              │
│  Instance Name · ● Healthy      [Upgrade ↗] │
│  v0.8.2 · openclaw                           │
│                                              │
│  ─────────────────────────────────────────── │
│                                              │
│  Default Model  [openrouter/claude-sonnet ▾] │
│  Fallback       [anthropic/haiku] [+ Add]    │
│                                              │
│  ─────────────────────────────────────────── │
│                                              │
│  Agents                           [+ New]    │
│  ┌─────────────────────────────────────────┐ │
│  │ main  · claude-sonnet · ● active        │ │
│  │ sub-1 · gpt-4o        · ○ idle          │ │
│  └─────────────────────────────────────────┘ │
│                                              │
└──────────────────────────────────────────────┘
```

**Retained**: Instance status header (name, health, version, upgrade entry), default/fallback model selection, agent list with per-agent model select.

**Moved out**: Recipes → stays as sidebar nav. Backups → Doctor page. InstallHub → Start page.

### 5. Sidebar Navigation (Instance Mode)

```
- Home          (status + model config + agent management)
- Channels
- Recipes
- Cron
- History
- Doctor        (health diagnostics, sessions, backups, logs)
── separator ──
- PendingChangesBar
```

Changes from current:
- Home added as explicit landing page
- Sessions merged into Doctor (both are operational)
- Backups merged into Doctor
- Recipes retained as standalone nav item

### 6. Chat Panel Scoping

- **Start mode**: Chat button hidden. No Chat panel.
- **Instance mode**: Chat button in top-right (current floating pill). Opens 380px right panel for talking to the instance's openclaw agent.
- **zeroclaw-driven features** (Doctor diagnosis, install flow): Use their own embedded A2UI interaction. Do not use the right Chat panel.

Right Chat panel is exclusively for openclaw agent conversation within an instance context.

## Migration Summary

| Current | New Location |
|---------|-------------|
| InstanceTabBar SSH/Docker dialogs | Start page instance card menus |
| Start sidebar: Install nav | Start page "+ New/Connect" card → Dialog |
| Start sidebar: Connect nav | Merged into above |
| Start sidebar: Profiles | Start sidebar (retained) |
| Start sidebar: Settings | Start sidebar (retained) |
| Home (controlMode=true) | Start page |
| Home (controlMode=false) | Instance Home (simplified) |
| InstallHub (embedded) | Dialog overlay from Start page |
| Install Copilot chat | Removed; replaced by A2UI stepper + blockers |
| Dashboard: Recipes section | Sidebar nav → Recipes page |
| Dashboard: Backups section | Doctor page |
| Dashboard: Status card | Instance Home header |
| Sessions page (sidebar nav) | Doctor page (sub-section) |
| Chat button (all pages) | Instance mode only |

## Component Impact

| Component | Change |
|-----------|--------|
| `App.tsx` | Route changes, Start mode sidebar simplification, Chat visibility scoping |
| `InstanceTabBar.tsx` | Add close buttons, remove `⋯` menus, remove all SSH/Docker dialogs |
| `InstallHub.tsx` | Merge install/connect modes, wrap in Dialog, remove copilot chat, implement A2UI stepper |
| `Home.tsx` | Remove controlMode branch, simplify to status + models + agents only |
| `Doctor.tsx` | Absorb Sessions and Backups sections |
| `Sessions.tsx` | Merge into Doctor or keep as Doctor sub-component |
| New: `StartPage.tsx` | Instance card grid, health overview, "+ new" entry point |
| New: `InstanceCard.tsx` | Card component with status, agent count, actions menu |
