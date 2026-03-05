import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "@/lib/use-api";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface Message {
  role: "user" | "assistant";
  content: string;
}

const CHAT_SESSION_CACHE_PREFIX = "clawpal_chat_session_messages_v1";
const CHAT_SESSION_CACHE_MAX_MESSAGES = 180;
const CHAT_SESSION_CACHE_TTL_MS = 14 * 24 * 60 * 60 * 1000;

type ChatSessionCache = {
  version: number;
  updatedAt: number;
  messages: Message[];
};

const CHAT_SESSION_CACHE_VERSION = 1;

function chatSessionCacheKey(instanceId: string, agent: string) {
  return `${CHAT_SESSION_CACHE_PREFIX}-${instanceId}-${agent}`;
}

function trimChatMessages(rawMessages: Message[]): Message[] {
  if (rawMessages.length <= CHAT_SESSION_CACHE_MAX_MESSAGES) return rawMessages;
  return rawMessages.slice(-CHAT_SESSION_CACHE_MAX_MESSAGES);
}

function loadChatSessionMessages(instanceId: string, agent: string): Message[] {
  try {
    const raw = localStorage.getItem(chatSessionCacheKey(instanceId, agent));
    if (!raw) return [];
    const parsed = JSON.parse(raw) as ChatSessionCache | null;
    if (!parsed || typeof parsed !== "object") return [];
    if (parsed.version !== CHAT_SESSION_CACHE_VERSION) return [];
    if (!Array.isArray(parsed.messages)) return [];
    if (typeof parsed.updatedAt !== "number" || Date.now() - parsed.updatedAt > CHAT_SESSION_CACHE_TTL_MS) {
      localStorage.removeItem(chatSessionCacheKey(instanceId, agent));
      return [];
    }
    return parsed.messages.filter(
      (message) =>
        message
        && typeof message === "object"
        && typeof message.role === "string"
        && typeof message.content === "string",
    ) as Message[];
  } catch (error) {
    console.warn("Failed to load chat session messages:", error);
    return [];
  }
}

function saveChatSessionMessages(instanceId: string, agent: string, messages: Message[]) {
  try {
    const key = chatSessionCacheKey(instanceId, agent);
    const nextMessages = trimChatMessages(messages);
    if (nextMessages.length === 0) {
      localStorage.removeItem(key);
      return;
    }
    const payload: ChatSessionCache = {
      version: CHAT_SESSION_CACHE_VERSION,
      updatedAt: Date.now(),
      messages: nextMessages,
    };
    localStorage.setItem(key, JSON.stringify(payload));
  } catch (error) {
    console.warn("Failed to persist chat session messages:", error);
  }
}

function clearChatSessionMessages(instanceId: string, agent: string) {
  localStorage.removeItem(chatSessionCacheKey(instanceId, agent));
}

const SESSION_KEY_PREFIX = "clawpal_chat_session_";

function loadSessionId(instanceId: string, agent: string): string | undefined {
  return localStorage.getItem(SESSION_KEY_PREFIX + instanceId + "_" + agent) || undefined;
}
function saveSessionId(instanceId: string, agent: string, sid: string) {
  localStorage.setItem(SESSION_KEY_PREFIX + instanceId + "_" + agent, sid);
}
function clearSessionId(instanceId: string, agent: string) {
  localStorage.removeItem(SESSION_KEY_PREFIX + instanceId + "_" + agent);
}

const CLAWPAL_CONTEXT = `[ClawPal Context] You are responding inside ClawPal, a desktop GUI for OpenClaw configuration.
Rules:
- You are in READ-ONLY advisory mode. Do NOT execute commands, send messages, or modify config directly.
- When the user asks to change something, explain what should be changed and show the config diff, but do NOT apply it.
- Only discuss OpenClaw configuration topics (agents, models, channels, recipes, memory, sessions).
- Keep responses concise (2-3 sentences unless the user asks for detail).
User message: `;

export function Chat() {
  const { t } = useTranslation();
  const ua = useApi();
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [agents, setAgents] = useState<string[]>([]);
  const [agentId, setAgentId] = useState("");
  const [sessionId, setSessionId] = useState<string | undefined>(undefined);
  const bottomRef = useRef<HTMLDivElement>(null);
  const agentIdRef = useRef("");

  useEffect(() => {
    agentIdRef.current = agentId;
  }, [agentId]);

  useEffect(() => {
    const previousAgentId = agentIdRef.current;
    setAgentId("");
    setSessionId(undefined);
    setMessages([]);
    ua.listAgents()
      .then((list) => {
        const ids = list.map((a) => a.id);
        setAgents(ids);
        const nextAgent =
          ids.includes(previousAgentId) && previousAgentId
            ? previousAgentId
            : (ids[0] || "");
        setAgentId(nextAgent);
        if (nextAgent) {
          setSessionId(loadSessionId(ua.instanceId, nextAgent));
          setMessages(loadChatSessionMessages(ua.instanceId, nextAgent));
        } else {
          setSessionId(undefined);
          setMessages([]);
        }
      })
      .catch((e) => console.error("Failed to load agent IDs:", e));
  }, [ua.instanceId, ua]);

  useEffect(() => {
    if (!agentId) return;
    if (messages.length > 0) {
      saveChatSessionMessages(ua.instanceId, agentId, messages);
    } else {
      clearChatSessionMessages(ua.instanceId, agentId);
    }
  }, [messages, agentId, ua.instanceId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, loading]);

  const send = useCallback(async () => {
    if (!input.trim() || loading || !agentId) return;
    if (ua.isRemote && !ua.isConnected) return;

    const userMsg: Message = { role: "user", content: input.trim() };
    setMessages((prev) => [...prev, userMsg]);
    setInput("");
    setLoading(true);

    try {
      // Inject ClawPal context on first message of a session
      const payload = sessionId ? userMsg.content : CLAWPAL_CONTEXT + userMsg.content;
      const result = await ua.chatViaOpenclaw(agentId, payload, sessionId);
      // Extract session ID for conversation continuity
      const meta = result.meta as Record<string, unknown> | undefined;
      const agentMeta = meta?.agentMeta as Record<string, unknown> | undefined;
      if (agentMeta?.sessionId) {
        const sid = agentMeta.sessionId as string;
        setSessionId(sid);
        saveSessionId(ua.instanceId, agentId, sid);
      }
      // Extract reply text
      const payloads = result.payloads as Array<{ text?: string }> | undefined;
      const text = payloads?.map((p) => p.text).filter(Boolean).join("\n") || t('chat.noResponse');
      setMessages((prev) => [...prev, { role: "assistant", content: text }]);
    } catch (err) {
      setMessages((prev) => [...prev, { role: "assistant", content: `Error: ${err}` }]);
    } finally {
      setLoading(false);
    }
  }, [input, loading, agentId, sessionId, ua, t]);

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 mb-2">
        <Select value={agentId} onValueChange={(a) => {
          setAgentId(a);
          setSessionId(loadSessionId(ua.instanceId, a));
          setMessages(loadChatSessionMessages(ua.instanceId, a));
        }}>
          <SelectTrigger size="sm" className="w-auto text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {agents.map((a) => (
              <SelectItem key={a} value={a}>{a}</SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          variant="ghost"
          size="sm"
          className="text-xs opacity-70"
          onClick={() => { clearSessionId(ua.instanceId, agentId); setSessionId(undefined); setMessages([]); }}
        >
          {t('chat.new')}
        </Button>
      </div>
      <ScrollArea className="flex-1 mb-2 overflow-hidden">
        {agents.length === 0 && (
          <div className="text-sm text-muted-foreground p-4 text-center">{t('chat.noAgents')}</div>
        )}
        {messages.map((msg, i) => (
          <div key={i} className={cn("mb-2", msg.role === "user" ? "text-right" : "text-left")}>
            <div className={cn(
              "inline-block px-3 py-2 rounded-lg max-w-[90%] text-left border border-border",
              msg.role === "user" ? "bg-muted" : "bg-card"
            )}>
              <div className="whitespace-pre-wrap text-sm">{msg.content}</div>
            </div>
          </div>
        ))}
        {loading && <div className="opacity-50 text-sm">{t('chat.thinking')}</div>}
        <div ref={bottomRef} />
      </ScrollArea>
      <div className="flex gap-2">
        <Input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); } }}
          placeholder={t('chat.placeholder')}
          className="flex-1"
        />
        <Button
          onClick={send}
          disabled={loading || !agentId}
        >
          {t('chat.send')}
        </Button>
      </div>
    </div>
  );
}
