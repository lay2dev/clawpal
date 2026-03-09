export function shouldEnableLocalInstanceScope({
  configExists,
  cliAvailable,
}: {
  configExists: boolean;
  cliAvailable: boolean;
}): boolean {
  return configExists && cliAvailable;
}

export function shouldEnableInstanceLiveReads({
  instanceToken,
  persistenceResolved,
  persistenceScope,
  isRemote,
}: {
  instanceToken: number;
  persistenceResolved: boolean;
  persistenceScope: string | null;
  isRemote: boolean;
}): boolean {
  if (instanceToken === 0 || !persistenceResolved) {
    return false;
  }
  if (isRemote) {
    return true;
  }
  return Boolean(persistenceScope);
}
