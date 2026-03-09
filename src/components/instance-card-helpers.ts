import type { TFunction } from "i18next";
import type { SshConnectionBottleneckStage } from "@/lib/types";

export function getConnectionQualityLabel(quality: string, t: TFunction): string {
  switch (quality) {
    case "excellent":
      return t("start.sshQualityExcellent");
    case "good":
      return t("start.sshQualityGood");
    case "fair":
      return t("start.sshQualityFair");
    case "poor":
      return t("start.sshQualityPoor");
    default:
      return t("start.sshQualityUnknown");
  }
}

export function getConnectionStageLabel(stage: SshConnectionBottleneckStage, t: TFunction): string {
  switch (stage) {
    case "connect":
      return t("start.sshStage.connect");
    case "gateway":
      return t("start.sshStage.gateway");
    case "config":
      return t("start.sshStage.config");
    case "agents":
      return t("start.sshStage.agents");
    case "version":
      return t("start.sshStage.version");
    default:
      return t("start.sshStage.other");
  }
}

export function getSshDotClass(quality: string): string {
  switch (quality) {
    case "excellent":
      return "bg-emerald-500 shadow-[0_0_12px_rgba(16,185,129,0.45)]";
    case "good":
      return "bg-lime-500 shadow-[0_0_12px_rgba(132,204,22,0.45)]";
    case "fair":
      return "bg-amber-500 shadow-[0_0_12px_rgba(217,119,6,0.45)]";
    case "poor":
      return "bg-red-500 shadow-[0_0_12px_rgba(220,38,38,0.45)]";
    default:
      return "bg-muted-foreground/40";
  }
}
