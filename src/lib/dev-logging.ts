/** Log an exception detail in development mode only. */
export function logDevException(label: string, detail: unknown): void {
  if (!import.meta.env.DEV) return;
  console.error(`[dev exception] ${label}`, detail);
}

/** Log an ignored error context in development mode only. */
export function logDevIgnoredError(context: string, detail: unknown): void {
  if (!import.meta.env.DEV) return;
  console.warn(`[dev ignored error] ${context}`, detail);
}
