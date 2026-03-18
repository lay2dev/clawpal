import { useCallback, useEffect, useState } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";

export function useAppUpdate(hasAppUpdate?: boolean, onAppUpdateSeen?: () => void) {
  const [appVersion, setAppVersion] = useState("");
  const [appUpdate, setAppUpdate] = useState<{ version: string; body?: string } | null>(null);
  const [appUpdateChecking, setAppUpdateChecking] = useState(false);
  const [appUpdating, setAppUpdating] = useState(false);
  const [appUpdateProgress, setAppUpdateProgress] = useState<number | null>(null);

  useEffect(() => { getVersion().then(setAppVersion).catch(() => {}); }, []);

  const handleCheckForUpdates = useCallback(async () => {
    setAppUpdateChecking(true);
    setAppUpdate(null);
    try {
      const update = await check();
      if (update) setAppUpdate({ version: update.version, body: update.body });
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
        if (event.event === "Started" && event.data.contentLength) totalBytes = event.data.contentLength;
        else if (event.event === "Progress") {
          downloadedBytes += event.data.chunkLength;
          if (totalBytes > 0) setAppUpdateProgress(Math.round((downloadedBytes / totalBytes) * 100));
        } else if (event.event === "Finished") setAppUpdateProgress(100);
      });
      await relaunch();
    } catch (e) {
      console.error("App update failed:", e);
      setAppUpdating(false);
      setAppUpdateProgress(null);
    }
  }, []);

  useEffect(() => {
    if (hasAppUpdate) { handleCheckForUpdates(); onAppUpdateSeen?.(); }
  }, [hasAppUpdate, handleCheckForUpdates, onAppUpdateSeen]);

  return { appVersion, appUpdate, appUpdateChecking, appUpdating, appUpdateProgress, handleCheckForUpdates, handleAppUpdate };
}
