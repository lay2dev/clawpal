export const OPEN_REMOTE_DOCTOR_SETTINGS_EVENT = "clawpal:open-remote-doctor-settings";

const PENDING_REMOTE_DOCTOR_SETTINGS_FOCUS_KEY = "clawpal:pending-remote-doctor-settings-focus";

export function requestRemoteDoctorSettingsFocus() {
  if (typeof window === "undefined") return;
  window.sessionStorage.setItem(PENDING_REMOTE_DOCTOR_SETTINGS_FOCUS_KEY, "1");
  window.dispatchEvent(new CustomEvent(OPEN_REMOTE_DOCTOR_SETTINGS_EVENT));
}

export function consumePendingRemoteDoctorSettingsFocus(): boolean {
  if (typeof window === "undefined") return false;
  const pending = window.sessionStorage.getItem(PENDING_REMOTE_DOCTOR_SETTINGS_FOCUS_KEY) === "1";
  if (pending) {
    window.sessionStorage.removeItem(PENDING_REMOTE_DOCTOR_SETTINGS_FOCUS_KEY);
  }
  return pending;
}
