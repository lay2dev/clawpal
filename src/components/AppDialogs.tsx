import { Suspense, lazy } from "react";
import { useTranslation } from "react-i18next";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { SshHost } from "../lib/types";

const SshFormWidget = lazy(() => import("./SshFormWidget").then((m) => ({ default: m.SshFormWidget })));

interface PassphraseDialogProps {
  open: boolean;
  hostLabel: string;
  input: string;
  onInputChange: (value: string) => void;
  onClose: (value: string | null) => void;
}

export function PassphraseDialog({ open, hostLabel, input, onInputChange, onClose }: PassphraseDialogProps) {
  const { t } = useTranslation();
  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) onClose(null); }}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("ssh.passphraseTitle")}</DialogTitle>
        </DialogHeader>
        <div className="space-y-2">
          <p className="text-sm text-muted-foreground">
            {t("ssh.passphrasePrompt", { host: hostLabel })}
          </p>
          <Label htmlFor="ssh-passphrase">{t("ssh.passphraseLabel")}</Label>
          <Input
            id="ssh-passphrase"
            type="password"
            value={input}
            onChange={(e) => onInputChange(e.target.value)}
            placeholder={t("ssh.passphrasePlaceholder")}
            autoFocus
            onKeyDown={(e) => { if (e.key === "Enter") onClose(input); }}
          />
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onClose(null)}>{t("instance.cancel")}</Button>
          <Button onClick={() => onClose(input)}>{t("ssh.passphraseConfirm")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

interface SshEditDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  host: SshHost | null;
  onSave: (host: SshHost) => void;
}

export function SshEditDialog({ open, onOpenChange, host, onSave }: SshEditDialogProps) {
  const { t } = useTranslation();
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("instance.editSsh")}</DialogTitle>
        </DialogHeader>
        {host && (
          <Suspense fallback={<p className="text-sm text-muted-foreground animate-pulse">Loading…</p>}>
            <SshFormWidget
              invokeId="ssh-edit-form"
              defaults={host}
              onSubmit={(_invokeId, h) => onSave({ ...h, id: host.id })}
              onCancel={() => onOpenChange(false)}
            />
          </Suspense>
        )}
      </DialogContent>
    </Dialog>
  );
}
