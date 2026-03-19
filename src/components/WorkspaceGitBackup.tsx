import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { GitBranch, GitCommit, CloudUpload, AlertCircle, Check } from "lucide-react";

import { hasGuidanceEmitted, useApi } from "@/lib/use-api";
import type { WorkspaceGitStatus } from "@/lib/types";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { AsyncActionButton } from "@/components/ui/AsyncActionButton";
import { Skeleton } from "@/components/ui/skeleton";

export function WorkspaceGitBackup() {
  const { t } = useTranslation();
  const ua = useApi();
  const [status, setStatus] = useState<WorkspaceGitStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState("");
  const [messageType, setMessageType] = useState<"success" | "error" | "info">("info");

  const refresh = useCallback(() => {
    setLoading(true);
    ua.workspaceGitStatus()
      .then((s) => {
        setStatus(s);
        setLoading(false);
      })
      .catch((e) => {
        console.error("Failed to load git status:", e);
        setStatus(null);
        setLoading(false);
      });
  }, [ua]);

  useEffect(() => {
    setStatus(null);
    setMessage("");
    refresh();
  }, [refresh, ua.instanceId, ua.instanceToken, ua.isRemote, ua.isConnected]);

  const showMessage = (text: string, type: "success" | "error" | "info") => {
    setMessage(text);
    setMessageType(type);
  };

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <GitBranch className="h-4 w-4" />
            {t("home.gitBackup")}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Skeleton className="h-16 w-full" />
        </CardContent>
      </Card>
    );
  }

  // Workspace not a git repo — show init button
  if (!status?.isGitRepo) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <GitBranch className="h-4 w-4" />
            {t("home.gitBackup")}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground mb-3">
            {t("home.gitNotInitialized")}
          </p>
          {message && (
            <p className={`text-sm mb-2 ${messageType === "error" ? "text-destructive" : "text-muted-foreground"}`}>
              {message}
            </p>
          )}
          <AsyncActionButton
            size="sm"
            variant="outline"
            loadingText={t("home.creating")}
            onClick={async () => {
              setMessage("");
              try {
                const result = await ua.workspaceGitInit();
                if (result === "already_initialized") {
                  showMessage(t("home.gitAlreadyInitialized"), "info");
                } else {
                  showMessage(t("home.gitInitialized"), "success");
                }
                refresh();
              } catch (e) {
                if (!hasGuidanceEmitted(e)) {
                  showMessage(t("home.gitInitFailed", { error: String(e) }), "error");
                }
              }
            }}
          >
            {t("home.gitInitRepo")}
          </AsyncActionButton>
        </CardContent>
      </Card>
    );
  }

  // Git repo exists — show status + sync button
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <GitBranch className="h-4 w-4" />
          {t("home.gitBackup")}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {/* Status details */}
        <div className="space-y-1 text-sm">
          {status.branch && (
            <div className="flex items-center gap-1.5 text-muted-foreground">
              <GitBranch className="h-3.5 w-3.5" />
              {t("home.gitBranch", { branch: status.branch })}
            </div>
          )}
          {status.hasRemote && status.remoteUrl ? (
            <div className="flex items-center gap-1.5 text-muted-foreground">
              <CloudUpload className="h-3.5 w-3.5" />
              <span className="truncate" title={status.remoteUrl}>
                {t("home.gitRemote", { url: status.remoteUrl })}
              </span>
            </div>
          ) : (
            <div className="flex items-center gap-1.5 text-amber-600 dark:text-amber-400">
              <AlertCircle className="h-3.5 w-3.5" />
              {t("home.gitLocalOnly")}
            </div>
          )}
          {status.uncommittedCount > 0 ? (
            <div className="flex items-center gap-1.5 text-amber-600 dark:text-amber-400">
              <GitCommit className="h-3.5 w-3.5" />
              {t("home.gitUncommitted", { count: status.uncommittedCount })}
            </div>
          ) : (
            <div className="flex items-center gap-1.5 text-green-600 dark:text-green-400">
              <Check className="h-3.5 w-3.5" />
              {t("home.gitClean")}
            </div>
          )}
          {status.ahead > 0 && (
            <div className="text-muted-foreground text-xs">
              {t("home.gitAhead", { count: status.ahead })}
            </div>
          )}
          {status.behind > 0 && (
            <div className="text-amber-600 dark:text-amber-400 text-xs">
              {t("home.gitBehind", { count: status.behind })}
            </div>
          )}
          {status.lastCommitMessage && (
            <div className="text-muted-foreground text-xs truncate" title={status.lastCommitMessage}>
              {t("home.gitLastCommit", { message: status.lastCommitMessage })}
            </div>
          )}
        </div>

        {/* Message */}
        {message && (
          <p className={`text-sm ${
            messageType === "error"
              ? "text-destructive"
              : messageType === "success"
                ? "text-green-600 dark:text-green-400"
                : "text-muted-foreground"
          }`}>
            {message}
          </p>
        )}

        {/* Sync button */}
        <div className="flex gap-2">
          <AsyncActionButton
            size="sm"
            variant={status.uncommittedCount > 0 ? "default" : "outline"}
            loadingText={t("home.gitSyncing")}
            onClick={async () => {
              setMessage("");
              try {
                const result = await ua.workspaceGitBackup();
                if (!result.committed) {
                  showMessage(t("home.gitClean"), "info");
                } else {
                  showMessage(t("home.gitSynced"), "success");
                }
                refresh();
              } catch (e) {
                if (!hasGuidanceEmitted(e)) {
                  showMessage(t("home.gitSyncFailed", { error: String(e) }), "error");
                }
              }
            }}
          >
            {t("home.gitSync")}
          </AsyncActionButton>
          {!status.hasRemote && (
            <p className="text-xs text-muted-foreground self-center">
              {t("home.gitNoRemote")}
            </p>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
