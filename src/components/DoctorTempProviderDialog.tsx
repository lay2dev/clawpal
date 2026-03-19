import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { AutocompleteField } from "./AutocompleteField";
import {
  emptyForm, normalizeOauthProvider, providerUsesOAuthAuth,
  defaultOauthAuthRef, isEnvVarLikeAuthRef, defaultEnvAuthRef,
  inferCredentialSource, providerSupportsOptionalApiKey,
  type ProfileForm, type CredentialSource,
} from "../lib/profile-utils";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useApi } from "@/lib/use-api";
import type { ModelCatalogProvider, ModelProfile, ProviderAuthSuggestion } from "@/lib/types";


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

interface DoctorTempProviderDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initialProfileId?: string | null;
  onSaved?: (profile: ModelProfile) => void;
}

export function DoctorTempProviderDialog({
  open,
  onOpenChange,
  initialProfileId,
  onSaved,
}: DoctorTempProviderDialogProps) {
  const { t } = useTranslation();
  const ua = useApi();
  const [form, setForm] = useState<ProfileForm>(emptyForm());
  const [profiles, setProfiles] = useState<ModelProfile[]>([]);
  const [catalog, setCatalog] = useState<ModelCatalogProvider[]>([]);
  const [credentialSource, setCredentialSource] = useState<CredentialSource>("manual");
  const [authSuggestion, setAuthSuggestion] = useState<ProviderAuthSuggestion | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const refreshSeedData = useCallback(async () => {
    const [nextProfiles, nextCatalog] = await Promise.all([
      ua.listModelProfiles(),
      ua.refreshModelCatalog().catch(() => [] as ModelCatalogProvider[]),
    ]);
    setProfiles(nextProfiles);
    setCatalog(nextCatalog);
    if (initialProfileId) {
      const existing = nextProfiles.find((profile) => profile.id === initialProfileId);
      if (existing) {
        setForm({
          id: existing.id,
          provider: existing.provider,
          model: existing.model,
          authRef: existing.authRef || "",
          apiKey: "",
          useCustomUrl: !!existing.baseUrl,
          baseUrl: existing.baseUrl || "",
          enabled: existing.enabled,
        });
        setCredentialSource(inferCredentialSource(existing.provider, existing.authRef || ""));
        return;
      }
    }
    setForm(emptyForm());
    setCredentialSource("manual");
  }, [initialProfileId, ua]);

  useEffect(() => {
    if (!open) return;
    void refreshSeedData();
  }, [open, refreshSeedData]);

  useEffect(() => {
    if (!open) return;
    if (!form.provider.trim()) {
      setAuthSuggestion(null);
      return;
    }
    ua.resolveProviderAuth(form.provider)
      .then(setAuthSuggestion)
      .catch(() => setAuthSuggestion(null));
  }, [form.provider, open, ua]);

  const modelCandidates = useMemo(() => {
    return catalog.find((item) => item.provider === form.provider)?.models || [];
  }, [catalog, form.provider]);

  const providerCandidates = useMemo(() => {
    const values = new Set<string>();
    for (const provider of PROVIDER_FALLBACK_OPTIONS) {
      if (provider.trim()) values.add(provider);
    }
    for (const item of catalog) {
      if (item.provider.trim()) values.add(item.provider.trim());
    }
    for (const profile of profiles) {
      if (profile.provider.trim()) values.add(profile.provider.trim());
    }
    return Array.from(values).sort((a, b) => a.localeCompare(b));
  }, [catalog, profiles]);

  const handleSave = async (event: FormEvent) => {
    event.preventDefault();
    if (!form.provider.trim() || !form.model.trim()) {
      setMessage(t("settings.providerModelRequired"));
      return;
    }
    if (
      credentialSource === "manual"
      && !providerSupportsOptionalApiKey(form.provider)
      && !form.apiKey.trim()
      && !form.id
    ) {
      setMessage(t("settings.apiKeyRequired"));
      return;
    }

    const explicitAuthRef = form.authRef.trim();
    const authRef = credentialSource === "oauth"
      ? (explicitAuthRef || defaultOauthAuthRef(form.provider))
      : credentialSource === "env"
        ? (explicitAuthRef || defaultEnvAuthRef(form.provider))
        : "";

    const payload: ModelProfile = {
      id: form.id,
      name: `${form.provider}/${form.model}`,
      provider: form.provider.trim(),
      model: form.model.trim(),
      authRef,
      apiKey: form.apiKey.trim() || undefined,
      baseUrl: form.useCustomUrl && form.baseUrl.trim() ? form.baseUrl.trim() : undefined,
      enabled: true,
    };

    setSaving(true);
    setMessage(null);
    try {
      const saved = await ua.upsertModelProfile(payload);
      onSaved?.(saved);
      onOpenChange(false);
      setForm(emptyForm());
      setCredentialSource("manual");
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        onOpenChange(nextOpen);
        if (!nextOpen) {
          setMessage(null);
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {t("doctor.configureTempProvider", {
              defaultValue: "Configure temporary gateway provider",
            })}
          </DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSave} className="space-y-4">
          <div className="rounded-md border border-border/60 bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
            {t("doctor.tempProviderHint", {
              defaultValue:
                "This profile is used only to give the temporary repair gateway inference. Prefer a provider with a static API key.",
            })}
          </div>

          <div className="space-y-1.5">
            <Label>{t("settings.provider")}</Label>
            <AutocompleteField
              value={form.provider}
              onChange={(value) => {
                const nextSource: CredentialSource = providerUsesOAuthAuth(value)
                  ? (credentialSource === "manual" ? "manual" : "oauth")
                  : (credentialSource === "oauth" ? "env" : credentialSource);
                setCredentialSource(nextSource);
                setForm((current) => ({
                  ...current,
                  provider: value,
                  model: "",
                  authRef: current.id
                    ? current.authRef
                    : providerUsesOAuthAuth(value)
                      ? defaultOauthAuthRef(value)
                      : (nextSource === "env" ? (current.authRef || defaultEnvAuthRef(value)) : ""),
                }));
              }}
              onFocus={() => {
                if (catalog.length === 0) {
                  void ua.refreshModelCatalog().then(setCatalog).catch(() => undefined);
                }
              }}
              options={providerCandidates.map((provider) => ({ value: provider, label: provider }))}
              placeholder="e.g. openai"
            />
          </div>

          <div className="space-y-1.5">
            <Label>{t("settings.model")}</Label>
            <AutocompleteField
              value={form.model}
              onChange={(value) => setForm((current) => ({ ...current, model: value }))}
              onFocus={() => {
                if (catalog.length === 0) {
                  void ua.refreshModelCatalog().then(setCatalog).catch(() => undefined);
                }
              }}
              options={modelCandidates.map((model) => ({ value: model.id, label: model.name || model.id }))}
              placeholder="e.g. gpt-4o"
            />
          </div>

          <div className="space-y-1.5">
            <Label>{t("settings.credentialSource")}</Label>
            <Select
              value={credentialSource}
              onValueChange={(value) => {
                const next = value as CredentialSource;
                if (next === "oauth" && !providerUsesOAuthAuth(form.provider)) return;
                setCredentialSource(next);
                setForm((current) => {
                  if (next === "oauth") {
                    const oauthRef = current.authRef.trim();
                    return {
                      ...current,
                      apiKey: "",
                      authRef: oauthRef && !isEnvVarLikeAuthRef(oauthRef)
                        ? oauthRef
                        : defaultOauthAuthRef(current.provider),
                    };
                  }
                  if (next === "env") {
                    return {
                      ...current,
                      authRef: current.authRef.trim() || defaultEnvAuthRef(current.provider),
                    };
                  }
                  return current;
                });
              }}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {providerUsesOAuthAuth(form.provider) ? (
                  <SelectItem value="oauth">{t("settings.credentialSourceOauth")}</SelectItem>
                ) : null}
                <SelectItem value="env">{t("settings.credentialSourceEnv")}</SelectItem>
                <SelectItem value="manual">{t("settings.credentialSourceManual")}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {credentialSource === "env" ? (
            <div className="space-y-1.5">
              <Label>{t("settings.authRef")}</Label>
              <Input
                placeholder={defaultEnvAuthRef(form.provider) || "OPENAI_API_KEY"}
                value={form.authRef}
                onChange={(event) => setForm((current) => ({ ...current, authRef: event.target.value }))}
              />
            </div>
          ) : null}

          {credentialSource === "manual" ? (
            <div className="space-y-1.5">
              <Label>{t("settings.apiKey")}</Label>
              <Input
                type="password"
                placeholder={authSuggestion?.hasKey
                  ? t("settings.apiKeyOptional")
                  : t("settings.apiKeyPlaceholder")}
                value={form.apiKey}
                onChange={(event) => setForm((current) => ({ ...current, apiKey: event.target.value }))}
              />
            </div>
          ) : null}

          <div className="flex items-center gap-2">
            <Checkbox
              id="doctor-temp-custom-url"
              checked={form.useCustomUrl}
              onCheckedChange={(checked) => {
                setForm((current) => ({ ...current, useCustomUrl: checked === true }));
              }}
            />
            <Label htmlFor="doctor-temp-custom-url">{t("settings.customBaseUrl")}</Label>
          </div>

          {form.useCustomUrl ? (
            <div className="space-y-1.5">
              <Label>{t("settings.baseUrl")}</Label>
              <Input
                placeholder="e.g. https://api.openai.com/v1"
                value={form.baseUrl}
                onChange={(event) => setForm((current) => ({ ...current, baseUrl: event.target.value }))}
              />
            </div>
          ) : null}

          {message ? <div className="text-sm text-destructive">{message}</div> : null}

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              {t("settings.cancel")}
            </Button>
            <Button type="submit" disabled={saving}>
              {saving
                ? t("doctor.repairing", { defaultValue: "Repairing..." })
                : t("settings.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
