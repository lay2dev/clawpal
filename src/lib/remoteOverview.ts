export function mergeOverviewSnapshot<
  TConfig extends Record<string, unknown>,
  TRuntime extends Record<string, unknown>,
>(
  configSnapshot: TConfig | null,
  runtimeSnapshot: TRuntime | null,
): (TConfig & TRuntime) | null {
  if (!configSnapshot && !runtimeSnapshot) {
    return null;
  }
  if (!runtimeSnapshot) {
    return configSnapshot as (TConfig & TRuntime) | null;
  }
  if (!configSnapshot) {
    return runtimeSnapshot as TConfig & TRuntime;
  }
  return {
    ...configSnapshot,
    ...runtimeSnapshot,
  };
}
