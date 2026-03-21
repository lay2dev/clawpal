/**
 * Cron job and watchdog type definitions.
 * Extracted from types.ts for readability.
 */

export interface CronConfigSnapshot {
  jobs: CronJob[];
}

export interface CronRuntimeSnapshot {
  jobs: CronJob[];
  watchdog: WatchdogStatus & { alive: boolean; deployed: boolean };
}

export type WatchdogJobStatus = "ok" | "pending" | "triggered" | "retrying" | "escalated";

export interface CronSchedule {
  kind: "cron" | "every" | "at";
  expr?: string;
  tz?: string;
  everyMs?: number;
  at?: string;
}

export interface CronJobState {
  lastRunAtMs?: number;
  lastStatus?: string;
  lastError?: string;
}

export interface CronJobDelivery {
  mode?: string;
  channel?: string;
  to?: string;
}

export interface CronJob {
  jobId: string;
  name: string;
  schedule: CronSchedule;
  sessionTarget: "main" | "isolated";
  agentId?: string;
  enabled: boolean;
  description?: string;
  state?: CronJobState;
  delivery?: CronJobDelivery;
}

export interface CronRun {
  jobId: string;
  startedAt: string;
  endedAt?: string;
  outcome: string;
  error?: string;
  ts?: number;
  runAtMs?: number;
  durationMs?: number;
  summary?: string;
}

export interface WatchdogJobState {
  status: WatchdogJobStatus;
  lastScheduledAt?: string;
  lastRunAt?: string | null;
  retries: number;
  lastError?: string;
  escalatedAt?: string;
}

export interface WatchdogStatus {
  pid: number;
  startedAt: string;
  lastCheckAt: string;
  gatewayHealthy: boolean;
  jobs: Record<string, WatchdogJobState>;
}
