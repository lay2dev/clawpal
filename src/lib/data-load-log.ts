import { invoke } from "@tauri-apps/api/core";

export type DataLoadMetricSource = "persisted" | "cache" | "config" | "runtime" | "cli";
export type DataLoadMetricPhase = "start" | "success" | "error";

export interface DataLoadMetricPayload {
  requestId: string;
  resource: string;
  page: string;
  instanceId: string;
  instanceToken: number | string | null;
  source: DataLoadMetricSource;
  phase: DataLoadMetricPhase;
  elapsedMs: number;
  cacheHit: boolean;
  errorSummary?: string;
}

let requestCounter = 0;
const SUPPRESSED_DATA_LOAD_LOG_RESOURCES = new Set([
  "listSessionFiles",
  "listBackups",
]);

export function createDataLoadRequestId(resource: string): string {
  requestCounter = (requestCounter + 1) % Number.MAX_SAFE_INTEGER;
  return `${resource}:${Date.now().toString(36)}:${requestCounter.toString(36)}`;
}

export function buildDataLoadLogLine(payload: DataLoadMetricPayload): string {
  return `[metrics][data_load] ${JSON.stringify(payload)}`;
}

export function shouldEmitDataLoadMetric(payload: DataLoadMetricPayload): boolean {
  return !SUPPRESSED_DATA_LOAD_LOG_RESOURCES.has(payload.resource);
}

export function emitDataLoadMetric(payload: DataLoadMetricPayload): void {
  if (!shouldEmitDataLoadMetric(payload)) return;
  void invoke("log_app_event", { message: buildDataLoadLogLine(payload) }).catch((error) => {
    if (import.meta.env.DEV) {
      console.warn("[dev ignored error] emitDataLoadMetric", error);
    }
  });
}

export function inferDataLoadPage(method: string): string {
  if (
    method.startsWith("getInstance")
    || method === "getStatusExtra"
    || method === "checkOpenclawUpdate"
  ) {
    return "home";
  }
  if (method.startsWith("getChannels")) {
    return "channels";
  }
  if (method.startsWith("getCron") || method.startsWith("listCron") || method === "getWatchdogStatus") {
    return "cron";
  }
  if (
    method === "diagnoseDoctorAssistant"
    || method === "repairDoctorAssistant"
    || method === "getRescueBotStatus"
    || method === "diagnosePrimaryViaRescue"
    || method === "repairPrimaryViaRescue"
  ) {
    return "doctor";
  }
  return "app";
}

export function inferDataLoadSource(method: string): DataLoadMetricSource {
  if (method.includes("ConfigSnapshot")) return "config";
  if (method.includes("RuntimeSnapshot")) return "runtime";
  return "cli";
}

export function parseInstanceToken(instanceCacheKey: string): number | string | null {
  const separatorIndex = instanceCacheKey.lastIndexOf("#");
  if (separatorIndex === -1) return null;
  const raw = instanceCacheKey.slice(separatorIndex + 1);
  if (!raw) return null;
  const asNumber = Number(raw);
  return Number.isFinite(asNumber) ? asNumber : raw;
}
