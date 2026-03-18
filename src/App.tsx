import { Suspense, lazy, startTransition, useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  MessageCircleIcon,
  XIcon,
} from "lucide-react";
import { StartPage } from "./pages/StartPage";
import logoUrl from "./assets/logo.png";
const InstanceTabBar = lazy(() => import("./components/InstanceTabBar").then((m) => ({ default: m.InstanceTabBar })));
import { InstanceContext } from "./lib/instance-context";
import { api } from "./lib/api";
import { withGuidance } from "./lib/guidance";
import { useFont } from "./lib/use-font";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { toast, Toaster } from "sonner";
import type { Route } from "./lib/routes";
import type { SshHost } from "./lib/types";

const Home = lazy(() => import("./pages/Home").then((m) => ({ default: m.Home })));
const Recipes = lazy(() => import("./pages/Recipes").then((m) => ({ default: m.Recipes })));
const Cook = lazy(() => import("./pages/Cook").then((m) => ({ default: m.Cook })));
const History = lazy(() => import("./pages/History").then((m) => ({ default: m.History })));
const Settings = lazy(() => import("./pages/Settings").then((m) => ({ default: m.Settings })));
const Doctor = lazy(() => import("./pages/Doctor").then((m) => ({ default: m.Doctor })));
const OpenclawContext = lazy(() => import("./pages/OpenclawContext").then((m) => ({ default: m.OpenclawContext })));
const Channels = lazy(() => import("./pages/Channels").then((m) => ({ default: m.Channels })));
const Cron = lazy(() => import("./pages/Cron").then((m) => ({ default: m.Cron })));
const Orchestrator = lazy(() => import("./pages/Orchestrator").then((m) => ({ default: m.Orchestrator })));
const Chat = lazy(() => import("./components/Chat").then((m) => ({ default: m.Chat })));

import { useInstanceManager } from "./hooks/useInstanceManager";
import { useSshConnection } from "./hooks/useSshConnection";
import { useInstancePersistence } from "./hooks/useInstancePersistence";
import { useChannelCache } from "./hooks/useChannelCache";
import { useAppLifecycle } from "./hooks/useAppLifecycle";
import { useWorkspaceTabs } from "./hooks/useWorkspaceTabs";
import { useNavItems } from "./hooks/useNavItems";
import { PassphraseDialog, SshEditDialog } from "./components/AppDialogs";
import { SidebarFooter } from "./components/SidebarFooter";

export function App() {
  const { t } = useTranslation();
  useFont();

  const [route, setRoute] = useState<Route>("home");
  const [recipeId, setRecipeId] = useState<string | null>(null);
  const [recipeSource, setRecipeSource] = useState<string | undefined>(undefined);
  const [chatOpen, setChatOpen] = useState(false);

  const navigateRoute = useCallback((next: Route) => {
    startTransition(() => setRoute(next));
  }, []);

  const showToast = useCallback((message: string, type: "success" | "error" = "success") => {
    if (type === "error") {
      toast.error(message, { duration: 5000 });
      return;
    }
    toast.success(message, { duration: 3000 });
  }, []);

  // ── Instance manager ──
  const instanceManager = useInstanceManager();
  const {
    sshHosts,
    registeredInstances,
    setRegisteredInstances,
    discoveredInstances,
    discoveringInstances,
    connectionStatus,
    setConnectionStatus,
    sshEditOpen,
    setSshEditOpen,
    editingSshHost,
    handleEditSsh,
    refreshHosts,
    refreshRegisteredInstances,
    discoverInstances,
    dockerInstances,
    upsertDockerInstance,
    renameDockerInstance,
    deleteDockerInstance,
  } = instanceManager;

  const resolveInstanceTransport = useCallback((instanceId: string) => {
    if (instanceId === "local") return "local";
    const registered = registeredInstances.find((item) => item.id === instanceId);
    if (registered?.instanceType === "docker") return "docker_local";
    if (registered?.instanceType === "remote_ssh") return "remote_ssh";
    if (instanceId.startsWith("docker:")) return "docker_local";
    if (instanceId.startsWith("ssh:")) return "remote_ssh";
    if (dockerInstances.some((item) => item.id === instanceId)) return "docker_local";
    if (sshHosts.some((host) => host.id === instanceId)) return "remote_ssh";
    return "local";
  }, [dockerInstances, sshHosts, registeredInstances]);

  // ── Workspace tabs (needs resolveInstanceTransport before SSH/persistence) ──
  // We forward-declare these as they form a dependency cycle with SSH + persistence.
  // useWorkspaceTabs is initialized after SSH and persistence hooks below.

  // Placeholder activeInstance for derived state — will be overridden by useWorkspaceTabs.
  // We need a temporary state to bootstrap the hooks that depend on activeInstance.
  const [_bootstrapActiveInstance, _setBootstrapActiveInstance] = useState("local");

  // ── Persistence (needs activeInstance — use bootstrap for now) ──
  const persistence = useInstancePersistence({
    activeInstance: _bootstrapActiveInstance,
    registeredInstances,
    dockerInstances,
    sshHosts,
    isDocker: registeredInstances.some((item) => item.id === _bootstrapActiveInstance && item.instanceType === "docker")
      || dockerInstances.some((item) => item.id === _bootstrapActiveInstance),
    isRemote: registeredInstances.some((item) => item.id === _bootstrapActiveInstance && item.instanceType === "remote_ssh")
      || sshHosts.some((host) => host.id === _bootstrapActiveInstance),
    isConnected: !(registeredInstances.some((item) => item.id === _bootstrapActiveInstance && item.instanceType === "remote_ssh")
      || sshHosts.some((host) => host.id === _bootstrapActiveInstance))
      || connectionStatus[_bootstrapActiveInstance] === "connected",
    resolveInstanceTransport,
    showToast,
    setShowSshTransferSpeedUi: () => {},  // patched below
  });

  const {
    configVersion,
    bumpConfigVersion,
    instanceToken,
    persistenceScope,
    setPersistenceScope,
    persistenceResolved,
    setPersistenceResolved,
    scheduleEnsureAccessForInstance,
  } = persistence;

  const isDocker = registeredInstances.some((item) => item.id === _bootstrapActiveInstance && item.instanceType === "docker")
    || dockerInstances.some((item) => item.id === _bootstrapActiveInstance);
  const isRemote = registeredInstances.some((item) => item.id === _bootstrapActiveInstance && item.instanceType === "remote_ssh")
    || sshHosts.some((host) => host.id === _bootstrapActiveInstance);
  const isConnected = !isRemote || connectionStatus[_bootstrapActiveInstance] === "connected";

  // ── SSH connection ──
  const ssh = useSshConnection({
    activeInstance: _bootstrapActiveInstance,
    sshHosts,
    isRemote,
    isConnected,
    connectionStatus,
    setConnectionStatus,
    setPersistenceScope,
    setPersistenceResolved,
    resolveInstanceTransport,
    showToast,
    scheduleEnsureAccessForInstance,
  });

  const {
    profileSyncStatus,
    showSshTransferSpeedUi,
    sshTransferStats,
    doctorNavPulse,
    setDoctorNavPulse,
    passphraseHostLabel,
    passphraseOpen,
    passphraseInput,
    setPassphraseInput,
    closePassphraseDialog,
    connectWithPassphraseFallback,
    syncRemoteAuthAfterConnect,
  } = ssh;

  // ── Workspace tabs ──
  const tabs = useWorkspaceTabs({
    registeredInstances,
    setRegisteredInstances,
    sshHosts,
    dockerInstances,
    resolveInstanceTransport,
    connectWithPassphraseFallback,
    syncRemoteAuthAfterConnect,
    scheduleEnsureAccessForInstance,
    upsertDockerInstance,
    refreshHosts,
    refreshRegisteredInstances,
    showToast,
    setConnectionStatus,
    navigateRoute,
  });

  const {
    openTabIds,
    setOpenTabIds,
    activeInstance,
    inStart,
    setInStart,
    startSection,
    setStartSection,
    openTab,
    closeTab,
    handleInstanceSelect,
    openTabs,
    openControlCenter,
    handleInstallReady,
    handleDeleteSsh,
  } = tabs;

  // Sync bootstrap → real activeInstance for hooks that depend on it.
  // This is a controlled pattern: useWorkspaceTabs owns the real state,
  // and we keep the bootstrap in sync so persistence/SSH hooks track it.
  if (_bootstrapActiveInstance !== activeInstance) {
    _setBootstrapActiveInstance(activeInstance);
  }

  // ── Channel cache ──
  const channels = useChannelCache({
    activeInstance,
    route,
    instanceToken,
    persistenceScope,
    persistenceResolved,
    isRemote,
    isConnected,
  });

  // ── App lifecycle ──
  const lifecycle = useAppLifecycle({
    showToast,
    refreshHosts,
    refreshRegisteredInstances,
  });

  const { appUpdateAvailable, setAppUpdateAvailable, appVersion } = lifecycle;

  // ── SSH edit save ──
  const handleSshEditSave = useCallback(async (host: SshHost) => {
    try {
      await withGuidance(
        () => api.upsertSshHost(host),
        "upsertSshHost",
        host.id,
        "remote_ssh",
      );
      refreshHosts();
      refreshRegisteredInstances();
      setSshEditOpen(false);
      showToast(t("instance.sshUpdated"), "success");
    } catch (e) {
      showToast(e instanceof Error ? e.message : String(e), "error");
    }
  }, [refreshHosts, refreshRegisteredInstances, showToast, t, setSshEditOpen]);

  // ── Discovered instance connect ──
  const handleConnectDiscovered = useCallback(async (discovered: import("./lib/types").DiscoveredInstance) => {
    try {
      await withGuidance(
        () => api.connectDockerInstance(discovered.homePath, discovered.label, discovered.id),
        "connectDockerInstance",
        discovered.id,
        "docker_local",
      );
      refreshRegisteredInstances();
      discoverInstances();
      showToast(t("start.connected", { label: discovered.label }), "success");
    } catch (e) {
      showToast(e instanceof Error ? e.message : String(e), "error");
    }
  }, [refreshRegisteredInstances, discoverInstances, showToast, t]);

  // ── Doctor navigation ──
  const openDoctor = useCallback(() => {
    setDoctorNavPulse(true);
    setInStart(false);
    navigateRoute("doctor");
    window.setTimeout(() => {
      setDoctorNavPulse(false);
    }, 1400);
  }, [navigateRoute, setDoctorNavPulse, setInStart]);

  // ── Navigation items ──
  const navItems = useNavItems({ inStart, startSection, setStartSection, route, navigateRoute, openDoctor, doctorNavPulse });

  return (
    <>
    <div className="flex flex-col h-screen bg-background text-foreground">
      <Suspense fallback={null}>
        <InstanceTabBar
          openTabs={openTabs}
          activeId={inStart ? null : activeInstance}
          startActive={inStart}
          connectionStatus={connectionStatus}
          appVersion={appVersion}
          onSelectStart={openControlCenter}
          onSelect={handleInstanceSelect}
          onClose={closeTab}
        />
      </Suspense>
      <InstanceContext.Provider value={{
        instanceId: activeInstance,
        instanceViewToken: activeInstance,
        instanceToken,
        persistenceScope,
        persistenceResolved,
        isRemote,
        isDocker,
        isConnected,
        channelNodes: channels.channelNodes,
        discordGuildChannels: channels.discordGuildChannels,
        channelsLoading: channels.channelsLoading,
        discordChannelsLoading: channels.discordChannelsLoading,
        refreshChannelNodesCache: channels.refreshChannelNodesCache,
        refreshDiscordChannelsCache: channels.refreshDiscordChannelsCache,
      }}>
      <div className="flex flex-1 overflow-hidden">

      {/* ── Sidebar ── */}
      <aside className="w-[220px] min-w-[220px] bg-sidebar border-r border-sidebar-border flex flex-col py-5">
        <div className="px-5 mb-6 flex items-center gap-2.5">
          <img src={logoUrl} alt="" className="w-9 h-9 rounded-xl shadow-sm" />
          <h1 className="text-xl font-bold tracking-tight" style={{ fontFamily: "'Fraunces', Georgia, serif" }}>
            ClawPal
          </h1>
        </div>

        <nav className="flex flex-col gap-0.5 px-3 flex-1">
          {navItems.map((item) => (
              <button
                key={item.key}
                className={cn(
                  "flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-200 cursor-pointer",
                  item.active
                    ? "bg-primary/10 text-primary shadow-sm"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                )}
                onClick={item.onClick}
              >
                {item.icon}
                <span>{item.label}</span>
                {item.badge}
              </button>
          ))}

          <div className="my-3 h-px bg-border/60" />

        </nav>

        <SidebarFooter
          profileSyncStatus={profileSyncStatus}
          showSshTransferSpeedUi={showSshTransferSpeedUi}
          isRemote={isRemote}
          isConnected={isConnected}
          sshTransferStats={sshTransferStats}
          inStart={inStart}
          showToast={showToast}
          bumpConfigVersion={bumpConfigVersion}
        />
      </aside>

      {/* ── Main Content ── */}
      <main className="flex-1 overflow-y-auto p-6 relative">
        {/* Chat toggle — floating pill (instance mode only) */}
        {!inStart && !chatOpen && (
          <button
            className="absolute top-5 right-5 z-10 flex items-center gap-2 px-3.5 py-2 rounded-full bg-primary/10 text-primary text-sm font-medium hover:bg-primary/15 transition-all duration-200 shadow-sm cursor-pointer"
            onClick={() => setChatOpen(true)}
          >
            <MessageCircleIcon className="size-4" />
            {t('nav.chat')}
          </button>
        )}

        <div className="animate-warm-enter">
          <Suspense fallback={<p className="text-sm text-muted-foreground animate-pulse">Loading…</p>}>
          {/* ── Start mode content ── */}
          {inStart && startSection === "overview" && (
            <StartPage
              dockerInstances={dockerInstances}
              sshHosts={sshHosts}
              registeredInstances={registeredInstances}
              openTabIds={new Set(openTabIds)}
              connectRemoteHost={connectWithPassphraseFallback}
              onOpenInstance={openTab}
              onRenameDocker={renameDockerInstance}
              onDeleteDocker={deleteDockerInstance}
              onDeleteSsh={handleDeleteSsh}
              onEditSsh={handleEditSsh}
              onInstallReady={handleInstallReady}
              showToast={showToast}
              onNavigate={(r) => navigateRoute(r as Route)}
              onOpenDoctor={openDoctor}
              discoveredInstances={discoveredInstances}
              discoveringInstances={discoveringInstances}
              onConnectDiscovered={handleConnectDiscovered}
            />
          )}
          {inStart && startSection === "profiles" && (
            <Settings
              key="global-profiles"
              globalMode
              section="profiles"
              onOpenDoctor={openDoctor}
              onDataChange={bumpConfigVersion}
            />
          )}
          {inStart && startSection === "settings" && (
            <Settings
              key="global-settings"
              globalMode
              section="preferences"
              onOpenDoctor={openDoctor}
              onDataChange={bumpConfigVersion}
              hasAppUpdate={appUpdateAvailable}
              onAppUpdateSeen={() => setAppUpdateAvailable(false)}
            />
          )}

          {/* ── Instance mode content ── */}
          {!inStart && route === "home" && (
            <Home
              key={`home-${activeInstance}-${configVersion}-${persistenceResolved ? "ready" : "pending"}-${persistenceScope ?? "none"}`}
              instanceLabel={openTabs.find((t) => t.id === activeInstance)?.label || activeInstance}
              showToast={showToast}
              onNavigate={(r) => navigateRoute(r as Route)}
            />
          )}
          {!inStart && route === "recipes" && (
            <Recipes
              onCook={(id, source) => {
                setRecipeId(id);
                setRecipeSource(source);
                navigateRoute("cook");
              }}
            />
          )}
          {!inStart && route === "cook" && recipeId && (
            <Cook
              recipeId={recipeId}
              recipeSource={recipeSource}
              onDone={() => {
                navigateRoute("recipes");
              }}
            />
          )}
          {!inStart && route === "cook" && !recipeId && <p>{t('config.noRecipeSelected')}</p>}
          {!inStart && route === "channels" && (
            <Channels
              key={`channels-${activeInstance}-${configVersion}-${persistenceResolved ? "ready" : "pending"}-${persistenceScope ?? "none"}`}
              showToast={showToast}
            />
          )}
          {!inStart && route === "cron" && <Cron key={`cron-${activeInstance}-${configVersion}-${persistenceResolved ? "ready" : "pending"}-${persistenceScope ?? "none"}`} />}
          {!inStart && route === "history" && <History key={`history-${activeInstance}-${configVersion}`} />}
          {!inStart && route === "doctor" && (
            <Doctor key={activeInstance} />
          )}
          {!inStart && route === "context" && <OpenclawContext key={`context-${activeInstance}`} />}
          {!inStart && route === "orchestrator" && <Orchestrator key={`orchestrator-${activeInstance}`} />}
          </Suspense>
        </div>
      </main>

      {/* ── Chat Panel (instance mode only) ── */}
      {!inStart && chatOpen && (
        <aside className="w-[380px] min-w-[380px] border-l border-border flex flex-col bg-card">
          <div className="flex items-center justify-between px-5 pt-5 pb-3">
            <h2 className="text-lg font-semibold">{t('nav.chat')}</h2>
            <Button
              variant="ghost"
              size="icon-xs"
              onClick={() => setChatOpen(false)}
            >
              <XIcon className="size-4" />
            </Button>
          </div>
          <div className="flex-1 overflow-hidden px-5 pb-5">
            <Suspense fallback={<p className="text-sm text-muted-foreground animate-pulse">Loading…</p>}>
              <Chat />
            </Suspense>
          </div>
        </aside>
      )}
      </div>
      </InstanceContext.Provider>
    </div>
    <PassphraseDialog
      open={passphraseOpen}
      hostLabel={passphraseHostLabel}
      input={passphraseInput}
      onInputChange={setPassphraseInput}
      onClose={closePassphraseDialog}
    />
    <SshEditDialog
      open={sshEditOpen}
      onOpenChange={setSshEditOpen}
      host={editingSshHost}
      onSave={handleSshEditSave}
    />
    <Toaster position="top-right" richColors />
    </>
  );
}
