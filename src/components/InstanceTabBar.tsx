import { useTranslation } from "react-i18next";
import { XIcon } from "lucide-react";
import { cn } from "@/lib/utils";

interface InstanceTabBarProps {
  openTabs: Array<{ id: string; label: string; type: "local" | "docker" | "ssh" }>;
  activeId: string | null;
  startActive: boolean;
  connectionStatus: Record<string, "connected" | "disconnected" | "error">;
  appVersion?: string;
  onSelectStart: () => void;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
}

export function InstanceTabBar({
  openTabs,
  activeId,
  startActive,
  connectionStatus,
  appVersion,
  onSelectStart,
  onSelect,
  onClose,
}: InstanceTabBarProps) {
  const { t } = useTranslation();

  const statusDot = (status: "connected" | "disconnected" | "error" | undefined) => {
    const color =
      status === "connected"
        ? "bg-emerald-500"
        : status === "error"
          ? "bg-red-400"
          : "bg-muted-foreground/40";
    return <span className={cn("inline-block w-2 h-2 rounded-full shrink-0 transition-colors duration-300", color)} />;
  };

  return (
    <div className="flex items-center gap-2 px-3 py-2 bg-sidebar border-b border-sidebar-border shrink-0">
      {/* Start tab - fixed, not closeable */}
      <div className="flex items-center pr-3 mr-1 border-r border-sidebar-border/80 shrink-0">
        <button
          className={cn(
            "flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm whitespace-nowrap transition-all duration-200 cursor-pointer",
            startActive
              ? "bg-primary/10 text-primary font-semibold border border-primary/20"
              : "text-muted-foreground hover:text-foreground hover:bg-accent"
          )}
          onClick={onSelectStart}
        >
          <span aria-hidden>🧭</span>
          {t("instance.start")}
        </button>
      </div>

      {/* Instance tabs */}
      <div className="flex items-center gap-1 overflow-x-auto min-w-0 flex-1">
        {openTabs.map((tab) => (
          <div key={tab.id} className="relative group">
            <button
              className={cn(
                "flex items-center gap-1.5 px-3 py-1.5 pr-7 rounded-lg text-sm whitespace-nowrap transition-all duration-200 cursor-pointer",
                !startActive && activeId === tab.id
                  ? "bg-card shadow-sm font-semibold text-primary border-b-2 border-b-primary"
                  : "text-muted-foreground hover:text-foreground"
              )}
              onClick={() => onSelect(tab.id)}
            >
              {statusDot(tab.type === "local" || tab.type === "docker" ? "connected" : connectionStatus[tab.id])}
              {tab.label}
            </button>
            <button
              className={cn(
                "absolute right-1 top-1/2 -translate-y-1/2 inline-flex items-center justify-center w-4 h-4 rounded text-muted-foreground hover:text-foreground hover:bg-accent transition-all cursor-pointer",
                "opacity-0 group-hover:opacity-100"
              )}
              title={t("instance.close")}
              onClick={(e) => {
                e.stopPropagation();
                onClose(tab.id);
              }}
            >
              <XIcon className="size-3" />
            </button>
          </div>
        ))}
      </div>
      <div className="shrink-0 pl-2 text-xs text-muted-foreground/80">
        {appVersion ? `v${appVersion}` : ""}
      </div>
    </div>
  );
}
