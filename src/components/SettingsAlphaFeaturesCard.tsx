import type { Ref } from "react";
import { useTranslation } from "react-i18next";

import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { DisclosureCard } from "@/components/DisclosureCard";

interface SettingsAlphaFeaturesCardProps {
  showSshTransferSpeedUi: boolean;
  remoteDoctorInviteCode: string;
  remoteDoctorInviteCodeInputRef?: Ref<HTMLInputElement>;
  onSshTransferSpeedUiToggle: (checked: boolean) => void;
  onRemoteDoctorInviteCodeChange: (value: string) => void;
  onRemoteDoctorInviteCodeSave: () => void;
}

export function SettingsAlphaFeaturesCard({
  showSshTransferSpeedUi,
  remoteDoctorInviteCode,
  remoteDoctorInviteCodeInputRef,
  onSshTransferSpeedUiToggle,
  onRemoteDoctorInviteCodeChange,
  onRemoteDoctorInviteCodeSave,
}: SettingsAlphaFeaturesCardProps) {
  const { t } = useTranslation();

  return (
    <DisclosureCard
      title={t("settings.alphaFeatures")}
      description={t("settings.alphaFeaturesDescription")}
    >
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <Label className="text-sm font-medium">{t("settings.alphaEnableSshTransferSpeedUi")}</Label>
        <Checkbox
          checked={showSshTransferSpeedUi}
          onCheckedChange={(checked) => onSshTransferSpeedUiToggle(checked === true)}
          aria-label={t("settings.alphaEnableSshTransferSpeedUi")}
          className="h-5 w-5"
        />
      </div>
      <p className="text-xs text-muted-foreground">
        {t("settings.alphaEnableSshTransferSpeedUiHint")}
      </p>
      <div className="space-y-2">
        <Label htmlFor="remote-doctor-invite-code" className="text-sm font-medium">
          {t("settings.remoteDoctorInviteCode")}
        </Label>
        <div className="flex gap-2">
          <Input
            id="remote-doctor-invite-code"
            type="password"
            ref={remoteDoctorInviteCodeInputRef}
            value={remoteDoctorInviteCode}
            onChange={(event) => onRemoteDoctorInviteCodeChange(event.target.value)}
            placeholder={t("settings.remoteDoctorInviteCodePlaceholder")}
          />
          <button
            type="button"
            onClick={onRemoteDoctorInviteCodeSave}
            className="inline-flex items-center justify-center rounded-md border px-3 text-sm"
          >
            {t("settings.save")}
          </button>
        </div>
        <p className="text-xs text-muted-foreground">
          {t("settings.remoteDoctorInviteCodeHint")}
        </p>
      </div>
    </DisclosureCard>
  );
}
