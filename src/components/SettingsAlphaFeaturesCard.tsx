import { useTranslation } from "react-i18next";

import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { DisclosureCard } from "@/components/DisclosureCard";

interface SettingsAlphaFeaturesCardProps {
  showSshTransferSpeedUi: boolean;
  onSshTransferSpeedUiToggle: (checked: boolean) => void;
}

export function SettingsAlphaFeaturesCard({
  showSshTransferSpeedUi,
  onSshTransferSpeedUiToggle,
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
    </DisclosureCard>
  );
}
