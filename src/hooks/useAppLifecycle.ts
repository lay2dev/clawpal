import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { check } from "@tauri-apps/plugin-updater";
import { getVersion } from "@tauri-apps/api/app";
import { api } from "@/lib/api";
import { withGuidance } from "@/lib/guidance";
import {
  LEGACY_DOCKER_INSTANCES_KEY,
  normalizeDockerInstance,
} from "@/lib/docker-instance-helpers";
import { logDevIgnoredError } from "@/lib/dev-logging";
import { OPEN_TABS_STORAGE_KEY } from "@/lib/routes";
import type { DockerInstance, PrecheckIssue } from "@/lib/types";

const PING_URL = "https://api.clawpal.zhixian.io/ping";

const preloadRouteModules = () =>
  Promise.allSettled([
    import("@/pages/Home"),
    import("@/pages/Channels"),
    import("@/pages/Recipes"),
    import("@/pages/Cron"),
    import("@/pages/Doctor"),
    import("@/pages/OpenclawContext"),
    import("@/pages/History"),
    import("@/components/Chat"),
    import("@/components/PendingChangesBar"),
  ]);

interface UseAppLifecycleParams {
  showToast: (message: string, type?: "success" | "error") => void;
  refreshHosts: () => void;
  refreshRegisteredInstances: () => void;
}

export function useAppLifecycle(params: UseAppLifecycleParams) {
  const { t } = useTranslation();
  const { showToast, refreshHosts, refreshRegisteredInstances } = params;

  const [appUpdateAvailable, setAppUpdateAvailable] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const legacyMigrationDoneRef = useRef(false);

  // Preload route modules
  useEffect(() => {
    const timer = window.setTimeout(() => {
      void preloadRouteModules();
    }, 1200);
    return () => window.clearTimeout(timer);
  }, []);

  // Startup: check for updates + analytics ping
  useEffect(() => {
    let installId = localStorage.getItem("clawpal_install_id");
    if (!installId) {
      installId = crypto.randomUUID();
      localStorage.setItem("clawpal_install_id", installId);
    }

    check()
      .then((update) => { if (update) setAppUpdateAvailable(true); })
      .catch((error) => logDevIgnoredError("check", error));

    getVersion().then((version) => {
      setAppVersion(version);
      const url = PING_URL;
      if (!url) return;
      fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ v: version, id: installId, platform: navigator.platform }),
      }).catch((error) => logDevIgnoredError("analytics ping request", error));
    }).catch((error) => logDevIgnoredError("getVersion", error));
  }, []);

  // Startup precheck: validate registry
  useEffect(() => {
    withGuidance(
      () => api.precheckRegistry(),
      "precheckRegistry",
      "local",
      "local",
    ).then((issues) => {
      const errors = issues.filter((i: PrecheckIssue) => i.severity === "error");
      if (errors.length === 1) {
        showToast(errors[0].message, "error");
      } else if (errors.length > 1) {
        showToast(`${errors[0].message}${t("doctor.remainingIssues", { count: errors.length - 1 })}`, "error");
      }
    }).catch((error) => {
      logDevIgnoredError("precheckRegistry", error);
    });
  }, [showToast, t]);

  // Legacy instance migration
  const readLegacyDockerInstances = useCallback((): DockerInstance[] => {
    try {
      const raw = localStorage.getItem(LEGACY_DOCKER_INSTANCES_KEY);
      if (!raw) return [];
      const parsed = JSON.parse(raw) as DockerInstance[];
      if (!Array.isArray(parsed)) return [];
      const out: DockerInstance[] = [];
      const seen = new Set<string>();
      for (const item of parsed) {
        if (!item?.id || typeof item.id !== "string") continue;
        const id = item.id.trim();
        if (!id || seen.has(id)) continue;
        seen.add(id);
        out.push(normalizeDockerInstance({ ...item, id }));
      }
      return out;
    } catch {
      return [];
    }
  }, []);

  const readLegacyOpenTabs = useCallback((): string[] => {
    try {
      const raw = localStorage.getItem(OPEN_TABS_STORAGE_KEY);
      if (!raw) return [];
      const parsed = JSON.parse(raw);
      if (!Array.isArray(parsed)) return [];
      return parsed.filter((id): id is string => typeof id === "string" && id.trim().length > 0);
    } catch {
      return [];
    }
  }, []);

  useEffect(() => {
    if (legacyMigrationDoneRef.current) return;
    legacyMigrationDoneRef.current = true;
    const legacyDockerInstances = readLegacyDockerInstances();
    const legacyOpenTabIds = readLegacyOpenTabs();
    withGuidance(
      () => api.migrateLegacyInstances(legacyDockerInstances, legacyOpenTabIds),
      "migrateLegacyInstances",
      "local",
      "local",
    )
      .then((result) => {
        if (
          result.importedSshHosts > 0
          || result.importedDockerInstances > 0
          || result.importedOpenTabInstances > 0
        ) {
          refreshRegisteredInstances();
          refreshHosts();
          localStorage.removeItem(LEGACY_DOCKER_INSTANCES_KEY);
        }
      })
      .catch((e) => {
        console.error("Legacy instance migration failed:", e);
      });
  }, [readLegacyDockerInstances, readLegacyOpenTabs, refreshRegisteredInstances, refreshHosts]);

  return {
    appUpdateAvailable,
    setAppUpdateAvailable,
    appVersion,
  };
}
