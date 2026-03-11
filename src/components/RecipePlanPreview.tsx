import type { PrecheckIssue, RecipePlan } from "@/lib/types";

function formatClaim(claim: RecipePlan["concreteClaims"][number]) {
  const details = [claim.id, claim.target, claim.path].filter(Boolean).join(" · ");
  return details ? `${claim.kind}: ${details}` : claim.kind;
}

function formatIssue(issue: PrecheckIssue) {
  return `${issue.code}: ${issue.message}`;
}

export function RecipePlanPreview({
  plan,
  routeSummary,
  authIssues = [],
  contextWarnings = [],
}: {
  plan: RecipePlan;
  routeSummary?: string;
  authIssues?: PrecheckIssue[];
  contextWarnings?: string[];
}) {
  const hasBlockingAuthIssue = authIssues.some((issue) => issue.severity === "error");
  const combinedWarnings = [...plan.warnings, ...contextWarnings];

  return (
    <div className="mb-4 rounded-lg border border-border/70 bg-muted/20 p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="text-sm font-medium">{plan.summary.recipeName}</div>
          <div className="text-xs text-muted-foreground">
            {plan.summary.executionKind} · {plan.summary.actionCount} action
            {plan.summary.actionCount === 1 ? "" : "s"}
            {plan.summary.skippedStepCount > 0
              ? ` · ${plan.summary.skippedStepCount} skipped`
              : ""}
          </div>
        </div>
        <div className="text-right">
          <div className="text-[11px] uppercase tracking-[0.2em] text-muted-foreground">
            Execution Digest
          </div>
          <div className="font-mono text-xs">{plan.executionSpecDigest}</div>
        </div>
      </div>

      {routeSummary && (
        <div className="mt-4 rounded-md border border-border/70 bg-background/80 px-3 py-2">
          <div className="text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
            Route
          </div>
          <div className="mt-1 font-mono text-xs">{routeSummary}</div>
        </div>
      )}

      <div className="mt-4 grid gap-4 md:grid-cols-2">
        <div>
          <div className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
            Capabilities
          </div>
          <div className="mt-2 flex flex-wrap gap-2">
            {plan.usedCapabilities.map((capability) => (
              <span
                key={capability}
                className="rounded-full bg-background px-2.5 py-1 font-mono text-xs"
              >
                {capability}
              </span>
            ))}
          </div>
        </div>

        <div>
          <div className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
            Resource Claims
          </div>
          <ul className="mt-2 space-y-1 text-sm text-muted-foreground">
            {plan.concreteClaims.map((claim, index) => (
              <li key={`${claim.kind}-${claim.id ?? claim.path ?? index}`}>
                {formatClaim(claim)}
              </li>
            ))}
          </ul>
        </div>
      </div>

      {authIssues.length > 0 && (
        <div
          className={
            hasBlockingAuthIssue
              ? "mt-4 rounded-md border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive"
              : "mt-4 rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-amber-950"
          }
        >
          <div className="font-medium">Auth Preconditions</div>
          {authIssues.map((issue) => (
            <div key={`${issue.code}:${issue.message}`} className="mt-1">
              {formatIssue(issue)}
            </div>
          ))}
        </div>
      )}

      {combinedWarnings.length > 0 && (
        <div className="mt-4 rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-amber-950">
          {combinedWarnings.map((warning) => (
            <div key={warning}>{warning}</div>
          ))}
        </div>
      )}
    </div>
  );
}
