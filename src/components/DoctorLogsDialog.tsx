import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { DownloadIcon, RefreshCwIcon } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { getDoctorLogTransport, type DoctorLogSource } from "@/lib/doctor-logs";
import { useApi } from "@/lib/use-api";

interface DoctorLogsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  source: DoctorLogSource;
  onSourceChange: (source: DoctorLogSource) => void;
}

const EMPTY_LOGS: Record<DoctorLogSource, string> = {
  clawpal: "",
  gateway: "",
  helper: "",
};

const EMPTY_ERRORS: Record<DoctorLogSource, string> = {
  clawpal: "",
  gateway: "",
  helper: "",
};

export function DoctorLogsDialog({
  open,
  onOpenChange,
  source,
  onSourceChange,
}: DoctorLogsDialogProps) {
  const { t } = useTranslation();
  const ua = useApi();
  const logsContentRef = useRef<HTMLPreElement>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [logsContent, setLogsContent] = useState<Record<DoctorLogSource, string>>(EMPTY_LOGS);
  const [logsError, setLogsError] = useState<Record<DoctorLogSource, string>>(EMPTY_ERRORS);
  const [logsLoading, setLogsLoading] = useState(false);

  const fetchLog = useCallback((which: DoctorLogSource) => {
    setLogsLoading(true);
    setLogsError((prev) => ({ ...prev, [which]: "" }));
    const transport = getDoctorLogTransport(which);
    const fn =
      transport === "local"
        ? api.readAppLog
        : which === "gateway"
          ? ua.readGatewayLog
          : ua.readHelperLog;
    fn(200)
      .then((text) => {
        setLogsContent((prev) => ({
          ...prev,
          [which]: text.trim() ? text : t("doctor.noLogs"),
        }));
        window.setTimeout(() => {
          if (logsContentRef.current) {
            logsContentRef.current.scrollTop = logsContentRef.current.scrollHeight;
          }
        }, 50);
      })
      .catch((error) => {
        const text = error instanceof Error ? error.message : String(error);
        setLogsContent((prev) => ({ ...prev, [which]: "" }));
        setLogsError((prev) => ({
          ...prev,
          [which]: text || t("doctor.noLogs"),
        }));
      })
      .finally(() => setLogsLoading(false));
  }, [t, ua]);

  useEffect(() => {
    if (!open) return;
    void fetchLog(source);
  }, [fetchLog, open, source]);

  const visibleContent = useMemo(() => {
    const content = logsContent[source] || "";
    const query = searchQuery.trim().toLowerCase();
    if (!query) {
      return content || t("doctor.noLogs");
    }
    const filteredLines = content
      .split("\n")
      .filter((line) => line.toLowerCase().includes(query));
    return filteredLines.length > 0 ? filteredLines.join("\n") : t("doctor.noLogMatches");
  }, [logsContent, searchQuery, source, t]);

  const exportLogs = useCallback(() => {
    try {
      const content = visibleContent || logsError[source] || t("doctor.noLogs");
      const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
      const filename = `clawpal-${source}-${timestamp}.log`;
      const blob = new Blob([content], { type: "text/plain" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.style.display = "none";
      anchor.href = url;
      anchor.download = filename;
      document.body.appendChild(anchor);
      anchor.click();
      window.setTimeout(() => {
        document.body.removeChild(anchor);
        URL.revokeObjectURL(url);
      }, 0);
      toast.success(t("doctor.exportLogsSuccess", { filename }));
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      toast.error(t("doctor.exportLogsFailed", { error: text }));
    }
  }, [logsError, source, t, visibleContent]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-3xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{t("doctor.logs")}</DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <div className="flex items-center gap-2 flex-wrap">
            <Button
              variant={source === "clawpal" ? "default" : "outline"}
              size="sm"
              onClick={() => onSourceChange("clawpal")}
            >
              {t("doctor.clawpalLogs")}
            </Button>
            <Button
              variant={source === "gateway" ? "default" : "outline"}
              size="sm"
              onClick={() => onSourceChange("gateway")}
            >
              {t("doctor.gatewayLogs")}
            </Button>
            <Button
              variant={source === "helper" ? "default" : "outline"}
              size="sm"
              onClick={() => onSourceChange("helper")}
            >
              {t("doctor.helperLogs")}
            </Button>
          </div>

          <div className="flex items-center gap-2 flex-wrap">
            <Input
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder={t("doctor.logsSearchPlaceholder")}
              aria-label={t("doctor.logsSearchPlaceholder")}
              className="h-9 sm:max-w-xs"
            />
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void fetchLog(source)}
              disabled={logsLoading}
            >
              <RefreshCwIcon className={`mr-1.5 size-3.5${logsLoading ? " animate-spin" : ""}`} />
              {t("doctor.refreshLogs")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={exportLogs}
              disabled={logsLoading}
            >
              <DownloadIcon className="mr-1.5 size-3.5" />
              {t("doctor.exportLogs")}
            </Button>
          </div>
        </div>

        {logsError[source] ? (
          <p className="mb-2 text-xs text-destructive">
            {t("doctor.logReadFailed", { error: logsError[source] })}
          </p>
        ) : null}

        <pre
          ref={logsContentRef}
          className="flex-1 min-h-[320px] max-h-[60vh] overflow-auto rounded-md border bg-muted p-3 text-xs font-mono whitespace-pre-wrap break-all"
        >
          {visibleContent}
        </pre>
      </DialogContent>
    </Dialog>
  );
}
