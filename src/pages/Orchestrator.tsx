import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useInstance } from "@/lib/instance-context";
import {
  clearOrchestratorEvents,
  readOrchestratorEvents,
  type OrchestratorEvent,
} from "@/lib/orchestrator-log";

function levelClass(level: OrchestratorEvent["level"]): string {
  if (level === "success") return "bg-emerald-500/10 text-emerald-600";
  if (level === "error") return "bg-red-500/10 text-red-600";
  return "bg-muted text-muted-foreground";
}

export function Orchestrator() {
  const { t } = useTranslation();
  const { instanceId } = useInstance();
  const [events, setEvents] = useState<OrchestratorEvent[]>([]);
  const [scope, setScope] = useState<"current" | "all">("current");

  useEffect(() => {
    const load = () => setEvents(readOrchestratorEvents());
    load();
    const timer = setInterval(load, 1000);
    return () => clearInterval(timer);
  }, []);

  const visible = useMemo(() => {
    const list = scope === "current"
      ? events.filter((e) => e.instanceId === instanceId)
      : events;
    return [...list].sort((a, b) => b.at.localeCompare(a.at));
  }, [events, instanceId, scope]);

  const onClear = () => {
    if (scope === "current") {
      clearOrchestratorEvents(instanceId);
    } else {
      clearOrchestratorEvents();
    }
    setEvents(readOrchestratorEvents());
  };

  return (
    <div>
      <h2 className="text-2xl font-bold mb-4">{t("orchestrator.title")}</h2>
      <p className="text-sm text-muted-foreground mb-4">{t("orchestrator.description")}</p>

      <div className="flex items-center gap-2 mb-4">
        <Button
          size="sm"
          variant={scope === "current" ? "default" : "outline"}
          onClick={() => setScope("current")}
        >
          {t("orchestrator.scope.current")}
        </Button>
        <Button
          size="sm"
          variant={scope === "all" ? "default" : "outline"}
          onClick={() => setScope("all")}
        >
          {t("orchestrator.scope.all")}
        </Button>
        <Button size="sm" variant="outline" onClick={onClear}>
          {t("orchestrator.clear")}
        </Button>
      </div>

      {visible.length === 0 ? (
        <p className="text-muted-foreground">{t("orchestrator.empty")}</p>
      ) : (
        <div className="space-y-3">
          {visible.map((event) => (
            <Card key={event.id}>
              <CardContent className="space-y-1">
                <div className="flex items-center justify-between gap-2">
                  <div className="font-medium">{event.message}</div>
                  <Badge className={levelClass(event.level)}>{event.level}</Badge>
                </div>
                <div className="text-xs text-muted-foreground">
                  {event.at} · {event.instanceId}
                  {event.sessionId ? ` · ${event.sessionId}` : ""}
                  {event.step ? ` · step=${event.step}` : ""}
                  {event.state ? ` · state=${event.state}` : ""}
                  {event.source ? ` · source=${event.source}` : ""}
                </div>
                {event.details && (
                  <div className="text-xs rounded border bg-muted/30 p-2 whitespace-pre-wrap break-all">
                    {event.details}
                  </div>
                )}
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
