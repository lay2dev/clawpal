import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SimpleMarkdown } from "@/components/SimpleMarkdown";
import type { DoctorChatMessage } from "@/lib/types";

interface DoctorChatProps {
  messages: DoctorChatMessage[];
  loading: boolean;
  error: string | null;
  connected: boolean;
  onSendMessage: (message: string) => void;
  onApproveInvoke: (invokeId: string) => void;
  onRejectInvoke: (invokeId: string, reason?: string) => void;
}

export function DoctorChat({
  messages,
  loading,
  error,
  connected,
  onSendMessage,
  onApproveInvoke,
  onRejectInvoke,
}: DoctorChatProps) {
  const { t } = useTranslation();
  const [input, setInput] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages, loading]);

  const handleSend = () => {
    if (!input.trim() || loading || !connected) return;
    onSendMessage(input.trim());
    setInput("");
  };

  return (
    <div className="flex flex-col">
      {/* Message list */}
      <div
        ref={scrollRef}
        className="h-[420px] mb-2 border rounded-md p-3 bg-muted/30 overflow-y-auto"
      >
        <div className="space-y-3">
          {messages.map((msg) => (
            <MessageBubble
              key={msg.id}
              message={msg}
              onApprove={onApproveInvoke}
              onReject={onRejectInvoke}
            />
          ))}
          {loading && (
            <div className="text-sm text-muted-foreground animate-pulse">
              {t("doctor.agentThinking")}
            </div>
          )}
        </div>
      </div>

      {/* Input area */}
      <div className="flex gap-2">
        <Input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              handleSend();
            }
          }}
          placeholder={t("doctor.sendFollowUp")}
          disabled={!connected || loading}
          className="flex-1"
        />
        <Button
          onClick={handleSend}
          disabled={!connected || loading || !input.trim()}
          size="sm"
        >
          {t("chat.send")} <kbd className="ml-1 text-xs opacity-60">{navigator.platform.includes("Mac") ? "⌘↵" : "Ctrl↵"}</kbd>
        </Button>
      </div>
    </div>
  );
}

function MessageBubble({
  message,
  onApprove,
  onReject,
}: {
  message: DoctorChatMessage;
  onApprove: (id: string) => void;
  onReject: (id: string, reason?: string) => void;
}) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);

  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div className="px-3 py-2 rounded-lg max-w-[85%] bg-[oklch(0.205_0_0)] dark:bg-[oklch(0.35_0.02_55)] text-white">
          <div className="whitespace-pre-wrap text-sm">{message.content}</div>
        </div>
      </div>
    );
  }

  if (message.role === "assistant") {
    return (
      <div className="flex justify-start">
        <div className="px-3 py-2 rounded-lg max-w-[85%] bg-[oklch(0.93_0_0)] dark:bg-muted dark:text-foreground">
          <div className="text-sm"><SimpleMarkdown content={message.content} /></div>
        </div>
      </div>
    );
  }

  if (message.role === "tool-call" && message.invoke) {
    const inv = message.invoke;
    const isPendingWrite = message.status === "pending" && inv.type === "write";
    const isPendingRead = message.status === "pending" && inv.type === "read";
    const statusBadge = message.status === "auto"
      ? <Badge variant="outline" className="text-xs">{t("doctor.autoExecuted")}</Badge>
      : message.status === "approved"
        ? <Badge variant="secondary" className="text-xs">{t("doctor.execute")}</Badge>
        : message.status === "rejected"
          ? <Badge variant="destructive" className="text-xs">{t("doctor.rejected")}</Badge>
          : isPendingRead
            ? <Badge variant="outline" className="text-xs">{t("doctor.firstTimeApproval")}</Badge>
            : <Badge variant="secondary" className="text-xs">{t("doctor.awaitingApproval")}</Badge>;

    return (
      <div className="rounded-md p-3 text-sm border-l-[3px] border-l-primary/40 border border-border bg-[oklch(0.96_0_0)] dark:bg-muted/50">
        <div className="flex items-center justify-between mb-1">
          <span className="font-mono font-medium text-xs">{inv.command}</span>
          <div className="flex items-center gap-2">
            {isPendingWrite && (
              <>
                <Button size="sm" variant="default" onClick={() => onApprove(inv.id)}>
                  {t("doctor.execute")}
                </Button>
                <Button size="sm" variant="outline" onClick={() => onReject(inv.id)}>
                  {t("doctor.skip")}
                </Button>
              </>
            )}
            {isPendingRead && (
              <Button size="sm" variant="outline" onClick={() => onApprove(inv.id)}>
                {t("doctor.allowRead")}
              </Button>
            )}
            {statusBadge}
          </div>
        </div>
        {inv.args && Object.keys(inv.args).length > 0 && (
          <pre className="text-xs text-muted-foreground bg-muted rounded p-2 mt-1 overflow-auto max-h-24">
            {JSON.stringify(inv.args, null, 2)}
          </pre>
        )}
      </div>
    );
  }

  if (message.role === "tool-result") {
    return (
      <div className="rounded-md text-sm border-l-[3px] border-l-border border border-border bg-[oklch(0.95_0_0)] dark:bg-muted/30">
        <button
          className="w-full text-left px-3 py-2 text-xs text-muted-foreground hover:text-foreground"
          onClick={() => setExpanded(!expanded)}
        >
          {expanded ? t("doctor.collapse") : t("doctor.details")}
        </button>
        {expanded && (
          <pre className="px-3 pb-2 text-xs font-mono overflow-auto max-h-48 whitespace-pre-wrap break-all">
            {message.content}
          </pre>
        )}
      </div>
    );
  }

  return null;
}
