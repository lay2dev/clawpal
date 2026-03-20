import type { Ref } from "react";
import { useTranslation } from "react-i18next";

import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { DisclosureCard } from "@/components/DisclosureCard";

interface SettingsAlphaFeaturesCardProps {
  showSshTransferSpeedUi: boolean;
  remoteDoctorGatewayUrl: string;
  remoteDoctorGatewayAuthToken: string;
  remoteDoctorGatewayUrlInputRef?: Ref<HTMLInputElement>;
  onSshTransferSpeedUiToggle: (checked: boolean) => void;
  onRemoteDoctorGatewayUrlChange: (value: string) => void;
  onRemoteDoctorGatewayUrlSave: () => void;
  onRemoteDoctorGatewayAuthTokenChange: (value: string) => void;
  onRemoteDoctorGatewayAuthTokenSave: () => void;
}

export function SettingsAlphaFeaturesCard({
  showSshTransferSpeedUi,
  remoteDoctorGatewayUrl,
  remoteDoctorGatewayAuthToken,
  remoteDoctorGatewayUrlInputRef,
  onSshTransferSpeedUiToggle,
  onRemoteDoctorGatewayUrlChange,
  onRemoteDoctorGatewayUrlSave,
  onRemoteDoctorGatewayAuthTokenChange,
  onRemoteDoctorGatewayAuthTokenSave,
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
        <Label htmlFor="remote-doctor-gateway-url" className="text-sm font-medium">
          {t("settings.remoteDoctorGatewayUrl")}
        </Label>
        <div className="flex gap-2">
          <Input
            id="remote-doctor-gateway-url"
            ref={remoteDoctorGatewayUrlInputRef}
            value={remoteDoctorGatewayUrl}
            onChange={(event) => onRemoteDoctorGatewayUrlChange(event.target.value)}
            placeholder={t("settings.remoteDoctorGatewayUrlPlaceholder")}
          />
          <button
            type="button"
            onClick={onRemoteDoctorGatewayUrlSave}
            className="inline-flex items-center justify-center rounded-md border px-3 text-sm"
          >
            {t("settings.save")}
          </button>
        </div>
        <p className="text-xs text-muted-foreground">
          {t("settings.remoteDoctorGatewayUrlHint")}
        </p>
      </div>
      <div className="space-y-2">
        <Label htmlFor="remote-doctor-gateway-auth-token" className="text-sm font-medium">
          {t("settings.remoteDoctorGatewayAuthToken")}
        </Label>
        <div className="flex gap-2">
          <Input
            id="remote-doctor-gateway-auth-token"
            type="password"
            value={remoteDoctorGatewayAuthToken}
            onChange={(event) => onRemoteDoctorGatewayAuthTokenChange(event.target.value)}
            placeholder={t("settings.remoteDoctorGatewayAuthTokenPlaceholder")}
          />
          <button
            type="button"
            onClick={onRemoteDoctorGatewayAuthTokenSave}
            className="inline-flex items-center justify-center rounded-md border px-3 text-sm"
          >
            {t("settings.save")}
          </button>
        </div>
        <p className="text-xs text-muted-foreground">
          {t("settings.remoteDoctorGatewayAuthTokenHint")}
        </p>
      </div>
    </DisclosureCard>
  );
}
