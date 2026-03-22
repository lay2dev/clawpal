import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  HomeIcon,
  HashIcon,
  ClockIcon,
  HistoryIcon,
  StethoscopeIcon,
  BookOpenIcon,
  KeyRoundIcon,
  SettingsIcon,
} from "lucide-react";
import type { Route } from "../lib/routes";

interface NavItem {
  key: string;
  active: boolean;
  icon: React.ReactNode;
  label: string;
  badge?: React.ReactNode;
  onClick: () => void;
}

export function useNavItems({
  inStart,
  startSection,
  setStartSection,
  route,
  navigateRoute,
  openDoctor,
  doctorNavPulse,
}: {
  inStart: boolean;
  startSection: "overview" | "profiles" | "settings";
  setStartSection: (s: "overview" | "profiles" | "settings") => void;
  route: Route;
  navigateRoute: (r: Route) => void;
  openDoctor: () => void;
  doctorNavPulse: boolean;
}): NavItem[] {
  const { t } = useTranslation();

  return useMemo(() => {
    if (inStart) {
      return [
        {
          key: "start-profiles",
          active: startSection === "profiles",
          icon: <KeyRoundIcon className="size-4" />,
          label: t("start.nav.profiles"),
          onClick: () => { navigateRoute("home"); setStartSection("profiles"); },
        },
        {
          key: "start-settings",
          active: startSection === "settings",
          icon: <SettingsIcon className="size-4" />,
          label: t("start.nav.settings"),
          onClick: () => { navigateRoute("home"); setStartSection("settings"); },
        },
      ];
    }
    return [
      { key: "instance-home", active: route === "home", icon: <HomeIcon className="size-4" />, label: t("nav.home"), onClick: () => navigateRoute("home") },
      { key: "channels", active: route === "channels", icon: <HashIcon className="size-4" />, label: t("nav.channels"), onClick: () => navigateRoute("channels") },
      {
        key: "recipes",
        active: route === "recipes" || route === "recipe-studio" || route === "cook",
        icon: <BookOpenIcon className="size-4" />,
        label: t("nav.recipes"),
        onClick: () => navigateRoute("recipes"),
      },
      { key: "cron", active: route === "cron", icon: <ClockIcon className="size-4" />, label: t("nav.cron"), onClick: () => navigateRoute("cron") },
      {
        key: "doctor", active: route === "doctor", icon: <StethoscopeIcon className="size-4" />, label: t("nav.doctor"),
        onClick: openDoctor,
        badge: doctorNavPulse ? <span className="ml-auto h-2 w-2 rounded-full bg-primary animate-pulse" /> : undefined,
      },
      { key: "openclaw-context", active: route === "context", icon: <BookOpenIcon className="size-4" />, label: t("nav.context"), onClick: () => navigateRoute("context") },
      { key: "history", active: route === "history", icon: <HistoryIcon className="size-4" />, label: t("nav.history"), onClick: () => navigateRoute("history") },
    ];
  }, [inStart, startSection, setStartSection, route, navigateRoute, openDoctor, doctorNavPulse, t]);
}
