import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import i18n from "../i18n";
import { api } from "./api";
import type { DoctorChatMessage, DoctorInvoke, GatewayCredentials } from "./types";

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
  // True after chat-final received — next delta is a NEW turn, not a continuation.
  // Prevents race: operator WS sends next-turn delta before bridge WS delivers invoke.
  const streamEndedRef = useRef(false);

  // Unique session key per diagnosis — avoids inheriting stale state
  const sessionKeyRef = useRef("");
  const agentIdRef = useRef("main");

  // Gate: only process invokes after startDiagnosis has been called.
  // Prevents stale invokes (replayed by the gateway on node reconnect)
  // from appearing and then being cleared by startDiagnosis.
  const sessionActiveRef = useRef(false);

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
        if (!sessionActiveRef.current) return;
        const text = e.payload.text;
        // Skip empty deltas — the gateway sends them between tool calls
        // and they create empty assistant bubbles that obscure tool-call UI
        if (!text) return;
        // If previous streaming ended (chat-final received), this delta is
        // a NEW agent turn. Don't replace the previous assistant message —
        // the invoke event from the bridge WS may not have arrived yet.
        const isNewTurn = streamEndedRef.current;
        streamEndedRef.current = false;
        streamingRef.current = text;
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (!isNewTurn && last?.role === "assistant" && !last.invoke) {
            return [...prev.slice(0, -1), { ...last, content: text }];
          }
          return [...prev, { id: nextMsgId(), role: "assistant", content: text }];
        });
      }),
      listen<{ text: string }>("doctor:chat-final", (e) => {
        if (!sessionActiveRef.current) return;
        const text = e.payload.text || streamingRef.current;
        streamingRef.current = "";
        // If previous turn already ended, this final is for a NEW turn.
        // Same race-condition guard as the delta handler.
        const isNewTurn = streamEndedRef.current;
        streamEndedRef.current = true;
        // When the agent issues a tool call, the gateway sends a "final"
        // chat event with no message content. Don't create an empty bubble
        // and don't clear loading — we're waiting for tool approval/result.
        if (!text) return;
        setLoading(false);
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (!isNewTurn && last?.role === "assistant" && !last.invoke) {
            return [...prev.slice(0, -1), { ...last, content: text }];
          }
          return [...prev, { id: nextMsgId(), role: "assistant", content: text }];
        });
      }),
      listen<DoctorInvoke>("doctor:invoke", (e) => {
        // Ignore invokes arriving before diagnosis starts. Stale invokes
        // (replayed by gateway during reconnect) are rejected on the Rust side
        // after handshake completes — see bridge_client.rs connect().
        if (!sessionActiveRef.current) return;

        const invoke = e.payload;

        // Deduplicate: gateway may send the same invoke twice
        setPendingInvokes((prev) => {
          if (prev.has(invoke.id)) return prev; // already seen
          return new Map(prev).set(invoke.id, invoke);
        });
        setMessages((prev) => {
          if (prev.some((m) => m.invoke?.id === invoke.id)) return prev; // already shown
          return [
            ...prev,
            { id: nextMsgId(), role: "tool-call", content: invoke.command, invoke, status: "pending" },
          ];
        });

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
          if (!prev.has(id)) return prev; // already handled
          const next = new Map(prev);
          next.delete(id);
          return next;
        });
        // Deduplicate: only append result if we haven't already
        setMessages((prev) => {
          if (prev.some((m) => m.role === "tool-result" && m.invokeId === id)) return prev;
          return [
            ...prev,
            { id: nextMsgId(), role: "tool-result" as const, content: JSON.stringify(result, null, 2), invokeResult: result, invokeId: id },
          ];
        });
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

  const connect = useCallback(async (url: string, credentials?: GatewayCredentials) => {
    setError(null);
    try {
      // Connect operator first (essential — for agent method + chat events)
      await api.doctorConnect(url, credentials);

      // Then connect as node (same URL, different role — for receiving tool calls)
      try {
        await api.doctorBridgeConnect(url, credentials);
      } catch (bridgeErr) {
        console.warn("Node connection failed (operator-only mode):", bridgeErr);
        setError(`Node registration failed: ${bridgeErr}`);
      }
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

  const startDiagnosis = useCallback(async (context: string, agentId = "main") => {
    agentIdRef.current = agentId;
    setLoading(true);
    setMessages([]);
    setPendingInvokes(new Map());
    streamingRef.current = "";
    streamEndedRef.current = false;
    // Fresh session key per diagnosis — no inherited stale state
    sessionKeyRef.current = `agent:${agentId}:clawpal-doctor:${target}:${crypto.randomUUID()}`;
    sessionActiveRef.current = true;
    try {
      const isRemote = target !== "local";
      const lang = i18n.language?.startsWith("zh") ? "Chinese (简体中文)" : "English";
      const executionModel = [
        "EXECUTION MODEL (critical — read carefully):",
        "Architecture: You (agent) → ClawPal (node) → target machine. The remote machine is NOT a node — ClawPal is the node and forwards commands transparently.",
        "system.run is your ONLY tool. Commands execute directly on the target machine via ClawPal.",
        "Every result includes an 'executedOn' field — check it to confirm where the command ran.",
        "If executedOn says 'connection lost', tell the user to reconnect in the Instance tab.",
        "You CAN run commands on the target. Do NOT claim you cannot. Do NOT ask the user to run commands manually.",
        "Do NOT use nodes.run, nodes.status, or any node management API — these will NOT reach the target.",
        "Do NOT use ssh, scp, or any remote access tool. Do NOT suggest node pairing.",
        "gatewayProcessRunning: false on the target does NOT mean you cannot run commands — your connection goes through ClawPal, not the target's gateway.",
        isRemote ? "Do NOT mention the host platform (macOS). Focus only on the target machine." : "",
      ].filter(Boolean).join("\n");
      const prompt = [
        "You are ClawPal's diagnostic agent. Diagnose OpenClaw issues on the target machine.",
        `Respond in ${lang}.`,
        isRemote
          ? `The target is a REMOTE machine (host ID: ${target}).`
          : "The target is the local machine running the OpenClaw gateway.",
        executionModel,
        "Analyze the system context below, then use system.run to gather more info and diagnose. Be concise and actionable.\n",
        context,
      ].join("\n");
      await api.doctorStartDiagnosis(prompt, sessionKeyRef.current, agentId);
    } catch (err) {
      setError(`Start diagnosis failed: ${err}`);
      setLoading(false);
    }
  }, [target]);

  const sendMessage = useCallback(async (message: string) => {
    setLoading(true);
    streamingRef.current = "";
    setMessages((prev) => [...prev, { id: nextMsgId(), role: "user", content: message }]);
    try {
      await api.doctorSendMessage(message, sessionKeyRef.current, agentIdRef.current);
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
    sessionActiveRef.current = false;
    setMessages([]);
    setPendingInvokes(new Map());
    setLoading(false);
    setError(null);
    setBridgeConnected(false);
    setApprovedPatterns(new Set());
    streamingRef.current = "";
    streamEndedRef.current = false;
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
