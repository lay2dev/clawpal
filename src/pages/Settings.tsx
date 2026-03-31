import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AutocompleteField } from "../components/AutocompleteField";
import { useAppUpdate } from "../hooks/useAppUpdate";
import {
  emptyForm, normalizeOauthProvider, providerUsesOAuthAuth,
  defaultOauthAuthRef, isEnvVarLikeAuthRef, defaultEnvAuthRef,
  inferCredentialSource, providerSupportsOptionalApiKey,
  type ProfileForm, type CredentialSource,
} from "../lib/profile-utils";
import type { FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { hasGuidanceEmitted, useApi } from "@/lib/use-api";
import { api } from "@/lib/api";
import { isAlreadyExplainedGuidanceError } from "@/lib/guidance";
import { useTheme } from "@/lib/use-theme";
import { useFont } from "@/lib/use-font";
import type { UiFont } from "@/lib/use-font";
import { resolveProfileCredentialView } from "@/lib/profile-credential";
import type {
  ModelCatalogProvider,
  ModelProfile,
  ProviderAuthSuggestion,
  ResolvedApiKey,
  SshHost,
} from "@/lib/types";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { BugReportSettings } from "@/components/BugReportSettings";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { SettingsAlphaFeaturesCard } from "@/components/SettingsAlphaFeaturesCard";
import { getSettingsProfileUiState } from "./settings-profile-ui";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
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
import { PlusIcon, RefreshCwIcon } from "lucide-react";


const MODEL_CATALOG_CACHE_TTL_MS = 5 * 60_000;
let modelCatalogCache: { value: ModelCatalogProvider[]; expiresAt: number } | null = null;
let profilesExtractedOnce = false;
let syncSelectionSessionCache: string[] = [];
const PROVIDER_FALLBACK_OPTIONS = [
  "openai",
  "openai-codex",
  "anthropic",
  "openrouter",
  "ollama",
  "lmstudio",
  "localai",
  "vllm",
];

// Profile utility functions extracted to ../lib/profile-utils.ts

// AutocompleteField extracted to ../components/AutocompleteField.tsx

export function Settings({
  onDataChange,
  hasAppUpdate,
  onAppUpdateSeen,
  globalMode = false,
  section = "all",
  onOpenDoctor,
  onConnectDevice,
}: {
  onDataChange?: () => void;
  hasAppUpdate?: boolean;
  onAppUpdateSeen?: () => void;
  globalMode?: boolean;
  section?: "all" | "profiles" | "preferences";
  onOpenDoctor?: () => void;
  onConnectDevice?: (hostId: string) => Promise<boolean>;
}) {
  const { t, i18n } = useTranslation();
  const ua = useApi();
  const { theme, setTheme } = useTheme();
  const { font, setFont } = useFont();
  const [profiles, setProfiles] = useState<ModelProfile[] | null>(null);
  const [catalog, setCatalog] = useState<ModelCatalogProvider[]>([]);
  const [apiKeys, setApiKeys] = useState<ResolvedApiKey[]>([]);
  const [form, setForm] = useState<ProfileForm>(emptyForm());
  const [credentialSource, setCredentialSource] = useState<CredentialSource>("manual");
  const [profileDialogOpen, setProfileDialogOpen] = useState(false);
  const [authSuggestion, setAuthSuggestion] = useState<ProviderAuthSuggestion | null>(null);
  const [testingProfileId, setTestingProfileId] = useState<string | null>(null);
  const [showSshTransferSpeedUi, setShowSshTransferSpeedUi] = useState(false);
  const [remoteDevices, setRemoteDevices] = useState<SshHost[]>([]);
  const [syncDialogOpen, setSyncDialogOpen] = useState(false);
  const [selectedSyncHostIds, setSelectedSyncHostIds] = useState<string[]>(() => [...syncSelectionSessionCache]);
  const [syncStatusByHostId, setSyncStatusByHostId] = useState<Record<string, "idle" | "syncing" | "success" | "failed">>({});
  const [hostConnectionById, setHostConnectionById] = useState<Record<string, boolean>>({});

  const [catalogRefreshed, setCatalogRefreshed] = useState(false);

  // ClawPal app version & self-update
  const { appVersion, appUpdate, appUpdateChecking, appUpdating, appUpdateProgress, handleCheckForUpdates, handleAppUpdate } = useAppUpdate(hasAppUpdate, onAppUpdateSeen);

  // Extract profiles from config on first load
  useEffect(() => {
    if (profilesExtractedOnce) return;
    profilesExtractedOnce = true;
    ua.extractModelProfilesFromConfig()
      .catch((e) => {
        profilesExtractedOnce = false;
        console.error("Failed to extract profiles:", e);
      });
  }, [ua]);

  const refreshProfiles = () => {
    const withTimeout = <T,>(promise: Promise<T>, timeoutMs: number, fallback: T): Promise<T> =>
      Promise.race([
        promise,
        new Promise<T>((resolve) => setTimeout(() => resolve(fallback), timeoutMs)),
      ]);

    withTimeout(ua.listModelProfiles(), 8000, [])
      .then(setProfiles)
      .catch((e) => {
        console.error("Failed to load profiles:", e);
        setProfiles([]);
      });
    withTimeout(ua.resolveApiKeys(), 8000, [])
      .then(setApiKeys)
      .catch((e) => {
        console.error("Failed to resolve API keys:", e);
        setApiKeys([]);
      });
  };

  useEffect(refreshProfiles, [ua]);

  useEffect(() => {
    ua.listSshHosts()
      .then((hosts) => setRemoteDevices(hosts))
      .catch((e) => {
        console.error("Failed to load SSH hosts:", e);
        setRemoteDevices([]);
      });
  }, [ua]);

  useEffect(() => {
    if (!syncDialogOpen || remoteDevices.length === 0) return;
    let cancelled = false;
    Promise.all(
      remoteDevices.map(async (device) => {
        try {
          const status = await ua.sshStatus(device.id);
          const text = String(status).toLowerCase();
          const connected = text.includes("connected") && !text.includes("disconnected") && !text.includes("no connection");
          return [device.id, connected] as const;
        } catch {
          return [device.id, false] as const;
        }
      }),
    ).then((pairs) => {
      if (cancelled) return;
      const next = Object.fromEntries(pairs);
      setHostConnectionById(next);
      setSelectedSyncHostIds((prev) => prev.filter((id) => next[id]));
    });
    return () => {
      cancelled = true;
    };
  }, [remoteDevices, syncDialogOpen, ua]);

  useEffect(() => {
    syncSelectionSessionCache = selectedSyncHostIds;
  }, [selectedSyncHostIds]);

  useEffect(() => {
    ua.getAppPreferences()
      .then((prefs) => {
        setShowSshTransferSpeedUi(Boolean(prefs.showSshTransferSpeedUi));
      })
      .catch((e) => console.error("Failed to load app preferences:", e));
  }, [ua]);

  // Load catalog on mount
  useEffect(() => {
    const now = Date.now();
    if (modelCatalogCache && modelCatalogCache.expiresAt > now) {
      setCatalog(modelCatalogCache.value);
      setCatalogRefreshed(true);
      return;
    }
    setCatalogRefreshed(false);
    ua.refreshModelCatalog()
      .then((fresh) => {
        setCatalog(fresh);
        modelCatalogCache = {
          value: fresh,
          expiresAt: Date.now() + MODEL_CATALOG_CACHE_TTL_MS,
        };
      })
      .catch((e) => console.error("Failed to load model catalog:", e));
  }, [ua]);

  // Refresh catalog from CLI when user focuses provider/model input
  const ensureCatalog = () => {
    if (catalogRefreshed) return;
    setCatalogRefreshed(true);
    ua.refreshModelCatalog().then((fresh) => {
      if (fresh.length > 0) setCatalog(fresh);
      modelCatalogCache = {
        value: fresh,
        expiresAt: Date.now() + MODEL_CATALOG_CACHE_TTL_MS,
      };
    }).catch((e) => console.error("Failed to refresh model catalog:", e));
  };

  const resolvedCredentialMap = useMemo(() => {
    const map = new Map<string, ResolvedApiKey>();
    for (const entry of apiKeys) {
      map.set(entry.profileId, entry);
    }
    return map;
  }, [apiKeys]);

  // Check for existing auth when provider changes
  useEffect(() => {
    if (form.id || !form.provider.trim()) {
      setAuthSuggestion(null);
      return;
    }
    if (ua.isRemote) {
      // For remote: infer from existing profiles
      const existing = (profiles || []).find(
        (p) => {
          if (p.provider !== form.provider) return false;
          const credential = resolveProfileCredentialView(p, resolvedCredentialMap.get(p.id));
          return credential.resolved;
        }
      );
      if (existing) {
        const credential = resolveProfileCredentialView(
          existing,
          resolvedCredentialMap.get(existing.id),
        );
        setAuthSuggestion({
          hasKey: true,
          source: `existing profile (${existing.provider}/${existing.model})`,
          authRef: credential.authRef || existing.authRef || "",
        });
      } else {
        setAuthSuggestion(null);
      }
    } else {
      ua.resolveProviderAuth(form.provider)
        .then(setAuthSuggestion)
        .catch((e) => { console.error("Failed to resolve provider auth:", e); setAuthSuggestion(null); });
    }
  }, [form.provider, form.id, ua, profiles, resolvedCredentialMap]);

  useEffect(() => {
    if (!providerUsesOAuthAuth(form.provider) && credentialSource === "oauth") {
      setCredentialSource("env");
    }
  }, [form.provider, credentialSource]);

  const modelCandidates = useMemo(() => {
    const found = catalog.find((c) => c.provider === form.provider);
    return found?.models || [];
  }, [catalog, form.provider]);

  const providerCandidates = useMemo(() => {
    const set = new Set<string>();
    for (const provider of PROVIDER_FALLBACK_OPTIONS) {
      if (provider.trim()) set.add(provider);
    }
    for (const item of catalog) {
      const provider = item.provider.trim();
      if (provider) set.add(provider);
    }
    for (const profile of profiles || []) {
      const provider = profile.provider.trim();
      if (provider) set.add(provider);
    }
    return Array.from(set).sort((a, b) => a.localeCompare(b));
  }, [catalog, profiles]);

  const saveProfile = async (authRefOverride?: string): Promise<boolean> => {
    if (!form.provider || !form.model) {
      toast.error(t('settings.providerModelRequired'));
      return false;
    }
    const apiKeyOptional = form.useCustomUrl || providerSupportsOptionalApiKey(form.provider);
    const oauthSource = credentialSource === "oauth" && providerUsesOAuthAuth(form.provider);
    const envSource = credentialSource === "env";
    const manualSource = credentialSource === "manual";
    if (!ua.isRemote && manualSource && !form.apiKey && !form.id && !apiKeyOptional) {
      toast.error(t('settings.apiKeyRequired'));
      return false;
    }
    const overrideAuthRef = (authRefOverride || "").trim();
    const explicitAuthRef = form.authRef.trim();
    const oauthFallbackAuthRef = defaultOauthAuthRef(form.provider);
    const resolvedAuthRef = oauthSource
      ? (overrideAuthRef || explicitAuthRef || oauthFallbackAuthRef)
      : envSource
        ? (
          overrideAuthRef
          || explicitAuthRef
          || ((!form.apiKey && authSuggestion?.authRef) ? authSuggestion.authRef : "")
        )
        : "";
    const profileData: ModelProfile = {
      id: form.id || "",
      name: `${form.provider}/${form.model}`,
      provider: form.provider,
      model: form.model,
      authRef: resolvedAuthRef,
      apiKey: form.apiKey || undefined,
      baseUrl: form.useCustomUrl && form.baseUrl ? form.baseUrl : undefined,
      enabled: form.enabled,
    };
    try {
      await ua.upsertModelProfile(profileData);
      toast.success(t('settings.profileSaved'));
      setForm(emptyForm());
      setProfileDialogOpen(false);
      refreshProfiles();
      onDataChange?.();
      return true;
    } catch (e) {
      const errorText = e instanceof Error ? e.message : String(e);
      toast.error(t('settings.saveFailed', { error: errorText }));
      return false;
    }
  };

  const upsert = (event: FormEvent) => {
    event.preventDefault();
    void saveProfile();
  };

  const editProfile = (profile: ModelProfile) => {
    setCredentialSource(inferCredentialSource(profile.provider, profile.authRef || ""));
    setForm({
      id: profile.id,
      provider: profile.provider,
      model: profile.model,
      authRef: profile.authRef || "",
      apiKey: "",
      useCustomUrl: !!profile.baseUrl,
      baseUrl: profile.baseUrl || "",
      enabled: profile.enabled,
    });
    setProfileDialogOpen(true);
  };

  const openAddProfile = () => {
    setCredentialSource("manual");
    setForm(emptyForm());
    setProfileDialogOpen(true);
  };

  const deleteProfile = (id: string) => {
    ua.deleteModelProfile(id)
      .then(() => {
        toast.success(t('settings.profileDeleted'));
        if (form.id === id) {
          setForm(emptyForm());
        }
        refreshProfiles();
        onDataChange?.();
      })
      .catch((e) => {
        const errorText = e instanceof Error ? e.message : String(e);
        toast.error(t('settings.deleteFailed', { error: errorText }));
      });
  };

  const toggleProfileEnabled = (profile: ModelProfile) => {
    const nextEnabled = !profile.enabled;
    ua.upsertModelProfile({
      ...profile,
      enabled: nextEnabled,
    })
      .then(() => {
        const message = nextEnabled
          ? t('settings.profileEnabledMessage', { name: `${profile.provider}/${profile.model}` })
          : t('settings.profileDisabledMessage', { name: `${profile.provider}/${profile.model}` });
        toast.success(message);
        refreshProfiles();
        onDataChange?.();
      })
      .catch((e) => {
        const errorText = e instanceof Error ? e.message : String(e);
        toast.error(t('settings.saveFailed', { error: errorText }));
      });
  };

  const testProfile = async (profile: ModelProfile) => {
    if (!profile.enabled) {
      toast.error(t('settings.testProfileDisabled'));
      return;
    }
    setTestingProfileId(profile.id);
    try {
      await ua.testModelProfile(profile.id);

      toast.success(
        t('settings.testProfileSuccess', {
          name: `${profile.provider}/${profile.model}`,
        }),
      );
    } catch (e) {
      const errorText = e instanceof Error ? e.message : String(e);
      if (hasGuidanceEmitted(e) || isAlreadyExplainedGuidanceError(errorText)) {
        if (onOpenDoctor) {
          toast.error(
            <div className="space-y-2">
              <p>{t('settings.testProfileFailed', { error: t('home.fixInDoctor') })}</p>
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    onOpenDoctor();
                  }}
                >
                  {t("home.fixInDoctor")}
                </Button>
              </div>
            </div>
          );
        }
        return;
      }
      toast.error(
        <div className="space-y-2">
          <p>{t('settings.testProfileFailed', { error: errorText })}</p>
          <div className="flex flex-wrap gap-2">
          {onOpenDoctor && (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => {
                onOpenDoctor();
              }}
            >
              {t("home.fixInDoctor")}
            </Button>
          )}
          </div>
        </div>
      );
    } finally {
      setTestingProfileId(null);
    }
  };

  const showProfiles = section !== "preferences";
  const showPreferences = section !== "profiles";

  const syncedDeviceCount = useMemo(() => {
    const ids = new Set<string>();
    for (const profile of profiles || []) {
      const source = profile.syncSourceHostId?.trim();
      if (source) ids.add(source);
    }
    return ids.size;
  }, [profiles]);

  const syncButtonText = remoteDevices.length > 0 && selectedSyncHostIds.length === 0
    ? t("settings.syncFromDevices")
    : t("settings.syncFromDevicesAction", { count: selectedSyncHostIds.length });

  const isDeviceSyncing = useMemo(
    () => Object.values(syncStatusByHostId).some((status) => status === "syncing"),
    [syncStatusByHostId],
  );

  const formatSyncedAt = (value?: string) => {
    if (!value) return "-";
    const timestamp = Date.parse(value);
    if (Number.isNaN(timestamp)) return value;
    return new Date(timestamp).toLocaleString();
  };

  const runDeviceSync = useCallback(async () => {
    if (selectedSyncHostIds.length === 0) {
      toast.message(t("settings.syncFromDevices"));
      return;
    }

    toast.success(t("settings.syncStarted", { count: selectedSyncHostIds.length }));
    setSyncDialogOpen(false);

    for (const hostId of selectedSyncHostIds) {
      const device = remoteDevices.find((item) => item.id === hostId);
      const deviceName = device?.label || hostId;
      setSyncStatusByHostId((prev) => ({ ...prev, [hostId]: "syncing" }));
      try {
        await api.remoteSyncProfilesToLocalAuth(hostId, deviceName);
        setSyncStatusByHostId((prev) => ({ ...prev, [hostId]: "success" }));
      } catch (error) {
        const errorText = error instanceof Error ? error.message : String(error);
        setSyncStatusByHostId((prev) => ({ ...prev, [hostId]: "failed" }));
        toast.error(t("settings.syncFailedForDevice", { device: deviceName, error: errorText }));
      }
    }
    refreshProfiles();
  }, [remoteDevices, selectedSyncHostIds, ua]);

  const handleSshTransferSpeedUiToggle = useCallback((nextChecked: boolean) => {
    setShowSshTransferSpeedUi(nextChecked);
    ua.setSshTransferSpeedUiPreference(nextChecked)
      .then((prefs) => {
        setShowSshTransferSpeedUi(Boolean(prefs.showSshTransferSpeedUi));
      })
      .catch((e) => {
        setShowSshTransferSpeedUi((current) => !current);
        const errorText = e instanceof Error ? e.message : String(e);
        toast.error(t("settings.sshTransferSpeedUiSaveFailed", { error: errorText }));
      });
  }, [t, ua]);

  return (
    <section>
      <h2 className="text-2xl font-bold mb-4">{t('settings.title')}</h2>

      {/* ---- Model Profiles ---- */}
      {showProfiles && !ua.isRemote && (
        <p className="text-sm text-muted-foreground mb-4">
          {t('settings.oauthHint')}
        </p>
      )}

          <div className="space-y-3">
            {/* Preferences: Version, Language, Theme */}
            {showPreferences && (
            <Card>
              <CardContent className="space-y-4">
                {/* Version */}
                <div className="flex items-center justify-between flex-wrap gap-2">
                  <Label className="text-sm font-semibold">{t('settings.currentVersion')}</Label>
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="text-sm font-medium">{appVersion ? `v${appVersion}` : "..."}</span>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={handleCheckForUpdates}
                      disabled={appUpdateChecking || appUpdating}
                    >
                      {appUpdateChecking ? t('settings.checkingUpdates') : t('settings.checkForUpdates')}
                    </Button>
                  </div>
                </div>
                {!appUpdateChecking && appUpdate && !appUpdating && (
                  <div className="flex items-center gap-2">
                    <Badge variant="outline" className="text-primary border-primary">
                      {t('settings.updateAvailable', { version: appUpdate.version })}
                    </Badge>
                    <Button size="sm" onClick={handleAppUpdate}>
                      {t('settings.updateRestart')}
                    </Button>
                  </div>
                )}
                {appUpdating && (
                  <div className="flex items-center gap-2">
                    <Badge variant="outline" className="text-muted-foreground">
                      {appUpdateProgress !== null && appUpdateProgress < 100
                        ? t('settings.downloading', { progress: appUpdateProgress })
                        : appUpdateProgress === 100
                          ? t('settings.installing')
                          : t('settings.preparing')}
                    </Badge>
                    {appUpdateProgress !== null && appUpdateProgress < 100 && (
                      <div className="w-32 h-1.5 bg-muted rounded-full overflow-hidden">
                        <div
                          className="h-full bg-primary rounded-full transition-all"
                          style={{ width: `${appUpdateProgress}%` }}
                        />
                      </div>
                    )}
                  </div>
                )}

                <div className="h-px bg-border" />

                {/* Language & Theme */}
                <div className="flex items-center justify-between flex-wrap gap-3">
                  <div className="flex items-center gap-3">
                    <Label className="text-sm font-semibold shrink-0">{t('settings.language')}</Label>
                    <Select
                      value={i18n.language?.startsWith('zh') ? 'zh' : 'en'}
                      onValueChange={(val) => i18n.changeLanguage(val)}
                    >
                      <SelectTrigger className="w-[140px]">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="en">English</SelectItem>
                        <SelectItem value="zh">简体中文</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="flex items-center gap-3">
                    <Label className="text-sm font-semibold shrink-0">{t('settings.theme')}</Label>
                    <Select value={theme} onValueChange={setTheme}>
                      <SelectTrigger className="w-[140px]">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="light">{t('settings.themeLight')}</SelectItem>
                        <SelectItem value="dark">{t('settings.themeDark')}</SelectItem>
                        <SelectItem value="system">{t('settings.themeSystem')}</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="flex items-center gap-3">
                    <Label className="text-sm font-semibold shrink-0">{t('settings.font')}</Label>
                    <Select value={font} onValueChange={(val) => setFont(val as UiFont)}>
                      <SelectTrigger className="w-[160px]">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="wenkai">{t('settings.fontWenkai')}</SelectItem>
                        <SelectItem value="nunito">{t('settings.fontNunito')}</SelectItem>
                        <SelectItem value="system">{t('settings.fontSystem')}</SelectItem>
                        <SelectItem value="serif">{t('settings.fontSerif')}</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                </div>

              </CardContent>
            </Card>
            )}

            {/* Profiles list */}
            {showProfiles && (
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between gap-2 flex-wrap">
                  <CardTitle>{t('settings.modelProfiles')}</CardTitle>
                  <div className="flex items-center gap-2 flex-wrap">
                    <Button
                      size="icon"
                      variant="outline"
                      onClick={() => setSyncDialogOpen(true)}
                      title={t("settings.syncedDevicesCount", { count: syncedDeviceCount })}
                      aria-label={t("settings.syncedDevicesCount", { count: syncedDeviceCount })}
                    >
                      <RefreshCwIcon className={`h-4 w-4 ${isDeviceSyncing ? "animate-spin" : ""}`} />
                    </Button>
                    <Button
                      size="icon"
                      onClick={openAddProfile}
                      title={t("settings.addProfile")}
                      aria-label={t("settings.addProfile")}
                    >
                      <PlusIcon className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              </CardHeader>
              <CardContent>
                {profiles === null ? (
                  <p className="text-muted-foreground">{t('settings.loadingProfiles')}</p>
                ) : profiles.length === 0 ? (
                  <p className="text-muted-foreground">{t('settings.noProfiles')}</p>
                ) : null}
                <div className="grid gap-2">
                  {(profiles || []).map((profile) => {
                    const profileUi = getSettingsProfileUiState(profile);
                    const credential = resolveProfileCredentialView(
                      profile,
                      resolvedCredentialMap.get(profile.id),
                    );
                    const statusLower = credential.status.trim().toLowerCase();
                    const credentialStatusText =
                      credential.kind === "oauth" && statusLower !== "..."
                        ? (credential.resolved
                          ? t("settings.credentialStatusOauthReady")
                          : credential.status)
                        : credential.status;
                    const showCredentialRef = credential.kind === "env_ref";
                    const showCredentialStatus = credential.kind !== "env_ref";
                    return (
                      <div
                        key={profile.id}
                        className="border border-border p-2.5 rounded-lg"
                      >
                        <div className="flex justify-between items-center">
                          <strong>{profile.provider}/{profile.model}</strong>
                          {profileUi.showEnabledBadge && profile.enabled ? (
                            <Badge className="bg-blue-500/10 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400">
                              {t('settings.enabled')}
                            </Badge>
                          ) : profileUi.showEnabledBadge ? (
                            <Badge className="bg-red-500/10 text-red-600 dark:bg-red-500/15 dark:text-red-400">
                              {t('settings.disabled')}
                            </Badge>
                          ) : null}
                        </div>
                        <div className="flex flex-wrap items-center gap-1.5 mt-1">
                          <Badge variant="outline" title={`${t('settings.credential')}: ${t(`settings.credentialKind.${credential.kind}`)}`}>
                            {t(`settings.credentialKind.${credential.kind}`)}
                          </Badge>
                          {showCredentialRef && (
                            <Badge
                              variant="outline"
                              title={`${t("settings.credentialRef")}: ${credential.authRef || "-"}`}
                            >
                              Ref
                            </Badge>
                          )}
                          {showCredentialStatus && (
                            <Badge
                              variant="outline"
                              title={`${t("settings.credentialStatus")}: ${credentialStatusText}`}
                            >
                              {credentialStatusText}
                            </Badge>
                          )}
                          {profile.baseUrl && (
                            <Badge variant="outline" title={`URL: ${profile.baseUrl}`}>
                              URL
                            </Badge>
                          )}
                          {(profile.syncSourceDeviceName || profile.syncSyncedAt) && (
                            <Badge
                              variant="outline"
                              title={t("settings.profileSyncSource", {
                                device: profile.syncSourceDeviceName || "-",
                                syncedAt: formatSyncedAt(profile.syncSyncedAt),
                              })}
                            >
                              {profile.syncSourceDeviceName || "-"}
                            </Badge>
                          )}
                        </div>
                        <div className="flex gap-1.5 mt-1.5">
                          {profileUi.actions.includes("edit") ? (
                            <Button
                              size="sm"
                              variant="outline"
                              type="button"
                              onClick={() => editProfile(profile)}
                            >
                              {t('settings.edit')}
                            </Button>
                          ) : null}
                          {profileUi.actions.includes("delete") ? (
                            <AlertDialog>
                              <AlertDialogTrigger asChild>
                                <Button size="sm" variant="destructive" type="button">
                                  {t('settings.delete')}
                                </Button>
                              </AlertDialogTrigger>
                              <AlertDialogContent>
                                <AlertDialogHeader>
                                  <AlertDialogTitle>{t('settings.deleteProfileTitle')}</AlertDialogTitle>
                                  <AlertDialogDescription>
                                    {t('settings.deleteProfileDescription', { name: `${profile.provider}/${profile.model}` })}
                                  </AlertDialogDescription>
                                </AlertDialogHeader>
                                <AlertDialogFooter>
                                  <AlertDialogCancel>{t('settings.cancel')}</AlertDialogCancel>
                                  <AlertDialogAction
                                    className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                                    onClick={() => deleteProfile(profile.id)}
                                  >
                                    {t('settings.delete')}
                                  </AlertDialogAction>
                                </AlertDialogFooter>
                              </AlertDialogContent>
                            </AlertDialog>
                          ) : null}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </CardContent>
            </Card>
            )}

            {showPreferences && (
              <SettingsAlphaFeaturesCard
                showSshTransferSpeedUi={showSshTransferSpeedUi}
                onSshTransferSpeedUiToggle={handleSshTransferSpeedUiToggle}
              />
            )}

            {showPreferences && <BugReportSettings />}
          </div>

      <Dialog open={syncDialogOpen} onOpenChange={setSyncDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("settings.syncDevicesTitle")}</DialogTitle>
          </DialogHeader>
          <div className="space-y-2 max-h-[320px] overflow-y-auto">
            {remoteDevices.length === 0 ? (
              <p className="text-sm text-muted-foreground">{t("settings.noSyncDevices")}</p>
            ) : remoteDevices.map((device) => {
              const checked = selectedSyncHostIds.includes(device.id);
              const connected = hostConnectionById[device.id] ?? false;
              const status = syncStatusByHostId[device.id] || "idle";
              const statusText = status === "syncing"
                ? t("settings.syncStatusSyncing")
                : status === "success"
                  ? t("settings.syncStatusSuccess")
                  : status === "failed"
                    ? t("settings.syncStatusFailed")
                    : t("settings.syncStatusIdle");
              return (
                <label key={device.id} className={`flex items-center justify-between gap-3 border border-border rounded-md px-3 py-2 ${connected ? "" : "opacity-70"}`}>
                  <div className="flex items-center gap-2">
                    <Checkbox
                      checked={checked}
                      onCheckedChange={async (value) => {
                        const enabled = Boolean(value);
                        if (!enabled) {
                          setSelectedSyncHostIds((prev) => prev.filter((id) => id !== device.id));
                          return;
                        }
                        if (!connected) {
                          if (!onConnectDevice) return;
                          const connectedNow = await onConnectDevice(device.id);
                          if (!connectedNow) return;
                          setHostConnectionById((prev) => ({ ...prev, [device.id]: true }));
                        }
                        setSelectedSyncHostIds((prev) => prev.includes(device.id) ? prev : [...prev, device.id]);
                      }}
                    />
                    <span className={`text-sm ${connected ? "" : "text-muted-foreground"}`}>{device.label}</span>
                  </div>
                  <span className="text-xs text-muted-foreground">
                    {connected ? statusText : t("settings.disconnected")}
                  </span>
                </label>
              );
            })}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setSyncDialogOpen(false)}>{t("settings.cancel")}</Button>
            <Button onClick={runDeviceSync}>{syncButtonText}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Add / Edit Profile Dialog */}
      <Dialog open={profileDialogOpen} onOpenChange={(open) => {
        setProfileDialogOpen(open);
        if (!open) {
          setCredentialSource("manual");
          setForm(emptyForm());
        }
      }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{form.id ? t('settings.editProfile') : t('settings.addProfile')}</DialogTitle>
          </DialogHeader>
          <form onSubmit={upsert} className="space-y-4">
            <div className="space-y-1.5">
              <Label>{t('settings.provider')}</Label>
              <AutocompleteField
                value={form.provider}
                onChange={(val) => {
                  const nextSource: CredentialSource = providerUsesOAuthAuth(val)
                    ? (credentialSource === "manual" ? "manual" : "oauth")
                    : (credentialSource === "oauth" ? "env" : credentialSource);
                  setCredentialSource(nextSource);
                  setForm((p) => ({
                    ...p,
                    provider: val,
                    model: "",
                    authRef: p.id
                      ? p.authRef
                      : providerUsesOAuthAuth(val)
                        ? defaultOauthAuthRef(val)
                        : (nextSource === "env" ? (p.authRef || defaultEnvAuthRef(val)) : p.authRef),
                  }));
                }}
                onFocus={ensureCatalog}
                options={providerCandidates.map((provider) => ({
                  value: provider,
                  label: provider,
                }))}
                placeholder="e.g. openai"
              />
            </div>

            <div className="space-y-1.5">
              <Label>{t('settings.model')}</Label>
              <AutocompleteField
                value={form.model}
                onChange={(val) =>
                  setForm((p) => ({ ...p, model: val }))
                }
                onFocus={ensureCatalog}
                options={modelCandidates.map((m) => ({
                  value: m.id,
                  label: m.name || m.id,
                }))}
                placeholder="e.g. gpt-4o"
              />
            </div>

            <div className="space-y-1.5">
              <Label>{t('settings.credentialSource')}</Label>
              <Select
                value={credentialSource}
                onValueChange={(val) => {
                  const next = val as CredentialSource;
                  if (next === "oauth" && !providerUsesOAuthAuth(form.provider)) {
                    return;
                  }
                  setCredentialSource(next);
                  setForm((p) => {
                    if (next === "oauth") {
                      const oauthRef = p.authRef.trim();
                      return {
                        ...p,
                        apiKey: "",
                        authRef: oauthRef && !isEnvVarLikeAuthRef(oauthRef)
                          ? oauthRef
                          : defaultOauthAuthRef(p.provider),
                      };
                    }
                    if (next === "env") {
                      const currentRef = p.authRef.trim();
                      return {
                        ...p,
                        authRef: currentRef || defaultEnvAuthRef(p.provider),
                      };
                    }
                    return p;
                  });
                }}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {providerUsesOAuthAuth(form.provider) && (
                    <SelectItem value="oauth">{t("settings.credentialSourceOauth")}</SelectItem>
                  )}
                  <SelectItem value="env">{t("settings.credentialSourceEnv")}</SelectItem>
                  <SelectItem value="manual">{t("settings.credentialSourceManual")}</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {credentialSource === "oauth" && providerUsesOAuthAuth(form.provider) && (
              <div className="rounded-md border border-border/70 bg-muted/30 px-3 py-2 text-xs text-muted-foreground space-y-2">
                <p>{t("settings.oauthProviderHint", { provider: normalizeOauthProvider(form.provider) })}</p>
                <p>{t("settings.oauthManualHint")}</p>
              </div>
            )}

            {credentialSource === "env" && (
              <div className="space-y-1.5">
                <Label>{t('settings.authRef')}</Label>
                <Input
                  placeholder={defaultEnvAuthRef(form.provider) || "OPENAI_API_KEY"}
                  value={form.authRef}
                  onChange={(e) =>
                    setForm((p) => ({ ...p, authRef: e.target.value }))
                  }
                />
                <p className="text-xs text-muted-foreground">
                  {t("settings.credentialSourceEnvHint")}
                </p>
              </div>
            )}

            {credentialSource === "manual" && (
              <div className="space-y-1.5">
                <Label>{t('settings.apiKey')}</Label>
                <Input
                  type="password"
                  placeholder={form.id
                    ? t('settings.apiKeyUnchanged')
                    : (authSuggestion?.hasKey || form.useCustomUrl || providerSupportsOptionalApiKey(form.provider))
                      ? t('settings.apiKeyOptional')
                      : t('settings.apiKeyPlaceholder')}
                  value={form.apiKey}
                  onChange={(e) =>
                    setForm((p) => ({ ...p, apiKey: e.target.value }))
                  }
                />
                {!form.id && authSuggestion?.hasKey && (
                  <p className="text-xs text-muted-foreground">
                    {t('settings.keyAvailable', { source: authSuggestion.source })}
                  </p>
                )}
              </div>
            )}

            <div className="flex items-center gap-2">
              <Checkbox
                id="custom-url"
                checked={form.useCustomUrl}
                onCheckedChange={(checked) =>
                  setForm((p) => ({ ...p, useCustomUrl: checked === true }))
                }
              />
              <Label htmlFor="custom-url">{t('settings.customBaseUrl')}</Label>
            </div>

            {form.useCustomUrl && (
              <div className="space-y-1.5">
                <Label>{t('settings.baseUrl')}</Label>
                <Input
                  placeholder="e.g. https://api.openai.com/v1"
                  value={form.baseUrl}
                  onChange={(e) =>
                    setForm((p) => ({ ...p, baseUrl: e.target.value }))
                  }
                />
              </div>
            )}

            <DialogFooter>
              {form.id && (
                <AlertDialog>
                  <AlertDialogTrigger asChild>
                    <Button type="button" variant="destructive" className="mr-auto">
                      {t('settings.delete')}
                    </Button>
                  </AlertDialogTrigger>
                  <AlertDialogContent>
                    <AlertDialogHeader>
                      <AlertDialogTitle>{t('settings.deleteProfileTitle')}</AlertDialogTitle>
                      <AlertDialogDescription>
                        {t('settings.deleteProfileDescription', { name: `${form.provider}/${form.model}` })}
                      </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                      <AlertDialogCancel>{t('settings.cancel')}</AlertDialogCancel>
                      <AlertDialogAction
                        className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                        onClick={() => { deleteProfile(form.id); setProfileDialogOpen(false); }}
                      >
                        {t('settings.delete')}
                      </AlertDialogAction>
                    </AlertDialogFooter>
                  </AlertDialogContent>
                </AlertDialog>
              )}
              <Button type="button" variant="outline" onClick={() => setProfileDialogOpen(false)}>
                {t('settings.cancel')}
              </Button>
              <Button type="submit">{t('settings.save')}</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

    </section>
  );
}
