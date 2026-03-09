import type { SshHost } from "@/lib/types";
import { installHubFallbackPromptTemplate, renderPromptTemplate } from "@/lib/prompt-templates";
import i18n from "../i18n";

export function isToolNarration(text: string): boolean {
  const t = text.trim();
  return /^建议执行.*命令[：:]/.test(t)
    || /^原因[：:]/.test(t)
    || /^(Running|Executing|Checking)[：: ]/i.test(t)
    || /^正在(执行|检查|运行)/.test(t);
}

export function extractChoices(text: string): { prose: string; options: Array<{ label: string; value: string }> } | null {
  const lines = text.split("\n");
  const optionLines: Array<{ idx: number; label: string }> = [];
  const listPattern = /^\s*(?:(?:选项|option)\s*\d+\s*[:：]\s*|(?:\*{1,2})?\d+[.)：:]\s*(?:\*{1,2})?\s*|[-•]\s+)\*{0,2}(.+?)\*{0,2}\s*$/i;

  for (let i = 0; i < lines.length; i++) {
    const match = lines[i].match(listPattern);
    if (match) {
      optionLines.push({ idx: i, label: match[1].trim() });
    }
  }

  if (optionLines.length < 2) return null;

  const firstIdx = optionLines[0].idx;
  const lastIdx = optionLines[optionLines.length - 1].idx;
  const blockSize = lastIdx - firstIdx + 1;
  if (blockSize > optionLines.length + 2) return null;

  const isHeaderLine = (line: string) => {
    const normalized = line.trim();
    return normalized.length === 0
      || /[：:]$/.test(normalized)
      || /^请/.test(normalized)
      || /choose|select/i.test(normalized);
  };

  const proseLines = lines.slice(0, firstIdx).filter((line) => !isHeaderLine(line));
  const afterLines = lines.slice(lastIdx + 1).filter((line) => {
    const normalized = line.trim();
    return normalized.length > 0 && !/^请/.test(normalized) && !/please/i.test(normalized);
  });
  const prose = [...proseLines, ...afterLines].join("\n").trim();

  const options = optionLines.map((option) => {
    const dashMatch = option.label.match(/^(.+?)\s*[-—–]+\s+(.+)$/);
    return {
      label: dashMatch ? dashMatch[1].trim() : option.label,
      value: option.label,
    };
  });

  return { prose, options };
}

export function sanitizeSshIdSegment(raw: string): string {
  const lowered = raw.toLowerCase().trim();
  const replaced = lowered.replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "");
  return replaced || "remote";
}

export function buildDefaultSshHostId(host: SshHost): string {
  const base = host.host || host.label || "remote";
  return `ssh:${sanitizeSshIdSegment(base)}`;
}

export function sanitizeLocalIdSegment(raw: string): string {
  const lowered = raw.toLowerCase().trim();
  const replaced = lowered.replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "");
  return replaced || "default";
}

export function resolveInstallPromptLanguage(language: string | null | undefined): string {
  return language?.startsWith("zh") ? "Chinese (简体中文)" : "English";
}

export function renderInstallPrompt(template: string, params: {
  language: string;
  userIntent: string;
}): string {
  return renderPromptTemplate(template, {
    "{{LANGUAGE}}": params.language,
    "{{USER_INTENT}}": params.userIntent,
  });
}

export function buildInstallPrompt(userIntent: string): string {
  return renderInstallPrompt(installHubFallbackPromptTemplate(), {
    language: resolveInstallPromptLanguage(i18n.language),
    userIntent,
  });
}
