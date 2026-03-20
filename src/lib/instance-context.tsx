import { createContext, useContext } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { AgentOverview, ChannelNode, DiscordGuildChannel, ModelProfile } from "./types";

interface InstanceContextValue {
  instanceId: string;
  instanceLabel?: string | null;
  instanceViewToken: string;
  instanceToken: number;
  persistenceScope: string | null;
  persistenceResolved: boolean;
  isRemote: boolean;
  isDocker: boolean;
  isConnected: boolean;
  channelNodes: ChannelNode[] | null;
  discordGuildChannels: DiscordGuildChannel[] | null;
  channelsLoading: boolean;
  discordChannelsLoading: boolean;
  discordChannelsResolved: boolean;
  agents: AgentOverview[] | null;
  agentsLoading: boolean;
  modelProfiles: ModelProfile[] | null;
  modelProfilesLoading: boolean;
  setAgentsCache: Dispatch<SetStateAction<AgentOverview[] | null>>;
  refreshAgentsCache: () => Promise<AgentOverview[]>;
  refreshModelProfilesCache: () => Promise<ModelProfile[]>;
  refreshChannelNodesCache: () => Promise<ChannelNode[]>;
  refreshDiscordChannelsCache: () => Promise<DiscordGuildChannel[]>;
}

export const InstanceContext = createContext<InstanceContextValue>({
  instanceId: "local",
  instanceLabel: "local",
  instanceViewToken: "local",
  instanceToken: 0,
  persistenceScope: "local",
  persistenceResolved: true,
  isRemote: false,
  isDocker: false,
  isConnected: true,
  channelNodes: null,
  discordGuildChannels: null,
  channelsLoading: false,
  discordChannelsLoading: false,
  discordChannelsResolved: false,
  agents: null,
  agentsLoading: false,
  modelProfiles: null,
  modelProfilesLoading: false,
  setAgentsCache: () => null,
  refreshAgentsCache: async () => [],
  refreshModelProfilesCache: async () => [],
  refreshChannelNodesCache: async () => [],
  refreshDiscordChannelsCache: async () => [],
});

export function useInstance() {
  return useContext(InstanceContext);
}
