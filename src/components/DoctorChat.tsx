import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { AgentMessageBubble } from "@/components/AgentMessageBubble";
import { DiagnosisCard } from "@/components/DiagnosisCard";
import { Textarea } from "@/components/ui/textarea";
import type { DoctorChatMessage } from "@/lib/types";

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false });
}

function formatDate(ts: number): string {
  return new Date(ts).toLocaleDateString();
}

function isSameDay(a: number, b: number): boolean {
  const da = new Date(a);
  const db = new Date(b);
  return da.getFullYear() === db.getFullYear() && da.getMonth() === db.getMonth() && da.getDate() === db.getDate();
}

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
        className="h-[420px] mb-2 border rounded-md p-3 bg-muted/30 overflow-y-auto overflow-x-hidden"
      >
        <div className="space-y-3">
          {error && (
            <div className="text-sm text-destructive border border-destructive/30 rounded-md px-3 py-2 bg-destructive/5">
              {error}
            </div>
          )}
          {messages.map((msg, idx) => {
            const ts = msg.timestamp;
            const prevTs = idx > 0 ? messages[idx - 1].timestamp : undefined;
            const showDateSep = ts && (!prevTs || !isSameDay(prevTs, ts));
            return (
              <div key={msg.id}>
                {showDateSep && (
                  <div className="text-center text-[10px] text-muted-foreground/60 py-1">
                    {formatDate(ts)}
                  </div>
                )}
                <div className="relative group">
                  <AgentMessageBubble
                    message={msg}
                    onApprove={onApproveInvoke}
                    onReject={onRejectInvoke}
                  />
                  {ts && (
                    <div className="text-[10px] text-muted-foreground/50 mt-0.5 px-1">
                      {formatTime(ts)}
                    </div>
                  )}
                </div>
                {msg.diagnosisReport && msg.diagnosisReport.items.length > 0 && (
                  <div className="mt-2">
                    <DiagnosisCard items={msg.diagnosisReport.items} />
                  </div>
                )}
              </div>
            );
          })}
          {loading && (
            <div className="text-sm text-muted-foreground animate-pulse">
              {t("doctor.agentThinking")}
            </div>
          )}
        </div>
      </div>

      {/* Input area */}
      <div className="flex items-end gap-2">
        <Textarea
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
          rows={1}
          className="flex-1 min-h-[44px] max-h-32 resize-none"
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
