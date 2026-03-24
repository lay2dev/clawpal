import { useTranslation } from "react-i18next";

import { SessionAnalysisPanel } from "@/components/SessionAnalysisPanel";
import { BackupsPanel } from "@/components/BackupsPanel";

export function OpenclawContext() {
  const { t } = useTranslation();

  return (
    <section className="space-y-8">
      <h2 className="text-2xl font-bold">{t("nav.context")}</h2>
      <div>
        <h3 className="text-lg font-semibold mb-3">{t("doctor.sessions")}</h3>
        <SessionAnalysisPanel />
      </div>
      <div>
        <h3 className="text-lg font-semibold mb-3">{t("doctor.backups")}</h3>
        <BackupsPanel />
      </div>
    </section>
  );
}
