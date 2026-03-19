import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { InstanceStatus, StatusExtra, ModelProfile } from "../lib/types";

/** Emit agent guidance events for duplicate installs and post-install onboarding. */
export function useHomeGuidance({
  statusExtra,
  statusSettled,
  status,
  modelProfiles,
  instanceId,
  isRemote,
  isDocker,
}: {
  statusExtra: StatusExtra | null;
  statusSettled: boolean;
  status: InstanceStatus | null;
  modelProfiles: ModelProfile[];
  instanceId: string;
  isRemote: boolean;
  isDocker: boolean;
}) {
  const { t } = useTranslation();
  const duplicateInstallGuidanceSigRef = useRef<string>("");
  const onboardingGuidanceSigRef = useRef<string>("");

  // Duplicate install guidance
  useEffect(() => {
    const entries = statusExtra?.duplicateInstalls || [];
    if (entries.length === 0) return;
    const signature = `${instanceId}:${entries.join("|")}`;
    if (duplicateInstallGuidanceSigRef.current === signature) return;
    duplicateInstallGuidanceSigRef.current = signature;
    const transport = isRemote ? "remote_ssh" : (isDocker ? "docker_local" : "local");
    window.dispatchEvent(new CustomEvent("clawpal:agent-guidance", {
      detail: {
        message: t("home.duplicateInstalls"),
        summary: t("home.duplicateInstalls"),
        actions: [t("home.fixInDoctor"), "Run `which -a openclaw` and keep only one valid binary in PATH"],
        source: "status-extra",
        operation: "status.extra.duplicate_installs",
        instanceId,
        transport,
        rawError: `Duplicate openclaw installs detected: ${entries.join(" ; ")}`,
        createdAt: Date.now(),
      },
    }));
  }, [statusExtra?.duplicateInstalls, t, instanceId, isDocker, isRemote]);

  // Post-install onboarding guidance
  useEffect(() => {
    if (!statusSettled || !status) return;
    const needsSetup = !status.healthy || (!isRemote && (modelProfiles.length === 0 || !status.globalDefaultModel));
    if (!needsSetup) return;
    const issues: string[] = [];
    if (!status.healthy) issues.push("unhealthy");
    if (!isRemote && modelProfiles.length === 0) issues.push("no_profiles");
    if (!isRemote && !status.globalDefaultModel) issues.push("no_default_model");
    const signature = `${instanceId}:onboarding:${issues.join(",")}`;
    if (onboardingGuidanceSigRef.current === signature) return;
    onboardingGuidanceSigRef.current = signature;
    const transport = isRemote ? "remote_ssh" : (isDocker ? "docker_local" : "local");
    const actions: string[] = [];
    if (!status.healthy) actions.push(t("onboarding.actionCheckDoctor"));
    if (!isRemote && modelProfiles.length === 0) actions.push(t("onboarding.actionAddProfile"));
    if (!isRemote && !status.globalDefaultModel && modelProfiles.length > 0) actions.push(t("onboarding.actionSetDefault"));
    window.dispatchEvent(new CustomEvent("clawpal:agent-guidance", {
      detail: {
        message: t("onboarding.summary"),
        summary: t("onboarding.summary"),
        actions,
        source: "onboarding",
        operation: "post_install.onboarding",
        instanceId,
        transport,
        rawError: `Instance needs setup: ${issues.join(", ")}`,
        createdAt: Date.now(),
      },
    }));
  }, [statusSettled, status, modelProfiles, t, instanceId, isDocker, isRemote]);
}
