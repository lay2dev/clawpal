import { api } from "./api";
import type { ModelProfile } from "./types";

export interface ActionContext {
  instanceId: string;
  isRemote: boolean;
}

/** Resolve a profile ID to a "provider/model" string by loading profiles from local or remote. */
async function resolveProfileToModelValue(
  profileId: string | undefined,
  ctx?: ActionContext,
): Promise<string | undefined> {
  if (!profileId || profileId === "__default__") return undefined;
  const profiles: ModelProfile[] = ctx?.isRemote
    ? await api.remoteListModelProfiles(ctx.instanceId)
    : await api.listModelProfiles();
  const profile = profiles.find((p) => p.id === profileId);
  if (!profile) return profileId; // fallback: use raw string
  return profile.model.includes("/")
    ? profile.model
    : `${profile.provider}/${profile.model}`;
}

export interface ActionDef {
  execute: (args: Record<string, unknown>, ctx?: ActionContext) => Promise<unknown>;
  describe: (args: Record<string, unknown>) => string;
}

function renderArgs(
  args: Record<string, unknown>,
  params: Record<string, string>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(args)) {
    if (typeof value === "string") {
      // If the entire value is a single template like "{{param}}", resolve to native type
      const singleMatch = value.match(/^\{\{(\w+)\}\}$/);
      if (singleMatch) {
        const paramValue = params[singleMatch[1]] ?? "";
        if (paramValue === "true") result[key] = true;
        else if (paramValue === "false") result[key] = false;
        else result[key] = paramValue;
      } else {
        let rendered = value;
        for (const [paramId, paramValue] of Object.entries(params)) {
          rendered = rendered.split(`{{${paramId}}}`).join(paramValue);
        }
        result[key] = rendered;
      }
    } else {
      result[key] = value;
    }
  }
  return result;
}

const registry: Record<string, ActionDef> = {
  create_agent: {
    execute: async (args, ctx) => {
      const modelValue = await resolveProfileToModelValue(
        args.modelProfileId as string | undefined,
        ctx,
      );
      if (ctx?.isRemote) {
        return api.remoteCreateAgent(ctx.instanceId, args.agentId as string, modelValue);
      }
      return api.createAgent(
        args.agentId as string,
        modelValue,
        args.independent as boolean | undefined,
      );
    },
    describe: (args) => {
      const model = args.modelProfileId as string | undefined;
      const modelLabel = !model || model === "__default__" ? "default model" : model;
      return `Create ${args.independent ? "independent " : ""}agent "${args.agentId}" (${modelLabel})`;
    },
  },
  setup_identity: {
    execute: (args, ctx) => {
      if (ctx?.isRemote) return Promise.resolve(); // Not supported for remote yet — skip silently
      return api.setupAgentIdentity(
        args.agentId as string,
        args.name as string,
        args.emoji as string | undefined,
      );
    },
    describe: (args) => {
      const emoji = args.emoji ? ` ${args.emoji}` : "";
      return `Set identity: ${args.name}${emoji}`;
    },
  },
  bind_channel: {
    execute: (args, ctx) => {
      if (ctx?.isRemote) {
        return api.remoteAssignChannelAgent(
          ctx.instanceId,
          args.channelType as string,
          args.peerId as string,
          args.agentId as string,
        );
      }
      return api.assignChannelAgent(
        args.channelType as string,
        args.peerId as string,
        args.agentId as string,
      );
    },
    describe: (args) =>
      `Bind ${args.channelType} channel → agent "${args.agentId}"`,
  },
  config_patch: {
    execute: (args, ctx) => {
      if (ctx?.isRemote) {
        return api.remoteApplyConfigPatch(
          ctx.instanceId,
          args.patchTemplate as string,
          args.params as Record<string, string>,
        );
      }
      return api.applyConfigPatch(
        args.patchTemplate as string,
        args.params as Record<string, string>,
      );
    },
    describe: () => "",
  },
  set_global_model: {
    execute: async (args, ctx) => {
      const modelValue = await resolveProfileToModelValue(
        args.profileId as string | undefined,
        ctx,
      ) ?? null;
      if (ctx?.isRemote) {
        return api.remoteSetGlobalModel(ctx.instanceId, modelValue);
      }
      return api.setGlobalModel(modelValue);
    },
    describe: (args) => `Set default model to ${args.profileId}`,
  },
};

export function getAction(actionType: string): ActionDef | undefined {
  return registry[actionType];
}

export interface ResolvedStep {
  index: number;
  action: string;
  label: string;
  args: Record<string, unknown>;
  description: string;
  skippable: boolean;
}

export function resolveSteps(
  steps: { action: string; label: string; args: Record<string, unknown> }[],
  params: Record<string, string>,
): ResolvedStep[] {
  return steps.map((step, index) => {
    const resolved = renderArgs(step.args, params);
    if (step.action === "config_patch") {
      resolved.params = params;
    }
    // A step is skippable if any template variable in its args references an empty param
    const skippable = Object.values(step.args).some((origValue) => {
      if (typeof origValue !== "string") return false;
      const matches = origValue.match(/\{\{(\w+)\}\}/g);
      if (!matches) return false;
      return matches.some((m) => {
        const paramId = m.slice(2, -2);
        const val = params[paramId];
        return val !== undefined && val.trim() === "";
      });
    });
    const actionDef = getAction(step.action);
    const description = actionDef?.describe(resolved) || step.label;
    return {
      index,
      action: step.action,
      label: step.label,
      args: resolved,
      description: description || step.label,
      skippable,
    };
  });
}

export async function executeStep(step: ResolvedStep, ctx?: ActionContext): Promise<void> {
  const actionDef = getAction(step.action);
  if (!actionDef) {
    throw new Error(`Unknown action type: ${step.action}`);
  }
  await actionDef.execute(step.args, ctx);
}
