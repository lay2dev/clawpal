import { useCallback, useEffect, useSyncExternalStore } from "react";

export type UiFont = "wenkai" | "nunito" | "system" | "serif";

const STORAGE_KEY = "clawpal_font";

let listeners: (() => void)[] = [];

function emitChange() {
  for (const listener of listeners) listener();
}

function getStoredFont(): UiFont {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    if (value === "wenkai" || value === "nunito" || value === "system" || value === "serif") {
      return value;
    }
  } catch {}
  return "system";
}

function applyFont(font: UiFont) {
  document.documentElement.setAttribute("data-font", font);
}

function getSnapshot(): UiFont {
  return getStoredFont();
}

function subscribe(cb: () => void) {
  listeners.push(cb);
  return () => {
    listeners = listeners.filter((listener) => listener !== cb);
  };
}

export function useFont() {
  const font = useSyncExternalStore(subscribe, getSnapshot, () => "system" as UiFont);

  const setFont = useCallback((next: UiFont) => {
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {}
    applyFont(next);
    emitChange();
  }, []);

  useEffect(() => {
    applyFont(font);
  }, [font]);

  return { font, setFont } as const;
}
