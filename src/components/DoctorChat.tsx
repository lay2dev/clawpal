import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
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
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, loading]);

  const handleSend = () => {
    if (!input.trim() || loading || !connected) return;
    onSendMessage(input.trim());
    setInput("");
  };

  return (
    <div className="flex flex-col h-full min-h-[400px]">
      {/* Connection status */}
      <div className="flex items-center gap-2 mb-2 text-xs">
        <span className={cn(
          "inline-block w-2 h-2 rounded-full",
          connected ? "bg-green-500" : "bg-gray-400"
        )} />
        <span className="text-muted-foreground">
          {connected ? t("doctor.connected") : t("doctor.disconnected")}
        </span>
        {error && <span className="text-destructive ml-2">{error}</span>}
      </div>

      {/* Message list */}
      <ScrollArea className="flex-1 mb-2 border rounded-md p-3 bg-muted/30">
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
          <div ref={bottomRef} />
        </div>
      </ScrollArea>

      {/* Input area */}
      <div className="flex gap-2">
        <Input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
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
          {t("chat.send")}
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
      <div className="text-right">
        <div className="inline-block px-3 py-2 rounded-lg max-w-[90%] text-left bg-muted border border-border">
          <div className="whitespace-pre-wrap text-sm">{message.content}</div>
        </div>
      </div>
    );
  }

  if (message.role === "assistant") {
    return (
      <div className="text-left">
        <div className="inline-block px-3 py-2 rounded-lg max-w-[90%] bg-card border border-border">
          <div className="whitespace-pre-wrap text-sm">{message.content}</div>
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
      <div className="border rounded-md p-3 bg-card text-sm">
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
      <div className="border rounded-md bg-muted/50 text-sm">
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
