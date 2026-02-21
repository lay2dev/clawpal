import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "@/lib/use-api";
import { useInstance } from "@/lib/instance-context";
import { DiffViewer } from "./DiffViewer";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { cn } from "@/lib/utils";
import type { PendingCommand, PreviewQueueResult } from "@/lib/types";

interface PendingChangesBarProps {
  onApplied?: () => void;
  showToast: (message: string, type?: "success" | "error") => void;
}

export function PendingChangesBar({ onApplied, showToast }: PendingChangesBarProps) {
  const { t } = useTranslation();
  const api = useApi();
  const { isConnected } = useInstance();

  const [count, setCount] = useState(0);
  const [expanded, setExpanded] = useState(false);
  const [commands, setCommands] = useState<PendingCommand[]>([]);

  // Preview dialog
  const [showPreview, setShowPreview] = useState(false);
  const [preview, setPreview] = useState<PreviewQueueResult | null>(null);
  const [previewing, setPreviewing] = useState(false);

  // Apply state
  const [applying, setApplying] = useState(false);
  const [applyError, setApplyError] = useState("");

  // Discard dialog
  const [showDiscard, setShowDiscard] = useState(false);

  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Poll queue count
  const refreshCount = useCallback(() => {
    if (!isConnected) return;
    api.queuedCommandsCount().then(setCount).catch(() => {});
  }, [api, isConnected]);

  useEffect(() => {
    refreshCount();
    pollRef.current = setInterval(refreshCount, 2000);
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [refreshCount]);

  // Fetch command list when expanded
  useEffect(() => {
    if (expanded && count > 0) {
      api.listQueuedCommands().then(setCommands).catch(() => {});
    }
  }, [expanded, count, api]);

  const handleRemove = useCallback(
    (id: string) => {
      api.removeQueuedCommand(id).then((ok) => {
        if (ok) {
          setCommands((prev) => prev.filter((c) => c.id !== id));
          refreshCount();
        }
      });
    },
    [api, refreshCount],
  );

  const handlePreview = useCallback(() => {
    setPreviewing(true);
    setApplyError("");
    api
      .previewQueuedCommands()
      .then((result) => {
        setPreview(result);
        setShowPreview(true);
      })
      .catch((e) => showToast(String(e), "error"))
      .finally(() => setPreviewing(false));
  }, [api, showToast]);

  const handleApply = useCallback(() => {
    setApplying(true);
    setApplyError("");
    api
      .applyQueuedCommands()
      .then((result) => {
        if (result.ok) {
          setShowPreview(false);
          setExpanded(false);
          refreshCount();
          showToast(t("queue.applySuccess"));
          onApplied?.();
        } else {
          setApplyError(
            result.error || t("queue.applyFailed", { error: "unknown" }),
          );
          if (result.rolledBack) {
            setApplyError((prev) => prev + " " + t("queue.rolledBack"));
          }
        }
      })
      .catch((e) => setApplyError(String(e)))
      .finally(() => setApplying(false));
  }, [api, refreshCount, showToast, onApplied, t]);

  const handleDiscard = useCallback(() => {
    api
      .discardQueuedCommands()
      .then(() => {
        setShowDiscard(false);
        setExpanded(false);
        setCommands([]);
        refreshCount();
        showToast(t("queue.discarded"));
      })
      .catch((e) => showToast(t("queue.discardFailed", { error: String(e) }), "error"));
  }, [api, refreshCount, showToast, t]);

  if (count === 0) return null;

  return (
    <>
      <div className="px-2 pb-2 space-y-1.5">
        <Separator className="mb-2" />

        {/* Header row */}
        <button
          className="w-full flex items-center justify-between px-1 text-xs text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
          onClick={() => setExpanded(!expanded)}
        >
          <span>{t("queue.pendingCount", { count })}</span>
          <span className="text-[10px]">{expanded ? "\u25B2" : "\u25BC"}</span>
        </button>

        {/* Expanded command list */}
        {expanded && commands.length > 0 && (
          <div className="max-h-[200px] overflow-y-auto space-y-1 px-1">
            {commands.map((cmd) => (
              <div
                key={cmd.id}
                className="flex items-center justify-between text-xs py-1 px-1.5 rounded bg-muted/50"
              >
                <span className="truncate flex-1 mr-2">{cmd.label}</span>
                <button
                  className="text-muted-foreground hover:text-destructive transition-colors shrink-0"
                  onClick={() => handleRemove(cmd.id)}
                  title={t("queue.remove")}
                >
                  &times;
                </button>
              </div>
            ))}
          </div>
        )}

        {/* Action buttons */}
        <Button
          className="w-full"
          size="sm"
          onClick={handlePreview}
          disabled={previewing}
        >
          {previewing && (
            <span className="mr-1.5 inline-block h-3.5 w-3.5 animate-spin rounded-full border-2 border-current border-t-transparent" />
          )}
          {previewing ? t("queue.previewing") : t("queue.preview")}
        </Button>
        <Button
          className="w-full"
          size="sm"
          variant="outline"
          onClick={() => setShowDiscard(true)}
        >
          {t("config.discard")}
        </Button>
      </div>

      {/* Preview & Apply Dialog */}
      <Dialog open={showPreview} onOpenChange={setShowPreview}>
        <DialogContent className="max-w-3xl">
          <DialogHeader>
            <DialogTitle>{t("queue.previewTitle")}</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">
            {t("queue.previewDescription", { count: preview?.commands.length ?? 0 })}
          </p>
          {preview && (
            <DiffViewer
              oldValue={preview.configBefore}
              newValue={preview.configAfter}
            />
          )}
          {preview?.errors && preview.errors.length > 0 && (
            <div className="space-y-1">
              {preview.errors.map((err, i) => (
                <p key={i} className="text-sm text-destructive">{err}</p>
              ))}
            </div>
          )}
          {applyError && (
            <p className="text-sm text-destructive">{applyError}</p>
          )}
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setShowPreview(false)}
              disabled={applying}
            >
              {t("config.cancel")}
            </Button>
            <Button onClick={handleApply} disabled={applying}>
              {applying ? t("config.applying") : t("queue.applyAndRestart")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Discard Confirmation */}
      <AlertDialog open={showDiscard} onOpenChange={setShowDiscard}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("queue.discardTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("queue.discardDescription", { count })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("config.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              onClick={handleDiscard}
            >
              {t("config.discard")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
