export type Route =
  | "home"
  | "recipes"
  | "recipe-studio"
  | "cook"
  | "history"
  | "channels"
  | "cron"
  | "doctor"
  | "context"
  | "orchestrator";

export const INSTANCE_ROUTES: Route[] = ["home", "channels", "recipes", "cron", "doctor", "context", "history"];

export const OPEN_TABS_STORAGE_KEY = "clawpal_open_tabs";
