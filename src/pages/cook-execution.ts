import type { ExecuteRecipeRequest, ExecutionSpec, RecipeSourceOrigin } from "@/lib/types";

export type CookStepStatus = "pending" | "running" | "done" | "failed" | "skipped";
export type CookPlanningStage = "validate" | "build" | "checks";
export type CookPhase = "params" | "confirm" | "execute" | "done";
export type CookPhaseState = "complete" | "current" | "upcoming";
export type CookExecutionState = "running" | "failed" | "done";

export interface CookPlanningCheckProgress {
  authRequired: boolean;
  configRequired: boolean;
  completedCount: number;
  totalCount: number;
}

export interface CookPhaseItem {
  key: CookPhase;
  labelKey: string;
  state: CookPhaseState;
}

export interface CookExecutionContext {
  instanceId: string;
  isRemote: boolean;
  isDocker: boolean;
}

export function buildCookExecutionSpec(
  spec: ExecutionSpec,
  context: CookExecutionContext,
): ExecutionSpec {
  const target = context.isRemote
    ? { kind: "remote_ssh", hostId: context.instanceId }
    : { kind: context.isDocker ? "docker_local" : "local" };

  return {
    ...spec,
    target,
  };
}

export function buildCookExecuteRequest(
  spec: ExecutionSpec,
  context: CookExecutionContext,
  sourceOrigin: RecipeSourceOrigin,
  sourceText?: string,
  workspaceSlug?: string,
): ExecuteRecipeRequest {
  return {
    spec: buildCookExecutionSpec(spec, context),
    sourceOrigin,
    sourceText,
    workspaceSlug,
  };
}

export function markCookStatuses(
  statuses: CookStepStatus[],
  next: Exclude<CookStepStatus, "skipped">,
): CookStepStatus[] {
  return statuses.map((status) => (status === "skipped" ? "skipped" : next));
}

export function markCookFailure(statuses: CookStepStatus[]): CookStepStatus[] {
  return statuses.map((status) => {
    if (status === "running") return "pending";
    return status;
  });
}

export function getCookPlanningProgress(
  stage: CookPlanningStage,
  checks?: CookPlanningCheckProgress,
): {
  value: number;
  labelKey: string;
  labelArgs?: Record<string, number>;
  animated: boolean;
} {
  switch (stage) {
    case "validate":
      return { value: 15, labelKey: "cook.progressValidate", labelArgs: undefined, animated: true };
    case "build":
      return { value: 52, labelKey: "cook.progressBuild", labelArgs: undefined, animated: true };
    case "checks": {
      const totalCount = Math.max(1, checks?.totalCount ?? 1);
      const completedCount = Math.max(0, Math.min(totalCount, checks?.completedCount ?? 0));
      const labelKey =
        checks?.authRequired && checks?.configRequired
          ? "cook.progressChecksBoth"
          : checks?.authRequired
            ? "cook.progressChecksAuth"
            : checks?.configRequired
              ? "cook.progressChecksConfig"
              : "cook.progressChecksBoth";
      const baseValue = 58;
      const stageSpan = 24;
      return {
        value: baseValue + Math.round((completedCount / totalCount) * stageSpan),
        labelKey,
        labelArgs: {
          complete: completedCount,
          total: totalCount,
        },
        animated: true,
      };
    }
  }
}

export function buildCookPhaseItems(currentPhase: CookPhase): CookPhaseItem[] {
  const phases: CookPhase[] = ["params", "confirm", "execute", "done"];
  const labelKeys: Record<CookPhase, string> = {
    params: "cook.phaseConfigure",
    confirm: "cook.phaseReview",
    execute: "cook.phaseExecute",
    done: "cook.phaseDone",
  };
  const currentIndex = phases.indexOf(currentPhase);

  return phases.map((phase, index) => ({
    key: phase,
    labelKey: labelKeys[phase],
    state:
      index < currentIndex
        ? "complete"
        : index === currentIndex
          ? "current"
          : "upcoming",
  }));
}

export function getCookExecutionProgress(
  executionState: CookExecutionState,
  statuses: CookStepStatus[],
): {
  value: number;
  actionableCount: number;
  totalCount: number;
  failed: boolean;
  animated: boolean;
  detailKey: string;
  detailArgs: Record<string, number>;
} {
  const actionableCount = statuses.filter((status) => status !== "skipped").length;
  const totalCount = statuses.length;

  if (statuses.length === 0) {
    return {
      value: 0,
      actionableCount: 0,
      totalCount: 0,
      failed: false,
      animated: false,
      detailKey: "cook.executionApplyingDetail",
      detailArgs: {
        actionable: 0,
        total: 0,
      },
    };
  }

  if (executionState === "done") {
    return {
      value: 100,
      actionableCount,
      totalCount,
      failed: false,
      animated: false,
      detailKey: "cook.executionDoneDetail",
      detailArgs: {
        complete: actionableCount,
        total: totalCount,
      },
    };
  }

  if (executionState === "failed") {
    return {
      value: actionableCount === 0 ? 100 : 65,
      actionableCount,
      totalCount,
      failed: true,
      animated: false,
      detailKey: "cook.executionFailedDetail",
      detailArgs: {
        actionable: actionableCount,
        total: totalCount,
      },
    };
  }

  return {
    value: actionableCount === 0 ? 100 : 65,
    actionableCount,
    totalCount,
    failed: false,
    animated: true,
    detailKey: "cook.executionApplyingDetail",
    detailArgs: {
      actionable: actionableCount,
      total: totalCount,
    },
  };
}
