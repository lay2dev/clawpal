import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { CreateAgentDialog } from "@/components/CreateAgentDialog";
import { UpgradeDialog } from "@/components/UpgradeDialog";
import { RecipeCard } from "@/components/RecipeCard";
import { Skeleton } from "@/components/ui/skeleton";
import type { InstanceStatus, AgentOverview, Recipe, BackupInfo, ModelProfile } from "../lib/types";
import { formatTime, formatBytes } from "@/lib/utils";
import { useInstance } from "@/lib/instance-context";

interface AgentGroup {
  identity: string;
  emoji?: string;
  agents: AgentOverview[];
}

function groupAgents(agents: AgentOverview[]): AgentGroup[] {
  const map = new Map<string, AgentGroup>();
  for (const a of agents) {
    // Group by workspace path (shared identity), fallback to agent id
    const key = a.workspace || a.id;
    if (!map.has(key)) {
      map.set(key, {
        identity: a.name || a.id,
        emoji: a.emoji,
        agents: [],
      });
    }
    map.get(key)!.agents.push(a);
  }
  return Array.from(map.values());
}

export function Home({
  onCook,
  showToast,
}: {
  onCook?: (recipeId: string, source?: string) => void;
  showToast?: (message: string, type?: "success" | "error") => void;
}) {
  const { t } = useTranslation();
  const { instanceId, isRemote, isConnected } = useInstance();
  const [status, setStatus] = useState<InstanceStatus | null>(null);
  const [version, setVersion] = useState<string | null>(null);
  const [updateInfo, setUpdateInfo] = useState<{ available: boolean; latest?: string } | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [agents, setAgents] = useState<AgentOverview[] | null>(null);
  const [recipes, setRecipes] = useState<Recipe[]>([]);
  const [backups, setBackups] = useState<BackupInfo[] | null>(null);
  const [modelProfiles, setModelProfiles] = useState<ModelProfile[]>([]);
  const [savingModel, setSavingModel] = useState(false);
  const [backingUp, setBackingUp] = useState(false);
  const [backupMessage, setBackupMessage] = useState("");


  // Create agent dialog
  const [showCreateAgent, setShowCreateAgent] = useState(false);
  const [showUpgradeDialog, setShowUpgradeDialog] = useState(false);

  const resolveModelValue = (profileId: string | null): string | null => {
    if (!profileId) return null;
    const profile = modelProfiles.find((p) => p.id === profileId);
    if (!profile) return profileId;
    return profile.model.includes("/")
      ? profile.model
      : `${profile.provider}/${profile.model}`;
  };

  // Health status with grace period: retry quickly when unhealthy, then slow-poll
  const [statusSettled, setStatusSettled] = useState(false);
  const retriesRef = useRef(0);
  const remoteErrorShownRef = useRef(false);

  const fetchStatus = useCallback(() => {
    if (isRemote) {
      if (!isConnected) return; // Wait for SSH connection
      api.remoteGetInstanceStatus(instanceId).then((s) => {
        setStatus(s);
        setStatusSettled(true);
        remoteErrorShownRef.current = false;
        if (s.openclawVersion) setVersion(s.openclawVersion);
      }).catch((e) => {
        console.error("Failed to fetch remote status:", e);
        if (!remoteErrorShownRef.current) {
          remoteErrorShownRef.current = true;
          showToast?.(t('home.remoteReadFailed', { error: String(e) }), "error");
        }
      });
    } else {
      api.getInstanceStatus().then((s) => {
        setStatus(s);
        if (s.healthy) {
          setStatusSettled(true);
          retriesRef.current = 0;
        } else if (retriesRef.current < 5) {
          retriesRef.current++;
        } else {
          setStatusSettled(true);
        }
      }).catch((e) => console.error("Failed to fetch status:", e));
    }
  }, [isRemote, isConnected, instanceId, showToast, t]);

  useEffect(() => {
    remoteErrorShownRef.current = false;
    fetchStatus();
    // Poll fast (2s) while not settled, slow (10s) once settled
    const interval = setInterval(fetchStatus, statusSettled ? 10000 : 2000);
    return () => clearInterval(interval);
  }, [fetchStatus, statusSettled]);

  const refreshAgents = useCallback(() => {
    if (isRemote) {
      if (!isConnected) return; // Wait for SSH connection
      api.remoteListAgentsOverview(instanceId).then((a) => {
        setAgents(a);
        remoteErrorShownRef.current = false;
      }).catch((e) => {
        console.error("Failed to load remote agents:", e);
        if (!remoteErrorShownRef.current) {
          remoteErrorShownRef.current = true;
          showToast?.(t('home.remoteAgentsFailed', { error: String(e) }), "error");
        }
      });
      return;
    }
    api.listAgentsOverview().then(setAgents).catch((e) => console.error("Failed to load agents:", e));
  }, [isRemote, isConnected, instanceId, showToast, t]);

  useEffect(() => {
    refreshAgents();
    // Auto-refresh agents every 15s
    const interval = setInterval(refreshAgents, 15000);
    return () => clearInterval(interval);
  }, [refreshAgents]);

  useEffect(() => {
    api.listRecipes().then((r) => setRecipes(r.slice(0, 4))).catch((e) => console.error("Failed to load recipes:", e));
  }, []);

  const refreshBackups = useCallback(() => {
    if (isRemote) {
      if (!isConnected) return;
      api.remoteListBackups(instanceId).then(setBackups).catch((e) => console.error("Failed to load remote backups:", e));
    } else {
      api.listBackups().then(setBackups).catch((e) => console.error("Failed to load backups:", e));
    }
  }, [isRemote, isConnected, instanceId]);
  useEffect(refreshBackups, [refreshBackups]);

  useEffect(() => {
    if (isRemote) {
      if (!isConnected) return;
      api.remoteListModelProfiles(instanceId).then((p) => setModelProfiles(p.filter((m) => m.enabled))).catch((e) => console.error("Failed to load remote model profiles:", e));
    } else {
      api.listModelProfiles().then((p) => setModelProfiles(p.filter((m) => m.enabled))).catch((e) => console.error("Failed to load model profiles:", e));
    }
  }, [isRemote, isConnected, instanceId]);

  // Match current global model value to a profile ID
  const currentModelProfileId = useMemo(() => {
    const modelVal = status?.globalDefaultModel;
    if (!modelVal) return null;
    const normalized = modelVal.toLowerCase();
    for (const p of modelProfiles) {
      const profileVal = p.model.includes("/") ? p.model : `${p.provider}/${p.model}`;
      if (profileVal.toLowerCase() === normalized || p.model.toLowerCase() === normalized) {
        return p.id;
      }
    }
    return null;
  }, [status?.globalDefaultModel, modelProfiles]);

  const agentGroups = useMemo(() => groupAgents(agents || []), [agents]);

  // Update check — deferred, runs once (not in poll loop)
  useEffect(() => {
    setCheckingUpdate(true);
    setUpdateInfo(null);
    const timer = setTimeout(() => {
      if (isRemote) {
        if (!isConnected) { setCheckingUpdate(false); return; }
        api.remoteCheckOpenclawUpdate(instanceId).then((u) => {
          setUpdateInfo({ available: u.upgradeAvailable, latest: u.latestVersion ?? undefined });
        }).catch((e) => console.error("Failed to check remote update:", e))
          .finally(() => setCheckingUpdate(false));
      } else {
        api.getSystemStatus().then((s) => {
          setVersion(s.openclawVersion);
          if (s.openclawUpdate) {
            setUpdateInfo({
              available: s.openclawUpdate.upgradeAvailable,
              latest: s.openclawUpdate.latestVersion,
            });
          }
        }).catch((e) => console.error("Failed to fetch system status:", e))
          .finally(() => setCheckingUpdate(false));
      }
    }, 500);
    return () => clearTimeout(timer);
  }, [isRemote, isConnected, instanceId]);


  const handleDeleteAgent = (agentId: string) => {
    if (isRemote && !isConnected) return;
    const deletePromise = isRemote
      ? api.remoteDeleteAgent(instanceId, agentId)
      : api.deleteAgent(agentId);
    deletePromise
      .then(() => refreshAgents())
      .catch((e) => showToast?.(String(e), "error"));
  };

  return (
    <div>
      <h2 className="text-2xl font-bold mb-4">{t('home.title')}</h2>

        {/* Status Summary */}
        <h3 className="text-lg font-semibold mt-6 mb-3">{t('home.status')}</h3>
        <Card>
          <CardContent className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-3 items-center">
            <span className="text-sm text-muted-foreground">{t('home.health')}</span>
            <span className="text-sm font-medium">
              {!status ? "..." : status.healthy ? (
                <Badge className="bg-green-100 text-green-700 border-0">{t('home.healthy')}</Badge>
              ) : !statusSettled ? (
                <Badge className="bg-amber-100 text-amber-700 border-0">{t('home.checking')}</Badge>
              ) : (
                <Badge className="bg-red-100 text-red-700 border-0">{t('home.unhealthy')}</Badge>
              )}
            </span>

            <span className="text-sm text-muted-foreground">{t('home.version')}</span>
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-sm font-medium">{version || "..."}</span>
              {checkingUpdate && (
                <Badge variant="outline" className="text-muted-foreground">{t('home.checkingUpdates')}</Badge>
              )}
              {!checkingUpdate && updateInfo?.available && updateInfo.latest && updateInfo.latest !== version && (
                <>
                  <Badge variant="outline" className="text-primary border-primary">
                    {t('home.available', { version: updateInfo.latest })}
                  </Badge>
                  <Button
                    size="sm"
                    className="text-xs h-6"
                    variant="outline"
                    onClick={() => api.openUrl("https://github.com/openclaw/openclaw/releases")}
                  >
                    {t('home.view')}
                  </Button>
                  <Button
                    size="sm"
                    className="text-xs h-6"
                    onClick={() => setShowUpgradeDialog(true)}
                  >
                    {t('home.upgrade')}
                  </Button>
                </>
              )}
            </div>


            <span className="text-sm text-muted-foreground">{t('home.defaultModel')}</span>
            <div className="max-w-xs">
              {status ? (
                <Select
                  value={currentModelProfileId || (status?.globalDefaultModel ? "__raw__" : "__none__")}
                  onValueChange={(val) => {
                    if (val === "__raw__") return;
                    setSavingModel(true);
                    const modelValue = resolveModelValue(val === "__none__" ? null : val);
                    const setModelPromise = isRemote
                      ? api.remoteSetGlobalModel(instanceId, modelValue)
                      : api.setGlobalModel(modelValue);
                    setModelPromise
                      .then(() => fetchStatus())
                      .catch((e) => showToast?.(String(e), "error"))
                      .finally(() => setSavingModel(false));
                  }}
                  disabled={savingModel}
                >
                  <SelectTrigger size="sm" className="text-sm">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="__none__">
                      <span className="text-muted-foreground">{t('home.notSet')}</span>
                    </SelectItem>
                    {status?.globalDefaultModel && !currentModelProfileId && (
                      <SelectItem value="__raw__">
                        {status.globalDefaultModel}
                      </SelectItem>
                    )}
                    {modelProfiles.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.provider}/{p.model}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              ) : (
                <span className="text-sm">...</span>
              )}
            </div>
          </CardContent>
        </Card>

        {/* Agents Overview -- grouped by identity */}
        <div className="flex items-center justify-between mt-6 mb-3">
          <h3 className="text-lg font-semibold">{t('home.agents')}</h3>
          <Button size="sm" variant="outline" onClick={() => setShowCreateAgent(true)}>
            {t('home.newAgent')}
          </Button>
        </div>
        {agents === null ? (
          <div className="space-y-3">
            <Skeleton className="h-24 w-full" />
            <Skeleton className="h-24 w-full" />
          </div>
        ) : agentGroups.length === 0 ? (
          <p className="text-muted-foreground">{t('home.noAgents')}</p>
        ) : (
          <div className="space-y-3">
            {agentGroups.map((group) => (
              <Card key={group.agents[0].workspace || group.agents[0].id}>
                <CardContent>
                  <div className="flex items-center gap-1.5 mb-2">
                    {group.emoji && <span>{group.emoji}</span>}
                    <strong className="text-base">{group.identity}</strong>
                  </div>
                  <div className="space-y-1.5">
                    {group.agents.map((agent) => (
                      <div
                        key={agent.id}
                        className="flex items-center justify-between rounded-md border px-3 py-1.5"
                      >
                        <div className="flex items-center gap-2.5">
                          <code className="text-sm text-foreground font-medium">{agent.id}</code>
                          <Select
                            value={(() => {
                              if (!agent.model) return "__none__";
                              const normalized = agent.model.toLowerCase();
                              for (const p of modelProfiles) {
                                const profileVal = p.model.includes("/") ? p.model : `${p.provider}/${p.model}`;
                                if (profileVal.toLowerCase() === normalized || p.model.toLowerCase() === normalized) {
                                  return p.id;
                                }
                              }
                              return "__none__";
                            })()}
                            onValueChange={(val) => {
                              const modelValue = resolveModelValue(val === "__none__" ? null : val);
                              const setModelPromise = isRemote
                                ? api.remoteSetAgentModel(instanceId, agent.id, modelValue)
                                : api.setAgentModel(agent.id, modelValue);
                              setModelPromise
                                .then(() => refreshAgents())
                                .catch((e) => showToast?.(String(e), "error"));
                            }}
                          >
                            <SelectTrigger size="sm" className="text-xs h-6 w-auto min-w-[120px] max-w-[200px]">
                              <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                              <SelectItem value="__none__">
                                <span className="text-muted-foreground">{t('home.defaultModelOption')}</span>
                              </SelectItem>
                              {modelProfiles.map((p) => (
                                <SelectItem key={p.id} value={p.id}>
                                  {p.provider}/{p.model}
                                </SelectItem>
                              ))}
                            </SelectContent>
                          </Select>
                        </div>
                        <div className="flex items-center gap-2">
                          {agent.online ? (
                            <Badge className="bg-green-100 text-green-700 border-0 text-xs">{t('home.online')}</Badge>
                          ) : (
                            <Badge className="bg-red-100 text-red-700 border-0 text-xs">{t('home.offline')}</Badge>
                          )}
                          {agent.id !== "main" && (
                            <AlertDialog>
                              <AlertDialogTrigger asChild>
                                <Button size="sm" variant="ghost" className="h-6 px-1.5 text-xs text-muted-foreground hover:text-destructive">
                                  {t('home.delete')}
                                </Button>
                              </AlertDialogTrigger>
                              <AlertDialogContent>
                                <AlertDialogHeader>
                                  <AlertDialogTitle>{t('home.deleteAgentTitle', { agentId: agent.id })}</AlertDialogTitle>
                                  <AlertDialogDescription>
                                    {t('home.deleteAgentDescription')}
                                  </AlertDialogDescription>
                                </AlertDialogHeader>
                                <AlertDialogFooter>
                                  <AlertDialogCancel>{t('config.cancel')}</AlertDialogCancel>
                                  <AlertDialogAction
                                    className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                                    onClick={() => handleDeleteAgent(agent.id)}
                                  >
                                    {t('home.delete')}
                                  </AlertDialogAction>
                                </AlertDialogFooter>
                              </AlertDialogContent>
                            </AlertDialog>
                          )}
                        </div>
                      </div>
                    ))}
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        )}

        {/* Recommended Recipes */}
        <h3 className="text-lg font-semibold mt-6 mb-3">{t('home.recommendedRecipes')}</h3>
        {recipes.length === 0 ? (
          <p className="text-muted-foreground">{t('home.noRecipes')}</p>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-3">
            {recipes.map((recipe) => (
              <RecipeCard
                key={recipe.id}
                recipe={recipe}
                onCook={() => onCook?.(recipe.id)}
                compact
              />
            ))}
          </div>
        )}

        {/* Backups */}
        <div className="flex items-center justify-between mt-6 mb-3">
          <h3 className="text-lg font-semibold">{t('home.backups')}</h3>
          <Button
            size="sm"
            variant="outline"
            disabled={backingUp}
            onClick={() => {
              setBackingUp(true);
              setBackupMessage("");
              const backupPromise = isRemote
                ? api.remoteBackupBeforeUpgrade(instanceId)
                : api.backupBeforeUpgrade();
              backupPromise
                .then((info) => {
                  setBackupMessage(t('home.backupCreated', { name: info.name }));
                  refreshBackups();
                })
                .catch((e) => setBackupMessage(t('home.backupFailed', { error: String(e) })))
                .finally(() => setBackingUp(false));
            }}
          >
            {backingUp ? t('home.creating') : t('home.createBackup')}
          </Button>
        </div>
        {backupMessage && (
          <p className="text-sm text-muted-foreground mb-2">{backupMessage}</p>
        )}
        {backups === null ? (
          <div className="space-y-2">
            <Skeleton className="h-16 w-full" />
            <Skeleton className="h-16 w-full" />
          </div>
        ) : backups.length === 0 ? (
          <p className="text-muted-foreground text-sm">{t('home.noBackups')}</p>
        ) : (
          <div className="space-y-2">
            {backups.map((backup) => (
              <Card key={backup.name}>
                <CardContent className="flex items-center justify-between">
                  <div>
                    <div className="font-medium text-sm">{backup.name}</div>
                    <div className="text-xs text-muted-foreground">
                      {formatTime(backup.createdAt)} — {formatBytes(backup.sizeBytes)}
                    </div>
                    {isRemote && backup.path && (
                      <div className="text-xs text-muted-foreground mt-0.5 font-mono">{backup.path}</div>
                    )}
                  </div>
                  <div className="flex gap-1.5">
                    {!isRemote && (
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => api.openUrl(backup.path)}
                      >
                        {t('home.show')}
                      </Button>
                    )}
                    <AlertDialog>
                      <AlertDialogTrigger asChild>
                        <Button size="sm" variant="outline">
                          {t('home.restore')}
                        </Button>
                      </AlertDialogTrigger>
                      <AlertDialogContent>
                        <AlertDialogHeader>
                          <AlertDialogTitle>{t('home.restoreTitle')}</AlertDialogTitle>
                          <AlertDialogDescription>
                            {t('home.restoreDescription', { name: backup.name })}
                          </AlertDialogDescription>
                        </AlertDialogHeader>
                        <AlertDialogFooter>
                          <AlertDialogCancel>{t('config.cancel')}</AlertDialogCancel>
                          <AlertDialogAction
                            onClick={() => {
                              const restorePromise = isRemote
                                ? api.remoteRestoreFromBackup(instanceId, backup.name)
                                : api.restoreFromBackup(backup.name);
                              restorePromise
                                .then((msg) => setBackupMessage(msg))
                                .catch((e) => setBackupMessage(t('home.restoreFailed', { error: String(e) })));
                            }}
                          >
                            {t('home.restore')}
                          </AlertDialogAction>
                        </AlertDialogFooter>
                      </AlertDialogContent>
                    </AlertDialog>
                    <AlertDialog>
                      <AlertDialogTrigger asChild>
                        <Button size="sm" variant="destructive">
                          {t('home.delete')}
                        </Button>
                      </AlertDialogTrigger>
                      <AlertDialogContent>
                        <AlertDialogHeader>
                          <AlertDialogTitle>{t('home.deleteBackupTitle')}</AlertDialogTitle>
                          <AlertDialogDescription>
                            {t('home.deleteBackupDescription', { name: backup.name })}
                          </AlertDialogDescription>
                        </AlertDialogHeader>
                        <AlertDialogFooter>
                          <AlertDialogCancel>{t('config.cancel')}</AlertDialogCancel>
                          <AlertDialogAction
                            className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                            onClick={() => {
                              const deletePromise = isRemote
                                ? api.remoteDeleteBackup(instanceId, backup.name)
                                : api.deleteBackup(backup.name);
                              deletePromise
                                .then(() => {
                                  setBackupMessage(t('home.deletedBackup', { name: backup.name }));
                                  refreshBackups();
                                })
                                .catch((e) => setBackupMessage(t('home.deleteBackupFailed', { error: String(e) })));
                            }}
                          >
                            {t('home.delete')}
                          </AlertDialogAction>
                        </AlertDialogFooter>
                      </AlertDialogContent>
                    </AlertDialog>
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        )}

      {/* Create Agent Dialog */}
      <CreateAgentDialog
        open={showCreateAgent}
        onOpenChange={setShowCreateAgent}
        modelProfiles={modelProfiles}
        onCreated={() => refreshAgents()}
      />

      {/* Upgrade Dialog */}
      <UpgradeDialog
        open={showUpgradeDialog}
        onOpenChange={setShowUpgradeDialog}
        isRemote={isRemote}
        instanceId={instanceId}
        currentVersion={version || ""}
        latestVersion={updateInfo?.latest || ""}
      />
    </div>
  );
}
