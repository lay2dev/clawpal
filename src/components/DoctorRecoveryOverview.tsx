import { ExternalLinkIcon, WrenchIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

import type {
  RescuePrimaryDiagnosisResult,
  RescuePrimaryRepairResult,
  RescuePrimarySectionItem,
} from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";

interface DoctorRecoveryOverviewProps {
  diagnosis: RescuePrimaryDiagnosisResult;
  checkLoading: boolean;
  repairing: boolean;
  progressLine: string | null;
  repairResult: RescuePrimaryRepairResult | null;
  repairError: string | null;
  onRepairAll: () => void;
  onRepairIssue: (issueId: string) => void;
}

function itemBadgeVariant(status: RescuePrimarySectionItem["status"]) {
  return status === "error" ? "destructive" : "outline";
}

function statusBadgeClass(
  status: RescuePrimaryDiagnosisResult["status"] | RescuePrimarySectionItem["status"],
) {
  if (status === "healthy" || status === "ok") {
    return "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:border-emerald-400/30 dark:bg-emerald-400/10 dark:text-emerald-300";
  }
  if (status === "degraded" || status === "warn") {
    return "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:border-amber-400/30 dark:bg-amber-400/10 dark:text-amber-300";
  }
  if (status === "inactive" || status === "info") {
    return "border-border/60 bg-muted/30 text-muted-foreground";
  }
  return "";
}

export function DoctorRecoveryOverview({
  diagnosis,
  checkLoading,
  repairing,
  progressLine,
  repairResult,
  repairError,
  onRepairAll,
  onRepairIssue,
}: DoctorRecoveryOverviewProps) {
  const { t } = useTranslation();
  const fixableCount = diagnosis.summary.fixableIssueCount;
  const fixText = t("doctor.fixSafeIssues", {
    count: fixableCount,
    defaultValue: fixableCount === 1 ? "Fix 1 safe issue" : `Fix ${fixableCount} safe issues`,
  });
  const translateStatus = (
    status: RescuePrimaryDiagnosisResult["status"] | RescuePrimarySectionItem["status"],
  ) => {
    if (status === "healthy" || status === "ok") {
      return t("doctor.primaryStatusHealthy", { defaultValue: "Healthy" });
    }
    if (status === "degraded" || status === "warn") {
      return t("doctor.primaryStatusDegraded", { defaultValue: "Degraded" });
    }
    if (status === "broken" || status === "error") {
      return t("doctor.primaryStatusBroken", { defaultValue: "Broken" });
    }
    if (status === "inactive") {
      return t("doctor.primaryStatusInactive", { defaultValue: "Inactive" });
    }
    return status;
  };

  return (
    <div className="mt-4 space-y-4">
      <Card className="border-border/60 bg-muted/20">
        <CardHeader className="pb-3">
          <div className="flex items-start justify-between gap-3">
            <div className="space-y-1">
              <CardTitle className="text-base">{diagnosis.summary.headline}</CardTitle>
              <p className="text-sm text-muted-foreground">
                {diagnosis.summary.recommendedAction}
              </p>
            </div>
            <Badge
              variant={diagnosis.summary.status === "broken" ? "destructive" : "outline"}
              className={statusBadgeClass(diagnosis.summary.status)}
            >
              {translateStatus(diagnosis.summary.status)}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              onClick={onRepairAll}
              disabled={checkLoading || repairing || fixableCount === 0}
            >
              <WrenchIcon className="mr-1.5 size-3.5" />
              {fixText}
            </Button>
          </div>
          {progressLine ? (
            <div className="h-5 overflow-hidden text-sm text-muted-foreground">
              <span
                key={progressLine}
                className="inline-block whitespace-nowrap transition-opacity duration-300 animate-pulse"
              >
                {progressLine}
              </span>
            </div>
          ) : null}
          {repairResult ? (
            <div className="text-sm text-muted-foreground">
              {t("doctor.repairSummaryInline", {
                defaultValue:
                  "Fixed {{applied}} issue(s), skipped {{skipped}}, failed {{failed}}.",
                applied: repairResult.appliedIssueIds.length,
                skipped: repairResult.skippedIssueIds.length,
                failed: repairResult.failedIssueIds.length,
              })}
            </div>
          ) : null}
          {repairError ? (
            <div className="text-sm text-destructive">{repairError}</div>
          ) : null}
        </CardContent>
      </Card>

      <div className="grid gap-3">
        {diagnosis.sections.map((section) => (
          <Card key={section.key} className="gap-2 py-4">
            <details
              open={section.status !== "healthy" ? true : undefined}
              className="group"
            >
              <summary className="list-none cursor-pointer">
                <CardHeader className="pb-0">
                  <div className="flex items-center justify-between gap-3">
                    <div className="space-y-1">
                      <CardTitle className="text-sm">{section.title}</CardTitle>
                      <p className="text-sm text-muted-foreground">{section.summary}</p>
                    </div>
                    <div className="flex items-center gap-2">
                      <Badge
                        variant={section.status === "broken" ? "destructive" : "outline"}
                        className={statusBadgeClass(section.status)}
                      >
                        {translateStatus(section.status)}
                      </Badge>
                      <Button
                        asChild
                        variant="ghost"
                        size="icon-sm"
                        className="text-muted-foreground hover:text-foreground"
                      >
                        <a
                          href={section.docsUrl}
                          target="_blank"
                          rel="noreferrer"
                          aria-label={`Open ${section.title} docs`}
                          title={`Open ${section.title} docs`}
                        >
                          <ExternalLinkIcon className="size-3.5" />
                        </a>
                      </Button>
                    </div>
                  </div>
                </CardHeader>
              </summary>
              <CardContent className="pt-3">
                <div className="grid gap-2">
                  {section.items.map((item) => (
                    <div
                      key={item.id}
                      className="rounded-md border border-border/50 bg-background/70 p-2"
                    >
                      <div className="flex items-center justify-between gap-3">
                        <div className="min-w-0">
                          <div className="text-sm">{item.label}</div>
                          {item.detail ? (
                            <div className="mt-1 text-xs text-muted-foreground">
                              {item.detail}
                            </div>
                          ) : null}
                        </div>
                        <div className="flex items-center gap-2">
                          {item.autoFixable && item.issueId ? (
                            <Button
                              variant="outline"
                              size="sm"
                              className="h-7 px-2 text-[11px]"
                              onClick={() => onRepairIssue(item.issueId!)}
                              disabled={checkLoading || repairing}
                            >
                              {t("doctor.fix", { defaultValue: "Fix" })}
                            </Button>
                          ) : null}
                          <Badge
                            variant={itemBadgeVariant(item.status)}
                            className={cn("text-[10px]", statusBadgeClass(item.status))}
                          >
                            {translateStatus(item.status)}
                          </Badge>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              </CardContent>
            </details>
          </Card>
        ))}
      </div>
    </div>
  );
}
