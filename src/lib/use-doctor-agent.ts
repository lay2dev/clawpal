import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import type { DoctorChatMessage, DoctorInvoke } from "./types";

let msgCounter = 0;
function nextMsgId(): string {
  return `dm-${++msgCounter}`;
}

function extractApprovalPattern(invoke: DoctorInvoke): string {
  const path = (invoke.args?.path as string) ?? "";
  const prefix = path.includes("/") ? path.substring(0, path.lastIndexOf("/") + 1) : path;
  return `${invoke.command}:${prefix}`;
}

export function useDoctorAgent() {
  const [connected, setConnected] = useState(false);
  const [bridgeConnected, setBridgeConnected] = useState(false);
  const [messages, setMessages] = useState<DoctorChatMessage[]>([]);
  const [pendingInvokes, setPendingInvokes] = useState<Map<string, DoctorInvoke>>(new Map());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [target, setTarget] = useState("local");
  const [approvedPatterns, setApprovedPatterns] = useState<Set<string>>(new Set());

  // Track streaming assistant message
  const streamingRef = useRef("");

  // Refs to avoid stale closures in useEffect listeners
  const approvedPatternsRef = useRef(approvedPatterns);
  useEffect(() => { approvedPatternsRef.current = approvedPatterns; }, [approvedPatterns]);
  const autoApproveRef = useRef<(invokeId: string) => Promise<void>>(null!);


  useEffect(() => {
    const unlisten = [
      listen("doctor:connected", () => {
        setConnected(true);
        setError(null);
      }),
      listen<{ reason: string }>("doctor:disconnected", (e) => {
        setConnected(false);
        setLoading(false);
        if (e.payload.reason && e.payload.reason !== "server closed") {
          setError(e.payload.reason);
        }
      }),
      listen<{ text: string }>("doctor:chat-delta", (e) => {
        streamingRef.current += e.payload.text;
        const text = streamingRef.current;
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (last?.role === "assistant" && !last.invoke) {
            return [...prev.slice(0, -1), { ...last, content: text }];
          }
          return [...prev, { id: nextMsgId(), role: "assistant", content: text }];
        });
      }),
      listen<{ text: string }>("doctor:chat-final", (e) => {
        const text = e.payload.text || streamingRef.current;
        streamingRef.current = "";
        setLoading(false);
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (last?.role === "assistant" && !last.invoke) {
            return [...prev.slice(0, -1), { ...last, content: text }];
          }
          return [...prev, { id: nextMsgId(), role: "assistant", content: text }];
        });
      }),
      listen<DoctorInvoke>("doctor:invoke", (e) => {
        const invoke = e.payload;
        setPendingInvokes((prev) => new Map(prev).set(invoke.id, invoke));
        setMessages((prev) => [
          ...prev,
          { id: nextMsgId(), role: "tool-call", content: invoke.command, invoke, status: "pending" },
        ]);

        // Auto-approve read commands if pattern already approved
        if (invoke.type === "read") {
          const pattern = extractApprovalPattern(invoke);
          if (approvedPatternsRef.current.has(pattern)) {
            autoApproveRef.current(invoke.id);
          }
          // else: show in chat, wait for user to click Allow
        }
      }),
      listen<{ id: string; result: unknown }>("doctor:invoke-result", (e) => {
        const { id, result } = e.payload;
        setPendingInvokes((prev) => {
          const next = new Map(prev);
          next.delete(id);
          return next;
        });
        // Append tool-result message (status already set by autoApprove/approveInvoke)
        setMessages((prev) => [
          ...prev,
          { id: nextMsgId(), role: "tool-result" as const, content: JSON.stringify(result, null, 2), invokeResult: result },
        ]);
        // Reset streaming for next assistant message
        streamingRef.current = "";
      }),
      listen("doctor:bridge-connected", () => {
        setBridgeConnected(true);
      }),
      listen<{ reason: string }>("doctor:bridge-disconnected", () => {
        setBridgeConnected(false);
      }),
      listen<{ message: string }>("doctor:error", (e) => {
        setError(e.payload.message);
        setLoading(false);
      }),
    ];

    return () => {
      unlisten.forEach((p) => p.then((f) => f()));
    };
  }, []);

  const autoApprove = useCallback(async (invokeId: string) => {
    try {
      await api.doctorApproveInvoke(invokeId, target);
      setMessages((prev) =>
        prev.map((m) => {
          if (m.invoke?.id === invokeId && m.role === "tool-call") {
            const pattern = extractApprovalPattern(m.invoke);
            setApprovedPatterns((p) => new Set(p).add(pattern));
            return { ...m, status: "auto" as const };
          }
          return m;
        })
      );
    } catch (err) {
      setError(`Auto-approve failed: ${err}`);
    }
  }, [target]);
  autoApproveRef.current = autoApprove;

  const connect = useCallback(async (url: string) => {
    setError(null);
    try {
      // Extract host from WebSocket URL for bridge TCP connection
      const wsUrl = new URL(url);
      const bridgeAddr = `${wsUrl.hostname}:18790`;

      // Connect bridge first (registers as node)
      await api.doctorBridgeConnect(bridgeAddr);

      // Then connect operator (for agent method)
      await api.doctorConnect(url);
    } catch (err) {
      const msg = `Connection failed: ${err}`;
      setError(msg);
      throw new Error(msg);
    }
  }, []);

  const disconnect = useCallback(async () => {
    try {
      await api.doctorDisconnect(); // Tauri command now closes both
    } catch (err) {
      setError(`Disconnect failed: ${err}`);
    }
    setConnected(false);
    setBridgeConnected(false);
    setLoading(false);
  }, []);

  const startDiagnosis = useCallback(async (context: string) => {
    setLoading(true);
    setMessages([]);
    streamingRef.current = "";
    try {
      await api.doctorStartDiagnosis(context);
    } catch (err) {
      setError(`Start diagnosis failed: ${err}`);
      setLoading(false);
    }
  }, []);

  const sendMessage = useCallback(async (message: string) => {
    setLoading(true);
    streamingRef.current = "";
    setMessages((prev) => [...prev, { id: nextMsgId(), role: "user", content: message }]);
    try {
      await api.doctorSendMessage(message);
    } catch (err) {
      setError(`Send message failed: ${err}`);
      setLoading(false);
    }
  }, []);

  const approveInvoke = useCallback(async (invokeId: string) => {
    setMessages((prev) =>
      prev.map((m) => {
        if (m.invoke?.id === invokeId && m.role === "tool-call") {
          if (m.invoke) {
            const pattern = extractApprovalPattern(m.invoke);
            setApprovedPatterns((p) => new Set(p).add(pattern));
          }
          return { ...m, status: "approved" as const };
        }
        return m;
      })
    );
    try {
      await api.doctorApproveInvoke(invokeId, target);
    } catch (err) {
      setError(`Approve failed: ${err}`);
    }
  }, [target]);

  const rejectInvoke = useCallback(async (invokeId: string, reason = "User rejected") => {
    setPendingInvokes((prev) => {
      const next = new Map(prev);
      next.delete(invokeId);
      return next;
    });
    setMessages((prev) =>
      prev.map((m) =>
        m.invoke?.id === invokeId && m.role === "tool-call"
          ? { ...m, status: "rejected" as const }
          : m
      )
    );
    try {
      await api.doctorRejectInvoke(invokeId, reason);
    } catch (err) {
      setError(`Reject failed: ${err}`);
    }
  }, []);

  const reset = useCallback(() => {
    setMessages([]);
    setPendingInvokes(new Map());
    setLoading(false);
    setError(null);
    setBridgeConnected(false);
    setApprovedPatterns(new Set());
    streamingRef.current = "";
  }, []);

  return {
    connected,
    bridgeConnected,
    messages,
    pendingInvokes,
    loading,
    error,
    target,
    setTarget,
    approvedPatterns,
    connect,
    disconnect,
    startDiagnosis,
    sendMessage,
    approveInvoke,
    rejectInvoke,
    reset,
  };
}
