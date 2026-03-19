import { Suspense, lazy } from "react";
import { useTranslation } from "react-i18next";
import { cn, formatBytes } from "@/lib/utils";
import { api } from "../lib/api";
import type { SshTransferStats } from "../lib/types";

const PendingChangesBar = lazy(() => import("./PendingChangesBar").then((m) => ({ default: m.PendingChangesBar })));

interface ProfileSyncStatus {
  phase: "idle" | "syncing" | "success" | "error";
  message: string;
  instanceId: string | null;
}

interface SidebarFooterProps {
  profileSyncStatus: ProfileSyncStatus;
  showSshTransferSpeedUi: boolean;
  isRemote: boolean;
  isConnected: boolean;
  sshTransferStats: SshTransferStats | null;
  inStart: boolean;
  showToast: (message: string, type?: "success" | "error") => void;
  bumpConfigVersion: () => void;
}

export function SidebarFooter({
  profileSyncStatus, showSshTransferSpeedUi, isRemote, isConnected,
  sshTransferStats, inStart, showToast, bumpConfigVersion,
}: SidebarFooterProps) {
  const { t } = useTranslation();
  return (
    <>
      <div className="px-5 pb-3 text-[11px] text-muted-foreground/80">
        <div className="flex items-center gap-1.5">
          <span className={cn(
            "inline-block h-1.5 w-1.5 rounded-full",
            profileSyncStatus.phase === "syncing" && "bg-amber-500 animate-pulse",
            profileSyncStatus.phase === "success" && "bg-green-500",
            profileSyncStatus.phase === "error" && "bg-red-500",
            profileSyncStatus.phase === "idle" && "bg-muted-foreground/40",
          )} />
          <span>
            {profileSyncStatus.phase === "idle"
              ? t("doctor.profileSyncIdle")
              : profileSyncStatus.phase === "syncing"
                ? t("doctor.profileSyncSyncing", { instance: profileSyncStatus.instanceId || t("instance.current") })
                : profileSyncStatus.phase === "success"
                  ? t("doctor.profileSyncSuccessStatus", { instance: profileSyncStatus.instanceId || t("instance.current") })
                  : t("doctor.profileSyncErrorStatus", { instance: profileSyncStatus.instanceId || t("instance.current") })}
          </span>
        </div>
        {showSshTransferSpeedUi && isRemote && isConnected && (
          <div className="mt-2 border-t border-border/40 pt-2 text-muted-foreground/75">
            <div className="text-[10px] uppercase tracking-wide">{t("doctor.sshTransferSpeedTitle")}</div>
            <div className="mt-0.5">
              {t("doctor.sshTransferSpeedDown", { speed: `${formatBytes(Math.max(0, Math.round(sshTransferStats?.downloadBytesPerSec ?? 0)))} /s` })}
            </div>
            <div>
              {t("doctor.sshTransferSpeedUp", { speed: `${formatBytes(Math.max(0, Math.round(sshTransferStats?.uploadBytesPerSec ?? 0)))} /s` })}
            </div>
          </div>
        )}
      </div>
      {!inStart && (
        <Suspense fallback={null}>
          <PendingChangesBar showToast={showToast} onApplied={bumpConfigVersion} onDiscarded={bumpConfigVersion} />
        </Suspense>
      )}
      <div className="px-5 pb-3 pt-2 flex items-center gap-2 text-xs text-muted-foreground/70">
        <a href="#" className="hover:text-foreground transition-colors duration-200" onClick={(e) => { e.preventDefault(); api.openUrl("https://clawpal.xyz"); }}>{t("nav.website")}</a>
        <span className="text-border">·</span>
        <a href="#" className="hover:text-foreground transition-colors duration-200" onClick={(e) => { e.preventDefault(); api.openUrl("https://x.com/zhixianio"); }}>@zhixian</a>
      </div>
    </>
  );
}
