/**
 * Cron page utility functions: schedule formatting, relative time, job filtering.
 * Extracted from Cron.tsx for readability.
 */
import type { TFunction } from "i18next";
import type { CronJob, CronSchedule } from "./types";

const DOW_EN = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const DOW_ZH = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];
const WATCHDOG_LATE_GRACE_MS = 5 * 60 * 1000;

export type CronFilter = "all" | "ok" | "retrying" | "escalated" | "disabled";

export function watchdogJobLikelyLate(job: { lastScheduledAt?: string; lastRunAt?: string | null } | undefined): boolean {
  if (!job?.lastScheduledAt) return false;
  const scheduledAt = Date.parse(job.lastScheduledAt);
  if (!Number.isFinite(scheduledAt)) return false;
  const runAt = job.lastRunAt ? Date.parse(job.lastRunAt) : Number.NaN;
  return (!Number.isFinite(runAt) || runAt + 1000 < scheduledAt) && Date.now() - scheduledAt > WATCHDOG_LATE_GRACE_MS;
}

export function computeJobFilter(job: CronJob, wdJob: { status?: string; lastScheduledAt?: string; lastRunAt?: string | null } | undefined): CronFilter {
  if (job.enabled === false) return "disabled";
  if (watchdogJobLikelyLate(wdJob)) return "escalated";
  const wdStatus = wdJob?.status;
  if (wdStatus === "retrying" || wdStatus === "pending") return "retrying";
  if (job.state?.lastStatus === "error") return "retrying";
  return "ok";
}

export function cronToHuman(expr: string, t: TFunction, lang: string): string {
  const parts = expr.trim().split(/\s+/);
  if (parts.length !== 5) return expr;
  const [min, hour, dom, mon, dow] = parts;
  const time = `${hour.padStart(2, "0")}:${min.padStart(2, "0")}`;
  const dowNames = lang.startsWith("zh") ? DOW_ZH : DOW_EN;
  if (min.startsWith("*/") && hour === "*" && dom === "*" && mon === "*" && dow === "*") return t("cron.every", { interval: `${min.slice(2)}m` });
  if (min === "0" && hour.startsWith("*/") && dom === "*" && mon === "*" && dow === "*") return t("cron.every", { interval: `${hour.slice(2)}h` });
  if (dom === "*" && mon === "*" && dow !== "*" && !hour.includes("/") && !min.includes("/")) {
    const days = dow.split(",").map(d => dowNames[parseInt(d)] || d).join(", ");
    return `${days} ${time}`;
  }
  if (dom !== "*" && !dom.includes("/") && mon === "*" && dow === "*" && !hour.includes("/") && !min.includes("/")) return t("cron.monthly", { day: dom, time });
  if (dom === "*" && mon === "*" && dow === "*" && !hour.includes("/") && !min.includes("/")) {
    const hours = hour.split(",");
    if (hours.length === 1) return t("cron.daily", { time });
    return t("cron.daily", { time: hours.map(h => `${h.padStart(2, "0")}:${min.padStart(2, "0")}`).join(", ") });
  }
  return expr;
}

export function formatSchedule(s: CronSchedule | undefined, t: TFunction, lang: string): string {
  if (!s) return "—";
  if (s.kind === "every" && s.everyMs) {
    const mins = Math.round(s.everyMs / 60000);
    return mins >= 60 ? t("cron.every", { interval: `${Math.round(mins / 60)}h` }) : t("cron.every", { interval: `${mins}m` });
  }
  if (s.kind === "at" && s.at) return fmtDate(new Date(s.at).getTime());
  if (s.kind === "cron" && s.expr) return cronToHuman(s.expr, t, lang);
  return "—";
}

export function fmtDate(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

export function fmtRelative(ms: number, t: TFunction): string {
  const diff = Date.now() - ms;
  const secs = Math.floor(diff / 1000);
  if (secs < 0) return t("cron.justNow");
  if (secs < 60) return t("cron.secsAgo", { count: secs });
  const mins = Math.floor(secs / 60);
  if (mins < 60) return t("cron.minsAgo", { count: mins });
  const hours = Math.floor(mins / 60);
  if (hours < 24) return t("cron.hoursAgo", { count: hours });
  return t("cron.daysAgo", { count: Math.floor(hours / 24) });
}

export function fmtDur(ms: number, t: TFunction): string {
  if (ms < 1000) return `${ms}ms`;
  const s = Math.round(ms / 1000);
  return s < 60 ? t("cron.durSecs", { count: s }) : t("cron.durMins", { m: Math.floor(s / 60), s: s % 60 });
}
