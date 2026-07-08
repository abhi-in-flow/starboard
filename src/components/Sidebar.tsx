import { BookMarked, LineChart, Settings } from "lucide-react";
import { cn } from "@/lib/utils";
import { type AppView, useUiStore } from "@/store/ui";

const navItems: { id: AppView; label: string; icon: typeof BookMarked }[] = [
  { id: "library", label: "Library", icon: BookMarked },
  { id: "insights", label: "Insights", icon: LineChart },
  { id: "settings", label: "Settings", icon: Settings },
];

export function Sidebar() {
  const view = useUiStore((s) => s.view);
  const setView = useUiStore((s) => s.setView);

  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground">
      <div className="border-b border-sidebar-border px-4 py-5">
        <p className="text-lg font-semibold tracking-tight">Starboard</p>
        <p className="text-xs text-muted-foreground">GitHub stars, organized</p>
      </div>
      <nav className="flex flex-1 flex-col gap-1 p-2">
        {navItems.map((item) => {
          const Icon = item.icon;
          const active = view === item.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => setView(item.id)}
              className={cn(
                "flex items-center gap-2 rounded-md px-3 py-2 text-sm transition-colors",
                active
                  ? "bg-sidebar-accent text-sidebar-accent-foreground"
                  : "hover:bg-sidebar-accent/60",
              )}
            >
              <Icon className="size-4" />
              {item.label}
            </button>
          );
        })}
      </nav>
    </aside>
  );
}
