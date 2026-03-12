import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { RecipeSourceEditor } from "@/components/RecipeSourceEditor";
import { RecipeValidationPanel } from "@/components/RecipeValidationPanel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import type { RecipeEditorOrigin, RecipeSourceDiagnostics } from "@/lib/types";
import { useApi } from "@/lib/use-api";

const EMPTY_DIAGNOSTICS: RecipeSourceDiagnostics = {
  errors: [],
  warnings: [],
};

function originLabelKey(origin: RecipeEditorOrigin): string {
  switch (origin) {
    case "workspace":
      return "recipeStudio.originWorkspace";
    case "external":
      return "recipeStudio.originExternal";
    default:
      return "recipeStudio.originBuiltin";
  }
}

function describeDirtyState(
  dirty: boolean,
): "recipeStudio.dirty" | "recipeStudio.saved" {
  return dirty ? "recipeStudio.dirty" : "recipeStudio.saved";
}

export function RecipeStudio({
  recipeId,
  recipeName,
  initialSource,
  origin,
  onBack,
}: {
  recipeId: string;
  recipeName: string;
  initialSource: string;
  origin: RecipeEditorOrigin;
  onBack: () => void;
}) {
  const { t } = useTranslation();
  const ua = useApi();
  const [source, setSource] = useState(initialSource);
  const [diagnostics, setDiagnostics] = useState<RecipeSourceDiagnostics>(EMPTY_DIAGNOSTICS);
  const [validating, setValidating] = useState(false);
  const [validationError, setValidationError] = useState<string | null>(null);

  useEffect(() => {
    setSource(initialSource);
  }, [initialSource, recipeId]);

  useEffect(() => {
    let cancelled = false;
    if (!source.trim()) {
      setDiagnostics(EMPTY_DIAGNOSTICS);
      setValidationError(null);
      setValidating(false);
      return () => {
        cancelled = true;
      };
    }

    setValidating(true);
    void ua.validateRecipeSourceText(source)
      .then((nextDiagnostics) => {
        if (cancelled) return;
        setDiagnostics(nextDiagnostics);
        setValidationError(null);
      })
      .catch((error) => {
        if (cancelled) return;
        setDiagnostics(EMPTY_DIAGNOSTICS);
        setValidationError(error instanceof Error ? error.message : String(error));
      })
      .finally(() => {
        if (!cancelled) {
          setValidating(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [source, ua]);

  const readOnly = origin === "builtin";
  const dirty = source !== initialSource;
  const summaryBadgeKey = useMemo(() => describeDirtyState(dirty), [dirty]);

  return (
    <section className="space-y-4">
      <div className="flex items-start justify-between gap-3 flex-wrap">
        <div className="space-y-1">
          <h2 className="text-2xl font-bold">{t("recipeStudio.title")}</h2>
          <p className="text-sm text-muted-foreground">
            {recipeName} · {recipeId}
          </p>
        </div>
        <div className="flex items-center gap-2 flex-wrap">
          <Badge variant="outline">{t(originLabelKey(origin))}</Badge>
          <Badge variant={readOnly ? "secondary" : "default"}>
            {t(readOnly ? "recipeStudio.readOnly" : "recipeStudio.editable")}
          </Badge>
          {!readOnly && (
            <Badge variant={dirty ? "default" : "outline"}>
              {t(summaryBadgeKey)}
            </Badge>
          )}
          <Button variant="outline" onClick={onBack}>
            {t("recipeStudio.back")}
          </Button>
        </div>
      </div>

      <Card className="border-dashed bg-muted/10">
        <CardContent className="flex items-center justify-between gap-3 flex-wrap py-4">
          <div>
            <div className="text-sm font-medium">{t("recipeStudio.sourceSummaryTitle")}</div>
            <p className="text-sm text-muted-foreground">
              {t("recipeStudio.sourceSummaryBody")}
            </p>
          </div>
          <Badge variant="outline">{t(originLabelKey(origin))}</Badge>
        </CardContent>
      </Card>

      <div className="grid gap-4 xl:grid-cols-[minmax(0,1.35fr)_minmax(20rem,0.85fr)]">
        <RecipeSourceEditor
          value={source}
          readOnly={readOnly}
          origin={origin}
          onChange={setSource}
        />
        <RecipeValidationPanel
          diagnostics={diagnostics}
          validating={validating}
          errorMessage={validationError}
        />
      </div>
    </section>
  );
}
