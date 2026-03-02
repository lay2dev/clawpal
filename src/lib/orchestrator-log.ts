export type OrchestratorEventLevel = "info" | "success" | "error";

export interface OrchestratorEvent {
  id: string;
  at: string;
  level: OrchestratorEventLevel;
  message: string;
  instanceId: string;
  sessionId?: string;
  goal?: string;
  source?: string;
  step?: string;
  state?: string;
  details?: string;
}

const STORAGE_KEY = "clawpal_orchestrator_events_v1";
const MAX_EVENTS = 300;

function canUseStorage(): boolean {
  return typeof window !== "undefined" && typeof window.localStorage !== "undefined";
}

export function readOrchestratorEvents(): OrchestratorEvent[] {
  if (!canUseStorage()) return [];
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as OrchestratorEvent[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function writeOrchestratorEvents(events: OrchestratorEvent[]): void {
  if (!canUseStorage()) return;
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(events));
}

export function appendOrchestratorEvent(
  event: Omit<OrchestratorEvent, "id" | "at"> & Partial<Pick<OrchestratorEvent, "id" | "at">>,
): OrchestratorEvent {
  const next: OrchestratorEvent = {
    id: event.id || crypto.randomUUID(),
    at: event.at || new Date().toISOString(),
    level: event.level,
    message: event.message,
    instanceId: event.instanceId,
    sessionId: event.sessionId,
    goal: event.goal,
    source: event.source,
    step: event.step,
    state: event.state,
    details: event.details,
  };
  const all = readOrchestratorEvents();
  all.push(next);
  if (all.length > MAX_EVENTS) {
    all.splice(0, all.length - MAX_EVENTS);
  }
  writeOrchestratorEvents(all);
  return next;
}

export function clearOrchestratorEvents(instanceId?: string): void {
  if (!instanceId) {
    writeOrchestratorEvents([]);
    return;
  }
  const filtered = readOrchestratorEvents().filter((e) => e.instanceId !== instanceId);
  writeOrchestratorEvents(filtered);
}
