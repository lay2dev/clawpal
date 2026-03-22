import { useMemo, useState } from "react";
import { ChevronDownIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { cn, formatTime } from "@/lib/utils";
import type { RecipeRuntimeAuditEntry } from "@/lib/types";

function statusClass(status: RecipeRuntimeAuditEntry["status"]): string {
  if (status === "succeeded") return "bg-emerald-500/10 text-emerald-600";
  if (status === "failed") return "bg-red-500/10 text-red-600";
  return "bg-muted text-muted-foreground";
}

function statusLabel(
  t: (key: string, args?: Record<string, unknown>) => string,
  status: RecipeRuntimeAuditEntry["status"],
): string {
  if (status === "succeeded") return t("cook.activityStatusSucceeded");
  if (status === "failed") return t("cook.activityStatusFailed");
  return t("cook.activityStatusStarted");
}

export function CookActivityPanel({
  title,
  description,
  activities,
  open,
  onOpenChange,
}: {
  title: string;
  description: string;
  activities: RecipeRuntimeAuditEntry[];
  open: boolean;
  onOpenChange: (next: boolean) => void;
}) {
  const { t } = useTranslation();
  const [expandedItems, setExpandedItems] = useState<Record<string, boolean>>({});
  const sorted = useMemo(
    () =>
      [...activities].sort((left, right) =>
        left.startedAt.localeCompare(right.startedAt),
      ),
    [activities],
  );

  return (
    <div className="rounded-md border border-border/70 bg-background/80 px-3 py-2">
      <button
        type="button"
        className="flex w-full items-center justify-between gap-3 text-left"
        onClick={() => onOpenChange(!open)}
      >
        <div>
          <div className="text-sm font-medium text-foreground">{title}</div>
          <div className="text-xs text-muted-foreground">{description}</div>
        </div>
        <ChevronDownIcon
          className={cn(
            "size-4 text-muted-foreground transition-transform",
            open && "rotate-180",
          )}
          aria-hidden="true"
        />
      </button>
      {open && (
        <div className="mt-3 space-y-3">
          {sorted.length === 0 ? (
            <div className="text-sm text-muted-foreground">{t("cook.activityEmpty")}</div>
          ) : (
            sorted.map((activity) => {
              const detailOpen = !!expandedItems[activity.id];
              return (
                <div
                  key={activity.id}
                  className="rounded-md border border-border/60 bg-muted/15 px-3 py-3"
                >
                  <button
                    type="button"
                    className="flex w-full items-start justify-between gap-3 text-left"
                    onClick={() =>
                      setExpandedItems((current) => ({
                        ...current,
                        [activity.id]: !current[activity.id],
                      }))
                    }
                  >
                    <div className="min-w-0 space-y-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <div className="text-sm font-medium text-foreground">
                          {activity.label}
                        </div>
                        <Badge className={statusClass(activity.status)}>
                          {statusLabel(t, activity.status)}
                        </Badge>
                        {activity.sideEffect && (
                          <Badge variant="outline">{t("cook.activitySideEffectBadge")}</Badge>
                        )}
                      </div>
                      <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                        <span>{formatTime(activity.startedAt)}</span>
                        {activity.target && <span>{activity.target}</span>}
                        {typeof activity.exitCode === "number" && (
                          <span>{t("cook.activityExitCode", { code: activity.exitCode })}</span>
                        )}
                      </div>
                    </div>
                    <ChevronDownIcon
                      className={cn(
                        "mt-0.5 size-4 shrink-0 text-muted-foreground transition-transform",
                        detailOpen && "rotate-180",
                      )}
                      aria-hidden="true"
                    />
                  </button>
                  {detailOpen && (
                    <div className="mt-3 space-y-3">
                      {activity.displayCommand && (
                        <div>
                          <div className="text-xs font-medium uppercase tracking-[0.14em] text-muted-foreground">
                            {t("cook.activityCommand")}
                          </div>
                          <pre className="mt-1 overflow-x-auto rounded-md bg-muted/40 px-3 py-2 text-xs text-foreground whitespace-pre-wrap break-all">
                            {activity.displayCommand}
                          </pre>
                        </div>
                      )}
                      {activity.stdoutSummary && (
                        <div>
                          <div className="text-xs font-medium uppercase tracking-[0.14em] text-muted-foreground">
                            {t("cook.activityStdout")}
                          </div>
                          <pre className="mt-1 overflow-x-auto rounded-md bg-muted/40 px-3 py-2 text-xs text-foreground whitespace-pre-wrap break-all">
                            {activity.stdoutSummary}
                          </pre>
                        </div>
                      )}
                      {activity.stderrSummary && (
                        <div>
                          <div className="text-xs font-medium uppercase tracking-[0.14em] text-muted-foreground">
                            {t("cook.activityStderr")}
                          </div>
                          <pre className="mt-1 overflow-x-auto rounded-md bg-muted/40 px-3 py-2 text-xs text-foreground whitespace-pre-wrap break-all">
                            {activity.stderrSummary}
                          </pre>
                        </div>
                      )}
                      {activity.details && (
                        <div>
                          <div className="text-xs font-medium uppercase tracking-[0.14em] text-muted-foreground">
                            {t("cook.activityDetails")}
                          </div>
                          <div className="mt-1 text-sm text-muted-foreground">
                            {activity.details}
                          </div>
                        </div>
                      )}
                      {activity.sideEffect && (
                        <div className="text-xs text-muted-foreground">
                          {t("cook.activitySideEffectNote")}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>
      )}
    </div>
  );
}
