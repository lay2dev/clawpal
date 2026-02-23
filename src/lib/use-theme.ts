import { useCallback, useEffect, useSyncExternalStore } from "react";

export type Theme = "light" | "dark" | "system";

const STORAGE_KEY = "clawpal_theme";
const DARK_MQ = "(prefers-color-scheme: dark)";

let listeners: (() => void)[] = [];
function emitChange() {
  for (const l of listeners) l();
}

function getStoredTheme(): Theme {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {}
  return "system";
}

function getSnapshot(): Theme {
  return getStoredTheme();
}

function subscribe(cb: () => void) {
  listeners.push(cb);
  return () => {
    listeners = listeners.filter((l) => l !== cb);
  };
}

function applyTheme(theme: Theme) {
  const isDark =
    theme === "dark" ||
    (theme === "system" && window.matchMedia(DARK_MQ).matches);
  document.documentElement.classList.toggle("dark", isDark);
}

export function useTheme() {
  const theme = useSyncExternalStore(subscribe, getSnapshot, () => "system" as Theme);

  const setTheme = useCallback((next: Theme) => {
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {}
    applyTheme(next);
    emitChange();
  }, []);

  // Apply on mount and listen for system preference changes
  useEffect(() => {
    applyTheme(theme);
    const mq = window.matchMedia(DARK_MQ);
    const handler = () => {
      if (getStoredTheme() === "system") applyTheme("system");
    };
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [theme]);

  return { theme, setTheme } as const;
}
