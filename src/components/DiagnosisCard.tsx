import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ClipboardCopyIcon, DownloadIcon } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import type { DiagnosisReportItem } from "@/lib/types";

interface DiagnosisCardProps {
  items: DiagnosisReportItem[];
}

const severityConfig = {
  error: { label: "ERROR", variant: "destructive" as const, border: "border-l-destructive" },
  warn: { label: "WARN", variant: "secondary" as const, border: "border-l-yellow-500" },
  info: { label: "INFO", variant: "outline" as const, border: "border-l-blue-500" },
};

function formatMarkdown(items: DiagnosisReportItem[]): string {
  return items
    .map((item, i) => {
      const sev = item.severity.toUpperCase();
      const lines = [`## ${i + 1}. [${sev}] ${item.problem}`];
      if (item.fix_options.length > 0) {
        lines.push("", "**Fix options:**", ...item.fix_options.map((o) => `- ${o}`));
      }
      return lines.join("\n");
    })
    .join("\n\n");
}

function formatJson(items: DiagnosisReportItem[]): string {
  return JSON.stringify(items, null, 2);
}

export function DiagnosisCard({ items }: DiagnosisCardProps) {
  const { t } = useTranslation();
  const [checked, setChecked] = useState<Record<number, boolean>>({});
  const [exportOpen, setExportOpen] = useState(false);
  const [copied, setCopied] = useState(false);

  const toggleCheck = (idx: number) => {
    setChecked((prev) => ({ ...prev, [idx]: !prev[idx] }));
  };

  const handleExport = (format: "markdown" | "json") => {
    const text = format === "json" ? formatJson(items) : formatMarkdown(items);
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
    setExportOpen(false);
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold text-muted-foreground">
          {t("doctor.diagnosisReport", { defaultValue: "Diagnosis Report" })} ({items.length})
        </span>
        <div className="relative">
          <Button
            variant="ghost"
            size="xs"
            onClick={() => setExportOpen(!exportOpen)}
          >
            <DownloadIcon className="size-3.5 mr-1" />
            {copied
              ? t("doctor.copied", { defaultValue: "Copied!" })
              : t("doctor.export", { defaultValue: "Export" })}
          </Button>
          {exportOpen && (
            <div className="absolute right-0 top-full mt-1 z-10 rounded-md border bg-popover p-1 shadow-md min-w-[120px]">
              <button
                className="w-full text-left text-xs px-2 py-1.5 rounded hover:bg-accent"
                onClick={() => handleExport("markdown")}
              >
                <ClipboardCopyIcon className="size-3 inline mr-1.5" />
                Markdown
              </button>
              <button
                className="w-full text-left text-xs px-2 py-1.5 rounded hover:bg-accent"
                onClick={() => handleExport("json")}
              >
                <ClipboardCopyIcon className="size-3 inline mr-1.5" />
                JSON
              </button>
            </div>
          )}
        </div>
      </div>

      {items.map((item, idx) => {
        const cfg = severityConfig[item.severity] ?? severityConfig.info;
        return (
          <Card
            key={idx}
            className={`border-l-[3px] ${cfg.border} bg-[oklch(0.96_0_0)] dark:bg-muted/50 py-3`}
          >
            <CardContent className="px-4 py-0 space-y-2">
              <div className="flex items-start gap-2">
                <Checkbox
                  checked={!!checked[idx]}
                  onCheckedChange={() => toggleCheck(idx)}
                  className="mt-0.5"
                />
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 flex-wrap">
                    <Badge variant={cfg.variant} className="text-[10px] px-1.5 py-0">
                      {cfg.label}
                    </Badge>
                    <span className="text-sm font-medium">{item.problem}</span>
                  </div>
                  {item.fix_options.length > 0 && (
                    <ul className="mt-1.5 space-y-0.5">
                      {item.fix_options.map((opt, oi) => (
                        <li key={oi} className="text-xs text-muted-foreground flex gap-1.5">
                          <span className="text-muted-foreground/60">•</span>
                          {opt}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
                {item.action && (
                  <Button variant="outline" size="xs" className="shrink-0">
                    {t("doctor.autoFix", { defaultValue: "Auto-fix" })}
                  </Button>
                )}
              </div>
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}
