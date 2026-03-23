import type { NavItem } from "@/hooks/useNavItems";
import { cn } from "@/lib/utils";

export function SidebarNavButton({ item }: { item: NavItem }) {
  return (
    <button
      type="button"
      className={cn(
        "flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-200",
        item.disabled
          ? item.active
            ? "bg-primary/10 text-primary shadow-sm cursor-not-allowed"
            : "text-muted-foreground cursor-not-allowed"
          : item.active
            ? "bg-primary/10 text-primary shadow-sm cursor-pointer"
            : "text-muted-foreground hover:bg-accent hover:text-accent-foreground cursor-pointer",
      )}
      aria-disabled={item.disabled ? "true" : undefined}
      title={item.tooltip}
      onClick={(event) => {
        if (item.disabled) {
          event.preventDefault();
          return;
        }
        item.onClick();
      }}
    >
      {item.icon}
      <span>{item.label}</span>
      {item.badge}
    </button>
  );
}
