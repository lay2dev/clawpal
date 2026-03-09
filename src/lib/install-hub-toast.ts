export function resolveInstallHubToastError(params: {
  error: string | null | undefined;
  previousError: string | null;
  ignoredSubstrings?: string[];
}) {
  const normalizedError = typeof params.error === "string" && params.error.trim()
    ? params.error.trim()
    : null;

  if (!normalizedError) {
    return {
      nextError: null,
      toastMessage: null,
    };
  }

  const shouldIgnore = (params.ignoredSubstrings ?? []).some((pattern) =>
    normalizedError.includes(pattern),
  );

  return {
    nextError: normalizedError,
    toastMessage: shouldIgnore || normalizedError === params.previousError
      ? null
      : normalizedError,
  };
}
