import { listen } from "@tauri-apps/api/event";
import { formatBytes } from "./utils";
import type {
  BackupDoneEvent,
  BackupErrorEvent,
  BackupInfo,
  BackupProgressEvent,
} from "./types";

export async function runBackupStream({
  start,
  onProgress,
}: {
  start: () => Promise<string>;
  onProgress?: (event: BackupProgressEvent) => void;
}): Promise<BackupInfo> {
  let handleId: string | null = null;
  const cleanup: Array<() => void> = [];
  let resolveResult: (info: BackupInfo) => void = () => {};
  let rejectResult: (error: unknown) => void = () => {};

  const dispose = () => {
    while (cleanup.length > 0) {
      const fn = cleanup.pop();
      fn?.();
    }
  };

  try {
    const result = new Promise<BackupInfo>((resolve, reject) => {
      resolveResult = resolve;
      rejectResult = reject;
    });

    cleanup.push(
      await listen<BackupProgressEvent>("backup:progress", (event) => {
        if (!handleId || event.payload.handleId !== handleId) return;
        onProgress?.(event.payload);
      }),
    );

    cleanup.push(
      await listen<BackupDoneEvent>("backup:done", (event) => {
        if (!handleId || event.payload.handleId !== handleId) return;
        dispose();
        resolveResult(event.payload.info);
      }),
    );

    cleanup.push(
      await listen<BackupErrorEvent>("backup:error", (event) => {
        if (!handleId || event.payload.handleId !== handleId) return;
        dispose();
        rejectResult(new Error(event.payload.error || "Backup failed"));
      }),
    );

    try {
      handleId = await start();
    } catch (error) {
      dispose();
      rejectResult(error);
    }

    const info = await result;

    return info;
  } catch (error) {
    dispose();
    throw error;
  }
}

export function formatBackupProgressLabel(event: BackupProgressEvent, fallback: string): string {
  const phaseLabel =
    event.phase === "config"
      ? "Config"
      : event.phase === "agents"
        ? "Agents"
        : event.phase === "memory"
          ? "Memory"
          : event.phase === "done"
            ? "Done"
            : "Snapshot";

  const details = [
    event.filesCopied > 0 ? `${event.filesCopied} files` : null,
    event.bytesCopied > 0 ? formatBytes(event.bytesCopied) : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return details ? `${fallback} ${phaseLabel} · ${details}` : `${fallback} ${phaseLabel}`;
}
