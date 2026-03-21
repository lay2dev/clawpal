import { createContext, useContext } from "react";
import type { Dispatch, SetStateAction } from "react";
import type {
  AgentOverview,
  AgentSessionAnalysis,
  BackupInfo,
  ChannelNode,
  ChannelsConfigSnapshot,
  ChannelsRuntimeSnapshot,
  DiscordGuildChannel,
  HistoryItem,
  ModelProfile,
  RecipeRuntimeRun,
  SessionFile,
} from "./types";

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
  channelsConfigSnapshot?: ChannelsConfigSnapshot | null;
  channelsRuntimeSnapshot?: ChannelsRuntimeSnapshot | null;
  channelsSnapshotsLoading?: boolean;
  channelsSnapshotsLoaded?: boolean;
  historyItems?: HistoryItem[];
  historyRuns?: RecipeRuntimeRun[];
  historyLoading?: boolean;
  historyLoaded?: boolean;
  sessionFiles?: SessionFile[];
  sessionAnalysis?: AgentSessionAnalysis[] | null;
  sessionsLoading?: boolean;
  sessionsLoaded?: boolean;
  backups?: BackupInfo[] | null;
  backupsLoading?: boolean;
  backupsLoaded?: boolean;
  setAgentsCache: Dispatch<SetStateAction<AgentOverview[] | null>>;
  setSessionAnalysis?: Dispatch<SetStateAction<AgentSessionAnalysis[] | null>>;
  setBackups?: Dispatch<SetStateAction<BackupInfo[] | null>>;
  refreshAgentsCache: () => Promise<AgentOverview[]>;
  refreshModelProfilesCache: () => Promise<ModelProfile[]>;
  refreshChannelNodesCache: () => Promise<ChannelNode[]>;
  refreshDiscordChannelsCache: (force?: boolean) => Promise<DiscordGuildChannel[]>;
  refreshChannelsSnapshotState?: () => Promise<void>;
  refreshHistoryState?: () => Promise<void>;
  refreshSessionFiles?: () => Promise<SessionFile[]>;
  refreshBackups?: () => Promise<BackupInfo[]>;
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
  channelsConfigSnapshot: null,
  channelsRuntimeSnapshot: null,
  channelsSnapshotsLoading: false,
  channelsSnapshotsLoaded: false,
  historyItems: [],
  historyRuns: [],
  historyLoading: false,
  historyLoaded: false,
  sessionFiles: [],
  sessionAnalysis: null,
  sessionsLoading: false,
  sessionsLoaded: false,
  backups: null,
  backupsLoading: false,
  backupsLoaded: false,
  setAgentsCache: () => null,
  setSessionAnalysis: () => null,
  setBackups: () => null,
  refreshAgentsCache: async () => [],
  refreshModelProfilesCache: async () => [],
  refreshChannelNodesCache: async () => [],
  refreshDiscordChannelsCache: async () => [],
  refreshChannelsSnapshotState: async () => {},
  refreshHistoryState: async () => {},
  refreshSessionFiles: async () => [],
  refreshBackups: async () => [],
});

export function useInstance() {
  return useContext(InstanceContext);
}
