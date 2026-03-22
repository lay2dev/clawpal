/**
 * Doctor diagnostic type definitions.
 * Extracted from types.ts for readability.
 */

export interface DoctorIssue {
  id: string;
  code: string;
  severity: "error" | "warn" | "info";
  message: string;
  autoFixable: boolean;
  fixHint?: string;
}

export interface DoctorReport {
  ok: boolean;
  score: number;
  issues: DoctorIssue[];
}

export interface PendingCommand {
  id: string;
  label: string;
  command: string[];
  createdAt: string;
}

export interface PreviewQueueResult {
  commands: PendingCommand[];
  configBefore: string;
  configAfter: string;
  warnings: string[];
  errors: string[];
}

export interface PreviewQueueResult {
  commands: PendingCommand[];
  configBefore: string;
  configAfter: string;
  warnings: string[];
  errors: string[];
}

export interface DoctorInvoke {
  id: string;
  command: string;
  args: Record<string, unknown>;
  type: "read" | "write";
}

export interface DiagnosisCitation {
  url: string;
  section?: string;
}

export interface DiagnosisReportItem {
  problem: string;
  severity: "error" | "warn" | "info";
  fix_options: string[];
  root_cause_hypothesis?: string;
  fix_steps?: string[];
  confidence?: number;
  citations?: DiagnosisCitation[];
  version_awareness?: string;
  action?: { tool: string; args: string; instance?: string; reason?: string };
}

export interface DoctorChatMessage {
  id: string;
  role: "assistant" | "user" | "tool-call" | "tool-result";
  content: string;
  invoke?: DoctorInvoke;
  invokeResult?: unknown;
  invokeId?: string;
  status?: "pending" | "approved" | "rejected" | "auto";
  diagnosisReport?: { items: DiagnosisReportItem[] };
  /** Epoch milliseconds when the message was created. */
  timestamp?: number;
}

export interface ApplyQueueResult {
  ok: boolean;
  appliedCount: number;
  totalCount: number;
  error: string | null;
  rolledBack: boolean;
}
export interface ApplyQueueResult {
  ok: boolean;
  appliedCount: number;
  totalCount: number;
  error: string | null;
  rolledBack: boolean;
}
