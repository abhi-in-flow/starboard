import { githubConnectionLabel, ollamaShortLabel } from "@/lib/statusCopy";

export type HealthTone = "ok" | "warn" | "error" | "idle";

export type HealthTarget =
  | {
      kind: "settings";
      tab: "general" | "status";
      section?: "github" | "ollama";
    }
  | { kind: "library" }
  | { kind: "categories"; section?: "taxonomy" | "assign" };

export type HealthItem = {
  id: "github" | "sync" | "readme" | "ollama" | "embeddings" | "categories";
  label: string;
  value: string;
  tone: HealthTone;
  target: HealthTarget;
};

export type HealthInput = {
  githubConnected?: boolean;
  githubUsername?: string | null;
  lastSyncedAt?: string | null;
  syncRunning?: boolean;
  readmeRunning?: boolean;
  pendingReadmes?: number;
  ollamaAvailable?: boolean | null;
  ollamaConfigured?: boolean;
  embeddingCoverage?: number;
  embedRunning?: boolean;
  assignmentCoverage?: number;
  categoryCount?: number;
  repoCount?: number;
};

function pct(value: number | undefined): string {
  if (value == null || !Number.isFinite(value)) {
    return "—";
  }
  return `${Math.round(value * 100)}%`;
}

export function deriveHealthItems(input: HealthInput): HealthItem[] {
  const githubConnected = input.githubConnected;
  const github: HealthItem = {
    id: "github",
    label: "GitHub",
    value: githubConnectionLabel(githubConnected, input.githubUsername),
    tone:
      githubConnected === true
        ? "ok"
        : githubConnected === false
          ? "error"
          : "idle",
    target: { kind: "settings", tab: "general", section: "github" },
  };

  let syncValue = "Never synced";
  let syncTone: HealthTone = "idle";
  if (input.syncRunning) {
    syncValue = "Syncing";
    syncTone = "warn";
  } else if (input.lastSyncedAt) {
    syncValue = "Synced";
    syncTone = "ok";
  } else if (githubConnected) {
    syncValue = "Not synced yet";
    syncTone = "warn";
  }
  const sync: HealthItem = {
    id: "sync",
    label: "Sync",
    value: syncValue,
    tone: syncTone,
    target: { kind: "library" },
  };

  const pending = input.pendingReadmes ?? 0;
  let readmeValue = "Up to date";
  let readmeTone: HealthTone = "ok";
  if (input.readmeRunning) {
    readmeValue = "Fetching";
    readmeTone = "warn";
  } else if (pending > 0) {
    readmeValue = `${pending.toLocaleString()} pending`;
    readmeTone = "warn";
  } else if (!input.lastSyncedAt) {
    readmeValue = "Waiting for sync";
    readmeTone = "idle";
  }
  const readme: HealthItem = {
    id: "readme",
    label: "READMEs",
    value: readmeValue,
    tone: readmeTone,
    target: { kind: "library" },
  };

  const ollama: HealthItem = {
    id: "ollama",
    label: "Ollama",
    value: ollamaShortLabel(input.ollamaAvailable, input.ollamaConfigured),
    tone:
      input.ollamaAvailable === true
        ? "ok"
        : input.ollamaAvailable === false
          ? "warn"
          : "idle",
    target: { kind: "settings", tab: "general", section: "ollama" },
  };

  const coverage = input.embeddingCoverage;
  let embedValue = pct(coverage);
  let embedTone: HealthTone = "idle";
  if (input.embedRunning) {
    embedValue = "Building";
    embedTone = "warn";
  } else if (coverage != null) {
    embedTone = coverage >= 0.9 ? "ok" : coverage > 0 ? "warn" : "idle";
  }
  const embeddings: HealthItem = {
    id: "embeddings",
    label: "Embeddings",
    value: embedValue,
    tone: embedTone,
    target: { kind: "settings", tab: "status" },
  };

  const assigned = input.assignmentCoverage;
  const cats = input.categoryCount ?? 0;
  let catValue = cats === 0 ? "No taxonomy" : pct(assigned);
  let catTone: HealthTone = cats === 0 ? "idle" : "ok";
  if (cats > 0 && assigned != null) {
    catTone = assigned >= 0.8 ? "ok" : "warn";
    catValue = `${pct(assigned)} assigned`;
  }
  const categories: HealthItem = {
    id: "categories",
    label: "Categories",
    value: catValue,
    tone: catTone,
    target:
      cats === 0
        ? { kind: "categories", section: "taxonomy" }
        : { kind: "settings", tab: "status" },
  };

  return [github, sync, readme, ollama, embeddings, categories];
}
