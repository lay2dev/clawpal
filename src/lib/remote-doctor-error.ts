const CONNECTION_LOST_PREFIX = "Connection lost while waiting for response";
const INVALID_REMOTE_DOCTOR_AUTH_MARKERS = ["invalid token", "invalid api key"];

export function formatRemoteDoctorErrorMessage(message: string): string {
  const trimmed = message.trim();
  if (!trimmed.includes(CONNECTION_LOST_PREFIX)) {
    return trimmed;
  }

  const lower = trimmed.toLowerCase();
  if (INVALID_REMOTE_DOCTOR_AUTH_MARKERS.some((marker) => lower.includes(marker))) {
    return [
      "Remote Doctor gateway rejected the saved Remote Doctor API key.",
      "Re-save the invite code in Settings to refresh it, then try again.",
      `Details: ${trimmed}`,
    ].join(" ");
  }

  return [
    "Remote Doctor server accepted the WebSocket but closed before replying.",
    "Check the invite code or saved Remote Doctor token first.",
    `Details: ${trimmed}`,
  ].join(" ");
}
