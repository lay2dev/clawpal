import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { api } from "@/lib/api";
import { useInstance } from "@/lib/instance-context";
import type { ModelCatalogProvider, ModelProfile, ProviderAuthSuggestion, ResolvedApiKey } from "@/lib/types";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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

type ProfileForm = {
  id: string;
  provider: string;
  model: string;
  apiKey: string;
  useCustomUrl: boolean;
  baseUrl: string;
  enabled: boolean;
};

function emptyForm(): ProfileForm {
  return {
    id: "",
    provider: "",
    model: "",
    apiKey: "",
    useCustomUrl: false,
    baseUrl: "",
    enabled: true,
  };
}

function AutocompleteField({
  value,
  onChange,
  onFocus,
  options,
  placeholder,
}: {
  value: string;
  onChange: (val: string) => void;
  onFocus?: () => void;
  options: { value: string; label: string }[];
  placeholder: string;
}) {
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);

  const filtered = options.filter(
    (o) =>
      !value ||
      o.value.toLowerCase().includes(value.toLowerCase()) ||
      o.label.toLowerCase().includes(value.toLowerCase()),
  );

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  return (
    <div ref={wrapperRef} className="relative">
      <Input
        placeholder={placeholder}
        value={value}
        onChange={(e) => {
          onChange(e.target.value);
          setOpen(true);
        }}
        onFocus={() => {
          setOpen(true);
          onFocus?.();
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape") setOpen(false);
        }}
      />
      {open && filtered.length > 0 && (
        <div className="absolute z-50 w-full mt-1 bg-popover border border-border rounded-md shadow-md max-h-[200px] overflow-y-auto">
          {filtered.map((option) => (
            <div
              key={option.value}
              className="px-3 py-1.5 text-sm cursor-pointer hover:bg-accent hover:text-accent-foreground"
              onMouseDown={(e) => {
                e.preventDefault();
                onChange(option.value);
                setOpen(false);
              }}
            >
              {option.label}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export function Settings({ onDataChange }: { onDataChange?: () => void }) {
  const { instanceId, isRemote, isConnected } = useInstance();
  const [profiles, setProfiles] = useState<ModelProfile[] | null>(null);
  const [catalog, setCatalog] = useState<ModelCatalogProvider[]>([]);
  const [apiKeys, setApiKeys] = useState<ResolvedApiKey[]>([]);
  const [form, setForm] = useState<ProfileForm>(emptyForm());
  const [message, setMessage] = useState("");
  const [authSuggestion, setAuthSuggestion] = useState<ProviderAuthSuggestion | null>(null);

  const [catalogRefreshed, setCatalogRefreshed] = useState(false);

  // ClawPal app version & self-update
  const [appVersion, setAppVersion] = useState<string>("");
  const [appUpdate, setAppUpdate] = useState<{ version: string; body?: string } | null>(null);
  const [appUpdateChecking, setAppUpdateChecking] = useState(false);
  const [appUpdating, setAppUpdating] = useState(false);
  const [appUpdateProgress, setAppUpdateProgress] = useState<number | null>(null);

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  const handleCheckForUpdates = useCallback(async () => {
    setAppUpdateChecking(true);
    setAppUpdate(null);
    try {
      const update = await check();
      if (update) {
        setAppUpdate({ version: update.version, body: update.body });
      }
    } catch (e) {
      console.error("Update check failed:", e);
    } finally {
      setAppUpdateChecking(false);
    }
  }, []);

  const handleAppUpdate = useCallback(async () => {
    setAppUpdating(true);
    setAppUpdateProgress(0);
    try {
      const update = await check();
      if (!update) return;
      let totalBytes = 0;
      let downloadedBytes = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started" && event.data.contentLength) {
          totalBytes = event.data.contentLength;
        } else if (event.event === "Progress") {
          downloadedBytes += event.data.chunkLength;
          if (totalBytes > 0) {
            setAppUpdateProgress(Math.round((downloadedBytes / totalBytes) * 100));
          }
        } else if (event.event === "Finished") {
          setAppUpdateProgress(100);
        }
      });
      await relaunch();
    } catch (e) {
      console.error("App update failed:", e);
      setAppUpdating(false);
      setAppUpdateProgress(null);
    }
  }, []);


  // Extract profiles from remote config on first load
  useEffect(() => {
    if (!isRemote || !isConnected) return;
    api.remoteExtractModelProfilesFromConfig(instanceId)
      .catch((e) => console.error("Failed to extract remote profiles:", e));
  }, [isRemote, isConnected, instanceId]);

  const refreshProfiles = () => {
    if (isRemote) {
      if (!isConnected) return;
      api.remoteListModelProfiles(instanceId).then(setProfiles).catch((e) => console.error("Failed to load remote profiles:", e));
      api.remoteResolveApiKeys(instanceId).then(setApiKeys).catch((e) => console.error("Failed to resolve remote API keys:", e));
    } else {
      api.listModelProfiles().then(setProfiles).catch((e) => console.error("Failed to load profiles:", e));
      api.resolveApiKeys().then(setApiKeys).catch((e) => console.error("Failed to resolve API keys:", e));
    }
  };

  useEffect(refreshProfiles, [isRemote, isConnected, instanceId]);

  // Load catalog from cache instantly (no CLI calls)
  useEffect(() => {
    setCatalogRefreshed(false);
    if (isRemote) {
      // For remote, load catalog on mount (no local cache)
      if (isConnected) {
        api.remoteRefreshModelCatalog(instanceId).then(setCatalog).catch((e) => console.error("Failed to load remote model catalog:", e));
      }
    } else {
      api.getCachedModelCatalog().then(setCatalog).catch((e) => console.error("Failed to load model catalog:", e));
    }
  }, [isRemote, isConnected, instanceId]);

  // Refresh catalog from CLI when user focuses provider/model input
  const ensureCatalog = () => {
    if (catalogRefreshed) return;
    setCatalogRefreshed(true);
    if (isRemote) {
      if (!isConnected) return;
      api.remoteRefreshModelCatalog(instanceId).then((fresh) => {
        if (fresh.length > 0) setCatalog(fresh);
      }).catch((e) => console.error("Failed to refresh remote model catalog:", e));
    } else {
      api.refreshModelCatalog().then((fresh) => {
        if (fresh.length > 0) setCatalog(fresh);
      }).catch((e) => console.error("Failed to refresh model catalog:", e));
    }
  };

  const maskedKeyMap = useMemo(() => {
    const map = new Map<string, string>();
    for (const entry of apiKeys) {
      map.set(entry.profileId, entry.maskedKey);
    }
    return map;
  }, [apiKeys]);

  // Check for existing auth when provider changes
  useEffect(() => {
    if (form.id || !form.provider.trim()) {
      setAuthSuggestion(null);
      return;
    }
    if (isRemote) {
      // For remote: check if any existing profile with the same provider already has a key
      const sameProviderProfile = (profiles || []).find(
        (p) => p.provider === form.provider && maskedKeyMap.has(p.id) && maskedKeyMap.get(p.id) !== "..."
      );
      if (sameProviderProfile) {
        setAuthSuggestion({
          hasKey: true,
          source: `existing profile (${sameProviderProfile.provider}/${sameProviderProfile.model})`,
          authRef: sameProviderProfile.authRef || "",
        });
      } else {
        setAuthSuggestion(null);
      }
    } else {
      api.resolveProviderAuth(form.provider)
        .then(setAuthSuggestion)
        .catch((e) => { console.error("Failed to resolve provider auth:", e); setAuthSuggestion(null); });
    }
  }, [form.provider, form.id, isRemote, profiles, maskedKeyMap]);

  const modelCandidates = useMemo(() => {
    const found = catalog.find((c) => c.provider === form.provider);
    return found?.models || [];
  }, [catalog, form.provider]);

  const upsert = (event: FormEvent) => {
    event.preventDefault();
    if (!form.provider || !form.model) {
      setMessage("Provider and Model are required");
      return;
    }
    if (!isRemote && !form.apiKey && !form.id && !authSuggestion?.hasKey) {
      setMessage("API Key is required");
      return;
    }
    const profileData: ModelProfile = {
      id: form.id || "",
      name: `${form.provider}/${form.model}`,
      provider: form.provider,
      model: form.model,
      authRef: (!form.apiKey && authSuggestion?.authRef) ? authSuggestion.authRef : "",
      apiKey: form.apiKey || undefined,
      baseUrl: form.useCustomUrl && form.baseUrl ? form.baseUrl : undefined,
      enabled: form.enabled,
    };
    const upsertPromise = isRemote
      ? api.remoteUpsertModelProfile(instanceId, profileData)
      : api.upsertModelProfile(profileData);
    upsertPromise
      .then(() => {
        setMessage("Profile saved");
        setForm(emptyForm());
        refreshProfiles();
        onDataChange?.();
      })
      .catch((e) => setMessage(`Save failed: ${e}`));
  };

  const editProfile = (profile: ModelProfile) => {
    setForm({
      id: profile.id,
      provider: profile.provider,
      model: profile.model,
      apiKey: "",
      useCustomUrl: !!profile.baseUrl,
      baseUrl: profile.baseUrl || "",
      enabled: profile.enabled,
    });
  };

  const deleteProfile = (id: string) => {
    const deletePromise = isRemote
      ? api.remoteDeleteModelProfile(instanceId, id)
      : api.deleteModelProfile(id);
    deletePromise
      .then(() => {
        setMessage("Profile deleted");
        if (form.id === id) {
          setForm(emptyForm());
        }
        refreshProfiles();
        onDataChange?.();
      })
      .catch((e) => setMessage(`Delete failed: ${e}`));
  };

  return (
    <section>
      <h2 className="text-2xl font-bold mb-4">Settings</h2>

      {/* ---- Model Profiles ---- */}
      {!isRemote && (
        <p className="text-sm text-muted-foreground mb-4">
          For OAuth-based providers (GitHub Copilot, etc.), use the CLI:
          <code className="mx-1 px-1.5 py-0.5 bg-muted rounded text-xs">openclaw models auth login</code>
          or
          <code className="mx-1 px-1.5 py-0.5 bg-muted rounded text-xs">openclaw models auth login-github-copilot</code>.
          Profiles created via CLI will appear in the list on the right.
        </p>
      )}

          <div className="grid grid-cols-2 gap-3 items-start">
            <div className="space-y-3">
            {/* Create / Edit form */}
            <Card>
              <CardHeader>
                <CardTitle>{form.id ? "Edit Profile" : "Add Profile"}</CardTitle>
              </CardHeader>
              <CardContent>
                <form onSubmit={upsert} className="space-y-4">
                  <div className="space-y-1.5">
                    <Label>Provider</Label>
                    <AutocompleteField
                      value={form.provider}
                      onChange={(val) =>
                        setForm((p) => ({ ...p, provider: val, model: "" }))
                      }
                      onFocus={ensureCatalog}
                      options={catalog.map((c) => ({
                        value: c.provider,
                        label: c.provider,
                      }))}
                      placeholder="e.g. openai"
                    />
                  </div>

                  <div className="space-y-1.5">
                    <Label>Model</Label>
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
                    <Label>API Key</Label>
                    <Input
                      type="password"
                      placeholder={form.id ? "(unchanged if empty)" : authSuggestion?.hasKey ? "(optional — key already available)" : "sk-..."}
                      value={form.apiKey}
                      onChange={(e) =>
                        setForm((p) => ({ ...p, apiKey: e.target.value }))
                      }
                    />
                    {!form.id && authSuggestion?.hasKey && (
                      <p className="text-xs text-muted-foreground">
                        Key available via {authSuggestion.source}. Leave empty to reuse it.
                      </p>
                    )}
                  </div>

                  <div className="flex items-center gap-2">
                    <Checkbox
                      id="custom-url"
                      checked={form.useCustomUrl}
                      onCheckedChange={(checked) =>
                        setForm((p) => ({ ...p, useCustomUrl: checked === true }))
                      }
                    />
                    <Label htmlFor="custom-url">Custom Base URL</Label>
                  </div>

                  {form.useCustomUrl && (
                    <div className="space-y-1.5">
                      <Label>Base URL</Label>
                      <Input
                        placeholder="e.g. https://api.openai.com/v1"
                        value={form.baseUrl}
                        onChange={(e) =>
                          setForm((p) => ({ ...p, baseUrl: e.target.value }))
                        }
                      />
                    </div>
                  )}

                  <div className="flex gap-2 mt-2">
                    <Button type="submit">Save</Button>
                    {form.id && (
                      <>
                        <AlertDialog>
                          <AlertDialogTrigger asChild>
                            <Button type="button" variant="destructive">
                              Delete
                            </Button>
                          </AlertDialogTrigger>
                          <AlertDialogContent>
                            <AlertDialogHeader>
                              <AlertDialogTitle>Delete profile?</AlertDialogTitle>
                              <AlertDialogDescription>
                                This will permanently delete the profile "{form.provider}/{form.model}". This action cannot be undone.
                              </AlertDialogDescription>
                            </AlertDialogHeader>
                            <AlertDialogFooter>
                              <AlertDialogCancel>Cancel</AlertDialogCancel>
                              <AlertDialogAction
                                className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                                onClick={() => deleteProfile(form.id)}
                              >
                                Delete
                              </AlertDialogAction>
                            </AlertDialogFooter>
                          </AlertDialogContent>
                        </AlertDialog>
                        <Button
                          type="button"
                          variant="outline"
                          onClick={() => setForm(emptyForm())}
                        >
                          Cancel
                        </Button>
                      </>
                    )}
                  </div>
                </form>
              </CardContent>
            </Card>

            {/* Current Version */}
            <Card>
              <CardHeader>
                <CardTitle>Current Version</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="flex items-center gap-3 flex-wrap">
                  <span className="text-sm font-medium">{appVersion ? `v${appVersion}` : "..."}</span>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleCheckForUpdates}
                    disabled={appUpdateChecking || appUpdating}
                  >
                    {appUpdateChecking ? "Checking..." : "Check for Updates"}
                  </Button>
                </div>
                {!appUpdateChecking && appUpdate && !appUpdating && (
                  <div className="flex items-center gap-2 mt-3">
                    <Badge variant="outline" className="text-primary border-primary">
                      v{appUpdate.version} available
                    </Badge>
                    <Button size="sm" onClick={handleAppUpdate}>
                      Update &amp; Restart
                    </Button>
                  </div>
                )}
                {appUpdating && (
                  <div className="flex items-center gap-2 mt-3">
                    <Badge variant="outline" className="text-muted-foreground">
                      {appUpdateProgress !== null && appUpdateProgress < 100
                        ? `Downloading... ${appUpdateProgress}%`
                        : appUpdateProgress === 100
                          ? "Installing..."
                          : "Preparing..."}
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
              </CardContent>
            </Card>
            </div>

            {/* Profiles list */}
            <Card>
              <CardHeader>
                <CardTitle>Model Profiles</CardTitle>
              </CardHeader>
              <CardContent>
                {profiles === null ? (
                  <p className="text-muted-foreground">Loading profiles...</p>
                ) : profiles.length === 0 ? (
                  <p className="text-muted-foreground">No model profiles yet.</p>
                ) : null}
                <div className="grid gap-2">
                  {(profiles || []).map((profile) => (
                    <div
                      key={profile.id}
                      className="border border-border p-2.5 rounded-lg"
                    >
                      <div className="flex justify-between items-center">
                        <strong>{profile.provider}/{profile.model}</strong>
                        {profile.enabled ? (
                          <Badge className="bg-blue-100 text-blue-700 border-0">
                            enabled
                          </Badge>
                        ) : (
                          <Badge className="bg-red-100 text-red-700 border-0">
                            disabled
                          </Badge>
                        )}
                      </div>
                      <div className="text-sm text-muted-foreground mt-1">
                        API Key: {maskedKeyMap.get(profile.id) || "..."}
                      </div>
                      {profile.baseUrl && (
                        <div className="text-sm text-muted-foreground mt-0.5">
                          URL: {profile.baseUrl}
                        </div>
                      )}
                      <div className="flex gap-1.5 mt-1.5">
                        <Button
                          size="sm"
                          variant="outline"
                          type="button"
                          onClick={() => editProfile(profile)}
                        >
                          Edit
                        </Button>
                        <AlertDialog>
                          <AlertDialogTrigger asChild>
                            <Button size="sm" variant="destructive" type="button">
                              Delete
                            </Button>
                          </AlertDialogTrigger>
                          <AlertDialogContent>
                            <AlertDialogHeader>
                              <AlertDialogTitle>Delete profile?</AlertDialogTitle>
                              <AlertDialogDescription>
                                This will permanently delete the profile "{profile.provider}/{profile.model}". This action cannot be undone.
                              </AlertDialogDescription>
                            </AlertDialogHeader>
                            <AlertDialogFooter>
                              <AlertDialogCancel>Cancel</AlertDialogCancel>
                              <AlertDialogAction
                                className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                                onClick={() => deleteProfile(profile.id)}
                              >
                                Delete
                              </AlertDialogAction>
                            </AlertDialogFooter>
                          </AlertDialogContent>
                        </AlertDialog>
                      </div>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          </div>

      {message && (
        <p className="text-sm text-muted-foreground mt-3">{message}</p>
      )}

    </section>
  );
}
