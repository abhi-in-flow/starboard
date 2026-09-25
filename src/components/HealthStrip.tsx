import { useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  deriveHealthItems,
  type HealthItem,
  type HealthTarget,
  healthTriggerLabel,
} from "@/lib/health";
import {
  getAuthStatus,
  getEmbedStatus,
  getOllamaStatus,
  getSetupStatus,
  getSyncStatus,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/store/ui";

function toneClass(tone: HealthItem["tone"]): string {
  if (tone === "ok") {
    return "bg-emerald-700";
  }
  if (tone === "warn") {
    return "bg-amber-600";
  }
  if (tone === "error") {
    return "bg-destructive";
  }
  return "bg-muted-foreground/50";
}

export function HealthStrip() {
  const setView = useUiStore((s) => s.setView);
  const openSettings = useUiStore((s) => s.openSettings);
  const openCategories = useUiStore((s) => s.openCategories);

  const setup = useQuery({
    queryKey: ["setupStatus"],
    queryFn: getSetupStatus,
  });
  const auth = useQuery({
    queryKey: ["authStatus"],
    queryFn: getAuthStatus,
  });
  const sync = useQuery({
    queryKey: ["syncStatus"],
    queryFn: getSyncStatus,
  });
  const ollama = useQuery({
    queryKey: ["ollamaStatus"],
    queryFn: getOllamaStatus,
  });
  const embed = useQuery({
    queryKey: ["embedStatus"],
    queryFn: getEmbedStatus,
  });

  const items = deriveHealthItems({
    githubConnected: auth.data?.connected ?? setup.data?.githubConnected,
    githubUsername: auth.data?.username ?? setup.data?.githubUsername,
    lastSyncedAt: sync.data?.lastSyncedAt ?? setup.data?.lastSyncedAt,
    syncRunning: sync.data?.running,
    readmeRunning: sync.data?.readmeRunning,
    pendingReadmes: sync.data?.pendingReadmes,
    ollamaAvailable: ollama.data ? ollama.data.available : null,
    ollamaConfigured: setup.data?.ollamaConfigured,
    embeddingCoverage: embed.data?.coverage ?? setup.data?.embeddingCoverage,
    embedRunning: embed.data?.running,
    assignmentCoverage: setup.data?.assignmentCoverage,
    categoryCount: setup.data?.categoryCount,
    repoCount: setup.data?.repoCount,
  });

  function go(target: HealthTarget) {
    if (target.kind === "library") {
      setView("library");
      return;
    }
    if (target.kind === "categories") {
      openCategories(target.section);
      return;
    }
    openSettings(target.section, target.tab);
  }

  return (
    <div className="border-t border-sidebar-border p-2">
      <Popover>
        <PopoverTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-between px-2 py-2 text-left"
            aria-label={healthTriggerLabel(items)}
          >
            <span className="text-xs font-medium">Status</span>
            <span className="flex items-center gap-1" aria-hidden="true">
              {items.map((item) => (
                <span
                  key={item.id}
                  className={cn("size-1.5 rounded-full", toneClass(item.tone))}
                />
              ))}
            </span>
          </Button>
        </PopoverTrigger>
        <PopoverContent align="start" side="right" className="w-64 p-2">
          <p className="px-2 pb-1 text-xs font-medium text-foreground/70">
            Library health
          </p>
          <ul className="flex flex-col">
            {items.map((item) => (
              <li key={item.id}>
                <button
                  type="button"
                  onClick={() => go(item.target)}
                  className="flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left text-xs hover:bg-muted"
                >
                  <span
                    className={cn(
                      "mt-1 size-1.5 shrink-0 rounded-full",
                      toneClass(item.tone),
                    )}
                    aria-hidden="true"
                  />
                  <span className="min-w-0">
                    <span className="block font-medium">{item.label}</span>
                    <span className="block text-foreground/70">
                      {item.value}
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </PopoverContent>
      </Popover>
    </div>
  );
}
