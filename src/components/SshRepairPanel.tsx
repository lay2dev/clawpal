import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { SshRepairAction } from "@/lib/types";
import { repairActionToLabel } from "@/lib/sshDiagnostic";

type TranslateFn = (
  key: string,
  options?: Record<string, string | number | boolean>,
) => string;

interface SshRepairPanelProps {
  repairPlan: SshRepairAction[];
  translate: TranslateFn;
}

export function SshRepairPanel({ repairPlan, translate }: SshRepairPanelProps) {
  if (!repairPlan.length) {
    return null;
  }

  return (
    <Card className="bg-[oklch(0.96_0_0)] dark:bg-muted/50">
      <CardHeader className="pb-2">
        <CardTitle className="text-sm">{translate("ssh.repairTitle")}</CardTitle>
      </CardHeader>
      <CardContent>
        <ul className="space-y-1.5">
          {repairPlan.map((action) => (
            <li key={action} className="text-sm text-muted-foreground flex gap-2">
              <span className="text-muted-foreground/60">•</span>
              <span>{repairActionToLabel(action, translate)}</span>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
