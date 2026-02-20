import { useTranslation } from "react-i18next";
import type { Recipe } from "../lib/types";
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

export function RecipeCard({
  recipe,
  onCook,
  compact,
}: {
  recipe: Recipe;
  onCook: (id: string) => void;
  compact?: boolean;
}) {
  const { t } = useTranslation();

  if (compact) {
    return (
      <Card
        className="cursor-pointer hover:border-primary/50 transition-colors"
        onClick={() => onCook(recipe.id)}
      >
        <CardContent>
          <strong>{recipe.name}</strong>
          <div className="text-sm text-muted-foreground mt-1.5">
            {recipe.description}
          </div>
          <div className="text-xs text-muted-foreground mt-2">
            {t('recipeCard.steps', { count: recipe.steps.length })} &middot; {recipe.difficulty}
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{recipe.name}</CardTitle>
        <CardDescription>{recipe.description}</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex flex-wrap gap-1.5 mb-2">
          {recipe.tags.map((tag) => (
            <Badge key={tag} variant="secondary" className="bg-muted-foreground/15">
              {tag}
            </Badge>
          ))}
        </div>
        <p className="text-sm text-muted-foreground">
          {t('recipeCard.steps', { count: recipe.steps.length })} &middot; {recipe.difficulty}
        </p>
      </CardContent>
      <CardFooter>
        <Button onClick={() => onCook(recipe.id)}>
          {t('recipeCard.cook')}
        </Button>
      </CardFooter>
    </Card>
  );
}
