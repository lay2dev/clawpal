# UI Reorganization Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Reorganize ClawPal's UI from a tab-as-registry model to a workspace model with a welcome gateway Start page, simplified instance Home, and Dialog-based InstallHub.

**Architecture:** Start page becomes an instance card grid (new `StartPage.tsx` + `InstanceCard.tsx`). Tab bar gets close buttons and loses all CRUD dialogs. Home.tsx drops controlMode/Recipes/Backups to become a focused config panel. Sessions/Backups merge into Doctor. InstallHub wraps in a Dialog, merges install/connect modes, removes copilot chat.

**Tech Stack:** React 18, TypeScript, Tailwind CSS v4, Radix UI primitives (shadcn/ui-style), i18next, Tauri v2 IPC via `useApi()` hook. No test framework — verify with `tsc --noEmit`.

**Design doc:** `docs/plans/2026-02-26-ui-reorganization-design.md`

---

## Task 1: InstanceCard component

**Files:**
- Create: `src/components/InstanceCard.tsx`
- Modify: `src/locales/en.json` (add `start.*` keys)
- Modify: `src/locales/zh.json` (add `start.*` keys)

A card showing one instance's name, type icon, health status, agent count, "opened" indicator, and a `⋯` actions menu. This is a pure presentational component.

**Step 1: Create InstanceCard.tsx**

```tsx
// src/components/InstanceCard.tsx
import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  MoreHorizontalIcon,
  MonitorIcon,
  ContainerIcon,
  ServerIcon,
} from "lucide-react";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";

type InstanceType = "local" | "docker" | "ssh";

interface InstanceCardProps {
  id: string;
  label: string;
  type: InstanceType;
  healthy: boolean | null; // null = unknown/loading
  agentCount: number;
  opened: boolean; // whether this instance is currently open in tab bar
  connectionStatus?: "connected" | "disconnected" | "error";
  onClick: () => void;
  onRename?: () => void;
  onEdit?: () => void;   // SSH edit
  onDelete?: () => void;
}
```

The component renders:
- A `Card` with `onClick` → opens/switches to instance in tab bar
- Top-left: type icon (`MonitorIcon` for local, `ContainerIcon` for docker, `ServerIcon` for ssh)
- Top-right: `⋯` Popover menu (Rename / Edit / Delete actions; Edit only for SSH type)
- Center: label text
- Bottom row: health status dot + health text, agent count badge
- Subtle border highlight (`border-primary/30`) when `opened === true`

Use the project's existing pattern: named export, `cn()` for classnames, `useTranslation()` for strings, Tailwind utility classes, warm design tokens.

**Step 2: Add locale keys**

In `en.json`, add under existing `start.*` namespace:
```json
"start.welcome": "Welcome to ClawPal",
"start.welcomeHint": "Select an instance to manage, or set up a new one.",
"start.addInstance": "New / Connect Instance",
"start.addInstanceHint": "Set up a new OpenClaw instance or connect an existing one",
"start.healthy": "Healthy",
"start.unhealthy": "Unhealthy",
"start.checking": "Checking...",
"start.offline": "Offline",
"start.agents_zero": "No agents",
"start.agents_one": "{{count}} agent",
"start.agents_other": "{{count}} agents",
"start.opened": "Open",
"start.menuRename": "Rename",
"start.menuEdit": "Edit",
"start.menuDelete": "Delete"
```

Add matching Chinese translations in `zh.json`.

**Step 3: Typecheck**

Run: `cd /Users/zhixian/Codes/clawpal && npx tsc --noEmit`
Expected: No errors.

**Step 4: Commit**

```bash
git add src/components/InstanceCard.tsx src/locales/en.json src/locales/zh.json
git commit -m "feat: add InstanceCard component for Start page"
```

---

## Task 2: Merge Sessions and Backups into Doctor page

**Files:**
- Modify: `src/pages/Doctor.tsx`
- Modify: `src/locales/en.json` (add section headers if needed)
- Modify: `src/locales/zh.json`

Currently Doctor.tsx has only the diagnosis agent card + logs dialog. Sessions.tsx is a thin wrapper around `SessionAnalysisPanel`. Backups logic currently lives in Home.tsx.

**Step 1: Add Sessions section to Doctor**

Import `SessionAnalysisPanel` from `src/components/SessionAnalysisPanel.tsx` and render it below the existing diagnosis card, wrapped in a collapsible section:

```tsx
// Inside Doctor.tsx, after the main diagnosis Card:
<div className="mt-8">
  <h3 className="text-lg font-semibold mb-4">{t("doctor.sessions")}</h3>
  <SessionAnalysisPanel />
</div>
```

**Step 2: Extract Backups section from Home.tsx into Doctor**

Move the backup listing + create/restore/delete logic from Home.tsx (lines ~777-895) into Doctor.tsx as a new section below Sessions. This includes:
- `backups` state, `refreshBackups` callback
- `backingUp`, `backupMessage` state
- The backup card list with Show/Restore/Delete AlertDialogs
- The "Create Backup" button

Wrap in a section:
```tsx
<div className="mt-8">
  <div className="flex items-center justify-between mb-4">
    <h3 className="text-lg font-semibold">{t("doctor.backups")}</h3>
    <Button size="sm" variant="outline" ...>
      {t("home.createBackup")}
    </Button>
  </div>
  {/* backup list */}
</div>
```

Reuse existing locale keys (`doctor.backups`, `home.createBackup`, etc.) — no need to create new ones since they already exist.

**Step 3: Typecheck**

Run: `npx tsc --noEmit`
Expected: No errors.

**Step 4: Commit**

```bash
git add src/pages/Doctor.tsx
git commit -m "feat: merge Sessions and Backups sections into Doctor page"
```

---

## Task 3: Simplify Home.tsx → Instance Home

**Files:**
- Modify: `src/pages/Home.tsx`

Remove the `controlMode` branch entirely (this was the Start page delegation). Remove Recipes section and Backups section (moved to sidebar nav and Doctor respectively). Keep only: status header, model config, agent management.

**Step 1: Remove controlMode branch**

Delete the `controlMode` prop and the entire `controlMode ? (...) : (...)` conditional. The component now always renders the instance dashboard.

Remove these props entirely: `controlMode`, `startSection`, `onStartSectionChange`, `onInstallReady`, `onRequestAddSsh`, `onConnectTarget`.

Remove the `InstallHub` import and its embedded rendering.
Remove the `Settings` import (was used for controlMode Profiles/Settings delegation).

**Step 2: Remove Recipes section**

Delete the "Recommended Recipes" `<h3>` + `RecipeCard` grid + the `recipes` state and its `useEffect` fetch.
Remove the `RecipeCard` import, `onCook` prop.

**Step 3: Remove Backups section**

Delete the "Backups" section (now in Doctor). Remove `backups` state, `refreshBackups`, `backingUp`, `backupMessage` and all backup-related JSX.

**Step 4: Clean up imports**

Remove now-unused imports: `InstallHub`, `Settings`, `RecipeCard`, `BackupInfo`, `InstallSession`, and any other types/components that are no longer referenced.

**Step 5: Add instance name to status header**

The status card header currently shows just "Status". Enhance it to show the instance identity:
- For local: "Local · ● Healthy" + version
- For docker: docker label from context
- For SSH: SSH host label

This requires reading the instance label. The component already has access to `useApi()` which provides `instanceId`. For the display name, accept a new optional `instanceLabel` prop from App.tsx.

**Step 6: Typecheck**

Run: `npx tsc --noEmit`
Expected: Errors in App.tsx (references removed props). These will be fixed in Task 6.

**Step 7: Commit**

```bash
git add src/pages/Home.tsx
git commit -m "feat: simplify Home to instance config panel (status + models + agents)"
```

---

## Task 4: Refactor InstallHub into Dialog

**Files:**
- Modify: `src/components/InstallHub.tsx`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh.json`

Wrap InstallHub in a Dialog. Merge install/connect into a single flow. Remove copilot chat. Implement A2UI stepper for progress.

**Step 1: Add Dialog wrapper**

Add `open` and `onOpenChange` props to `InstallHub`. Wrap the entire component body in `<Dialog open={open} onOpenChange={onOpenChange}>`. Use `<DialogContent className="max-w-lg max-h-[80vh] overflow-y-auto">`.

```tsx
export function InstallHub({
  open,
  onOpenChange,
  showToast,
  onNavigate,
  onReady,
  onRequestAddSsh,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  showToast?: ...;
  onNavigate?: ...;
  onReady?: ...;
  onRequestAddSsh?: ...;
}) {
```

**Step 2: Merge install/connect modes**

Remove `launcherMode` state and `forcedLauncherMode` / `hideLauncherModeSwitch` props. Instead of two mode tabs, present a single intent input with unified hint chips:

```tsx
const INTENT_HINTS = ["本机", "Docker", "远程 SSH", "连接已有实例"];
```

The backend's `installDecideTarget` already handles both install and connect semantics based on the intent string — no frontend mode split needed.

Remove the `onConnectTarget` prop and its associated connect-mode logic.

**Step 3: Remove copilot chat**

Delete `copilotMessages` state, `appendCopilotMessage` callback, and the "Install Copilot" chat panel JSX. Replace with an A2UI stepper:

```tsx
{/* A2UI Stepper */}
{session && (
  <div className="space-y-2 mt-4">
    {(["precheck", "install", "init", "verify"] as const).map((step) => {
      const state = stepState(session, step); // "done" | "running" | "failed" | "pending"
      return (
        <div key={step} className="flex items-center gap-2.5">
          <StepIcon state={state} />
          <span className={cn(
            "text-sm",
            state === "running" && "font-medium text-primary",
            state === "failed" && "text-destructive",
            state === "pending" && "text-muted-foreground",
          )}>
            {t(`home.install.step.${step}`)}
          </span>
          {state === "running" && (
            <span className="text-xs text-muted-foreground animate-pulse">
              {t("home.install.running")}
            </span>
          )}
        </div>
      );
    })}
  </div>
)}
```

Helper `StepIcon` renders a small circle: green check for done, spinning for running, red X for failed, gray for pending.

Helper `stepState` derives state from `session.state` and `session.current_step`.

**Step 4: Simplify blocker recovery**

Keep `autoBlocker` logic but simplify the UI. Instead of showing blocker codes and technical details, show a Card with:
- Plain message (from `autoBlocker.message`)
- Action buttons based on `autoBlocker.actions`:
  - `"settings"` → Button "Configure Profiles" → `onNavigate("settings")`
  - `"doctor"` → Button "Open Doctor" → `onNavigate("doctor")`
  - `"instances"` → Button "Manage Instances" → close dialog
  - `"resume"` → Button "Retry" → resume auto-install

Expandable details only if `autoBlocker.details` exists (collapsed by default).

**Step 5: Completion behavior**

When `session.state === "ready"`:
- Show success message with two action buttons: "Configure API & models" and "Configure channels"
- On action click or dialog close: `onOpenChange(false)`, then `onReady(session)`
- The parent (StartPage) handles adding the instance to tab bar

**Step 6: Add locale keys**

```json
"home.install.setupTitle": "Set Up Instance",
"home.install.setupDesc": "Describe where you want to install or connect OpenClaw.",
"home.install.intentHints": "Quick options:",
"home.install.stepDone": "Done",
"home.install.stepRunning": "Running...",
"home.install.stepFailed": "Failed",
"home.install.stepPending": "Pending"
```

Add Chinese translations.

**Step 7: Typecheck**

Run: `npx tsc --noEmit`
Expected: Errors in files that still reference old InstallHub props. Will be fixed in Task 6.

**Step 8: Commit**

```bash
git add src/components/InstallHub.tsx src/locales/en.json src/locales/zh.json
git commit -m "feat: refactor InstallHub into Dialog with unified flow and A2UI stepper"
```

---

## Task 5: Create StartPage component

**Files:**
- Create: `src/pages/StartPage.tsx`

The Start page renders: welcome heading, instance card grid (using `InstanceCard`), "+ New/Connect" placeholder card, and the InstallHub Dialog.

**Step 1: Create StartPage.tsx**

```tsx
// src/pages/StartPage.tsx
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { PlusIcon } from "lucide-react";
import { InstanceCard } from "@/components/InstanceCard";
import { InstallHub } from "@/components/InstallHub";
import { api } from "@/lib/api";
import type { DockerInstance, SshHost, InstallSession } from "@/lib/types";

interface InstanceHealthInfo {
  id: string;
  healthy: boolean | null;
  agentCount: number;
}

interface StartPageProps {
  // Instance registry (all known instances)
  dockerInstances: DockerInstance[];
  sshHosts: SshHost[];
  connectionStatus: Record<string, "connected" | "disconnected" | "error">;
  // Which instances are currently open in tab bar
  openTabIds: Set<string>;
  // Actions
  onOpenInstance: (id: string) => void;
  onRenameDocker: (id: string, label: string) => void;
  onDeleteDocker: (instance: DockerInstance, deleteData: boolean) => Promise<void>;
  onDeleteSsh: (hostId: string) => void;
  onEditSsh: (host: SshHost) => void;
  onInstallReady: (session: InstallSession) => void;
  onRequestAddSsh: () => void;
  showToast: (message: string, type?: "success" | "error") => void;
  onNavigate: (route: string) => void;
}
```

The component:
1. On mount, polls health for each known instance (local + docker + ssh) via `getInstanceStatus()`. Stores results in a `Map<string, InstanceHealthInfo>`.
2. Renders a grid of `InstanceCard` components — one per known instance.
3. Renders a dashed-border "+ New/Connect" card at the end.
4. Manages `installDialogOpen` state. Clicking the "+" card opens InstallHub as Dialog.
5. SSH rename/edit/delete dialogs are here (moved from InstanceTabBar). Import the existing `Dialog` patterns.

For Docker rename: reuse the same pattern as current InstanceTabBar's `dockerRenameOpen` dialog.
For Docker delete: reuse the same pattern as current InstanceTabBar's `dockerDeleteOpen` dialog.
For SSH edit: reuse the same pattern as current InstanceTabBar's SSH form dialog.
For SSH delete: reuse the same pattern as current InstanceTabBar's SSH delete confirm dialog.

**Step 2: Typecheck**

Run: `npx tsc --noEmit`

**Step 3: Commit**

```bash
git add src/pages/StartPage.tsx
git commit -m "feat: add StartPage with instance card grid and InstallHub dialog"
```

---

## Task 6: Refactor InstanceTabBar

**Files:**
- Modify: `src/components/InstanceTabBar.tsx`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh.json`

Slim down InstanceTabBar to only show/switch/close tabs. Remove all dialogs and `⋯` menus.

**Step 1: Add close and new callback props**

```tsx
interface InstanceTabBarProps {
  openTabs: Array<{ id: string; label: string; type: "local" | "docker" | "ssh" }>;
  activeId: string | null; // null when Start is active
  startActive: boolean;
  connectionStatus: Record<string, "connected" | "disconnected" | "error">;
  onSelectStart: () => void;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
}
```

Remove old props: `dockerInstances`, `hosts`, `onHostsChange`, `addDialogSignal`, `onDockerInstanceRename`, `onDockerInstanceDelete`.

**Step 2: Remove all dialogs**

Delete: SSH add/edit dialog, SSH delete dialog, Docker rename dialog, Docker delete dialog, SSH key guide dialog. Delete all associated state (`dialogOpen`, `editingHost`, `form`, `saving`, `keyGuideOpen`, `menuOpenFor`, `sshDeleteOpen`, `deletingHost`, `dockerRenameOpen`, `editingDocker`, `dockerLabel`, `dockerDeleteOpen`, `deletingDocker`, `deleteDockerData`, `dockerDeleting`, `dockerDeleteError`).

Delete the `CopyBlock` helper component.

**Step 3: Remove `⋯` popover menus**

Replace the `<Popover>` + `<PopoverTrigger>` on each tab with a simple close `×` button:

```tsx
<button
  className={cn(
    "absolute right-1 top-1/2 -translate-y-1/2 inline-flex items-center justify-center w-4 h-4 rounded text-muted-foreground hover:text-foreground hover:bg-accent transition-all cursor-pointer",
    "opacity-0 group-hover:opacity-100"
  )}
  title={t("instance.close")}
  onClick={(e) => {
    e.stopPropagation();
    onClose(tab.id);
  }}
>
  <XIcon className="size-3" />
</button>
```

**Step 4: Simplify tab rendering**

Instead of separate loops for docker and ssh, iterate over the unified `openTabs` array:

```tsx
{openTabs.map((tab) => (
  <div key={tab.id} className="relative group">
    <button
      className={cn(
        "flex items-center gap-1.5 px-3 py-1.5 pr-7 rounded-lg text-sm whitespace-nowrap transition-all duration-200 cursor-pointer",
        activeId === tab.id
          ? "bg-card shadow-sm font-semibold text-primary border-b-2 border-b-primary"
          : "text-muted-foreground hover:text-foreground"
      )}
      onClick={() => onSelect(tab.id)}
    >
      {statusDot(tab.type === "local" ? "connected" : connectionStatus[tab.id])}
      {tab.label}
    </button>
    {/* close button */}
  </div>
))}
```

**Step 5: Add locale keys**

```json
"instance.close": "Close tab"
```

And Chinese translation.

**Step 6: Typecheck**

Run: `npx tsc --noEmit`
Expected: Errors in App.tsx (old props). Fixed in next task.

**Step 7: Commit**

```bash
git add src/components/InstanceTabBar.tsx src/locales/en.json src/locales/zh.json
git commit -m "feat: simplify InstanceTabBar to workspace model with close buttons"
```

---

## Task 7: Rewire App.tsx

**Files:**
- Modify: `src/App.tsx`

This is the integration task. Wire up new StartPage, simplified Home, refactored InstanceTabBar, and Chat scoping.

**Step 1: Add open tabs state**

```tsx
const [openTabIds, setOpenTabIds] = useState<string[]>(["local"]);
```

Persist to localStorage under key `clawpal_open_tabs` so workspace survives restarts.

Add helpers:
```tsx
const openTab = (id: string) => {
  setOpenTabIds((prev) => prev.includes(id) ? prev : [...prev, id]);
  setActiveInstance(id);
  setRoute(lastInstanceRoute);
};

const closeTab = (id: string) => {
  setOpenTabIds((prev) => {
    const next = prev.filter((t) => t !== id);
    // If closing the active tab, switch to Start or adjacent tab
    if (activeInstance === id) {
      if (next.length === 0) {
        setRoute("home"); // auto-navigate to Start
      } else {
        setActiveInstance(next[next.length - 1]);
      }
    }
    return next;
  });
};
```

**Step 2: Derive openTabs array for InstanceTabBar**

```tsx
const openTabs = useMemo(() => {
  return openTabIds.map((id) => {
    if (id === "local") return { id, label: t("instance.local"), type: "local" as const };
    const docker = dockerInstances.find((d) => d.id === id);
    if (docker) return { id, label: docker.label || id, type: "docker" as const };
    const ssh = sshHosts.find((h) => h.id === id);
    if (ssh) return { id, label: ssh.label || ssh.host, type: "ssh" as const };
    return { id, label: id, type: "local" as const }; // fallback
  });
}, [openTabIds, dockerInstances, sshHosts, t]);
```

**Step 3: Update InstanceTabBar usage**

Replace current `<InstanceTabBar>` with new props:

```tsx
<InstanceTabBar
  openTabs={openTabs}
  activeId={inStart ? null : activeInstance}
  startActive={inStart}
  connectionStatus={connectionStatus}
  onSelectStart={openControlCenter}
  onSelect={handleInstanceSelect}
  onClose={closeTab}
/>
```

**Step 4: Replace Start mode content with StartPage**

In the main content area, replace the `route === "home" && <Home controlMode ...>` with:

```tsx
{inStart && (
  <StartPage
    dockerInstances={dockerInstances}
    sshHosts={sshHosts}
    connectionStatus={connectionStatus}
    openTabIds={new Set(openTabIds)}
    onOpenInstance={openTab}
    onRenameDocker={renameDockerInstance}
    onDeleteDocker={deleteDockerInstance}
    onDeleteSsh={(hostId) => {
      api.deleteSshHost(hostId).then(refreshHosts);
    }}
    onEditSsh={(host) => { /* open SSH edit dialog in StartPage */ }}
    onInstallReady={handleInstallReady}
    onRequestAddSsh={() => {}}
    showToast={showToast}
    onNavigate={(r) => setRoute(r as Route)}
  />
)}
```

**Step 5: Simplify sidebar nav for Start mode**

Replace the 4-item Start nav with 2 items:

```tsx
const navItems = inStart
  ? [
      {
        key: "start-profiles",
        active: startSection === "profiles",
        icon: <KeyRoundIcon className="size-4" />,
        label: t("start.nav.profiles"),
        onClick: () => { setRoute("home"); setStartSection("profiles"); },
      },
      {
        key: "start-settings",
        active: startSection === "settings",
        icon: <SettingsIcon className="size-4" />,
        label: t("start.nav.settings"),
        onClick: () => { setRoute("home"); setStartSection("settings"); },
      },
    ]
  : [
      // Instance mode nav: Home, Channels, Recipes, Cron, History, Doctor
      {
        key: "instance-home",
        active: route === "home",
        icon: <HomeIcon className="size-4" />,
        label: t("nav.home"),
        onClick: () => setRoute("home"),
      },
      // ... Channels, Recipes, Cron, History, Doctor (add Recipes back)
    ];
```

Add `HomeIcon` and `BookOpenIcon` (for Recipes) to lucide imports.

**Step 6: Add Home and Recipes to instance nav**

Instance mode sidebar gains two entries compared to current:
- Home (new, was previously not in nav)
- Recipes (was only accessible from dashboard, now in nav)

Update `INSTANCE_ROUTES`:
```tsx
const INSTANCE_ROUTES: Route[] = ["home", "channels", "recipes", "cron", "doctor", "history"];
```

Remove `"sessions"` from routes (merged into Doctor).

**Step 7: Scope Chat panel to instance mode**

Change the chat toggle and panel to only render when `!inStart`:

```tsx
{!inStart && !chatOpen && (
  <button className="absolute top-5 right-5 z-10 ...">
    <MessageCircleIcon className="size-4" />
    {t('nav.chat')}
  </button>
)}

{!inStart && chatOpen && (
  <aside className="w-[380px] ...">
    ...
  </aside>
)}
```

**Step 8: Update Home rendering (remove controlMode)**

Replace:
```tsx
{route === "home" && <Home controlMode startSection={startSection} ... />}
```

With (only for instance mode):
```tsx
{!inStart && route === "home" && (
  <Home
    key={`home-${configVersion}`}
    showToast={showToast}
    onNavigate={(r) => setRoute(r as Route)}
    instanceLabel={openTabs.find(t => t.id === activeInstance)?.label || activeInstance}
  />
)}
```

**Step 9: Handle startSection routing for Start mode**

When `inStart` and `startSection` is "profiles" or "settings", render `<Settings>` in the main content area. When neither is active (user just clicked Start tab), render `<StartPage>`.

```tsx
{inStart && startSection === "profiles" && (
  <Settings globalMode section="profiles" />
)}
{inStart && startSection === "settings" && (
  <Settings globalMode section="preferences" hasAppUpdate={appUpdateAvailable} onAppUpdateSeen={() => setAppUpdateAvailable(false)} />
)}
{inStart && startSection !== "profiles" && startSection !== "settings" && (
  <StartPage ... />
)}
```

Add a new startSection value: `"overview"` (default when clicking Start tab).

**Step 10: Remove Sessions route**

Remove `route === "sessions" && <Sessions />` rendering. Remove `Sessions` import. The Sessions page content is now in Doctor.

**Step 11: Fix handleInstallReady**

Update to also open the newly created instance as a tab:

```tsx
const handleInstallReady = useCallback((session: InstallSession) => {
  // ... existing logic for docker instance creation ...
  // After instance is registered, open it as a tab:
  const instanceId = /* resolve from session */;
  openTab(instanceId);
}, [...]);
```

**Step 12: Persist openTabIds**

```tsx
useEffect(() => {
  localStorage.setItem("clawpal_open_tabs", JSON.stringify(openTabIds));
}, [openTabIds]);

// On mount:
const [openTabIds, setOpenTabIds] = useState<string[]>(() => {
  try {
    const stored = localStorage.getItem("clawpal_open_tabs");
    if (stored) {
      const parsed = JSON.parse(stored);
      if (Array.isArray(parsed) && parsed.length > 0) return parsed;
    }
  } catch {}
  return ["local"];
});
```

**Step 13: Typecheck**

Run: `npx tsc --noEmit`
Expected: PASS (all components now aligned).

**Step 14: Commit**

```bash
git add src/App.tsx
git commit -m "feat: rewire App.tsx for workspace tab model, Start page, and Chat scoping"
```

---

## Task 8: Cleanup dead code

**Files:**
- Modify: `src/pages/Sessions.tsx` — can be deleted entirely (content now in Doctor)
- Modify: `src/App.tsx` — remove Sessions import if not already done
- Modify: `src/components/InstallHub.tsx` — remove any remaining dead copilot code
- Modify: `src/locales/en.json` — remove orphaned keys if desired (optional, low priority)
- Modify: `src/locales/zh.json` — same

**Step 1: Delete Sessions.tsx or convert to redirect**

If any deep links or references to Sessions exist, keep the file but make it render `<Doctor />` with a sessions-focused prop. Otherwise, delete the file entirely.

**Step 2: Remove unused imports across all modified files**

Scan for unused imports in: App.tsx, Home.tsx, InstallHub.tsx, InstanceTabBar.tsx, Doctor.tsx.

**Step 3: Typecheck**

Run: `npx tsc --noEmit`
Expected: PASS.

**Step 4: Commit**

```bash
git add -A
git commit -m "chore: remove dead code from UI reorganization"
```

---

## Task 9: Smoke test and final verification

**Step 1: Run dev server**

```bash
cd /Users/zhixian/Codes/clawpal && npm run dev
```

**Step 2: Manual verification checklist**

- [ ] Start page shows instance card grid with all known instances
- [ ] Clicking instance card opens it in tab bar
- [ ] Tab bar shows `×` close button on hover for instance tabs
- [ ] Closing a tab does not delete instance data
- [ ] Closing all tabs navigates to Start
- [ ] Start tab is always visible, not closeable
- [ ] "+ New/Connect" card opens InstallHub Dialog
- [ ] InstallHub Dialog shows unified intent input + hint chips
- [ ] Install flow shows A2UI stepper (not copilot chat)
- [ ] Blocker recovery shows inline action buttons
- [ ] Instance Home shows status header + model config + agents only
- [ ] Sidebar shows Profiles/Settings in Start mode
- [ ] Sidebar shows Home/Channels/Recipes/Cron/History/Doctor in instance mode
- [ ] Chat button only visible in instance mode
- [ ] Doctor page includes Sessions and Backups sections
- [ ] Recipes accessible from sidebar nav in instance mode
- [ ] PendingChangesBar only visible in instance mode

**Step 3: Typecheck final**

```bash
npx tsc --noEmit
```

**Step 4: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: address issues found during smoke test"
```

---

## Dependency Graph

```
Task 1 (InstanceCard)  ──┐
                          ├──▶ Task 5 (StartPage) ──┐
Task 4 (InstallHub)   ──┘                           │
                                                     ├──▶ Task 7 (App.tsx rewire) ──▶ Task 8 (Cleanup) ──▶ Task 9 (Verify)
Task 2 (Doctor merge)  ─────────────────────────────┤
                                                     │
Task 3 (Home simplify) ─────────────────────────────┤
                                                     │
Task 6 (TabBar refactor) ───────────────────────────┘
```

Tasks 1–4 can be parallelized in pairs:
- **Parallel group A:** Task 1 (InstanceCard) + Task 2 (Doctor merge)
- **Parallel group B:** Task 3 (Home simplify) + Task 4 (InstallHub refactor)
- **Then:** Task 5 (StartPage, depends on 1+4)
- **Then:** Task 6 (TabBar, independent but wait for clarity)
- **Then:** Task 7 (App.tsx, depends on everything)
- **Then:** Task 8 + 9 (cleanup + verify)
