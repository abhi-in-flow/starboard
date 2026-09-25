import type { SetupStatus } from "@/types";

export type SetupStepId =
  | "github"
  | "sync"
  | "ollama"
  | "taxonomy"
  | "assign"
  | "embeddings";

export type SetupStepState = "complete" | "available" | "blocked";

export type SetupSurface = "full" | "banner" | "hidden";

export type SetupTruth = {
  githubConnected: boolean;
  githubUsername: string | null;
  lastSyncedAt: string | null;
  repoCount: number;
  ollamaAvailable: boolean | null;
  ollamaConfigured: boolean;
  ollamaMessage: string | null;
  categoryCount: number;
  categorizedRepos: number;
  embeddedRepos: number;
  embeddingCoverage: number;
};

export type DerivedSetupStep = {
  id: SetupStepId;
  title: string;
  description: string;
  optional: boolean;
  state: SetupStepState;
  detail: string;
};

/** Map backend snapshot + live Ollama health into checklist rows. */
export function setupTruthFromStatus(
  status: SetupStatus,
  ollama?: { available: boolean; message: string } | null,
): SetupTruth {
  return {
    githubConnected: status.githubConnected,
    githubUsername: status.githubUsername,
    lastSyncedAt: status.lastSyncedAt,
    repoCount: status.repoCount,
    ollamaAvailable: ollama ? ollama.available : null,
    ollamaConfigured: status.ollamaConfigured,
    ollamaMessage: ollama?.message ?? null,
    categoryCount: status.categoryCount,
    categorizedRepos: status.categorizedRepos,
    embeddedRepos: status.embeddedRepos,
    embeddingCoverage: status.embeddingCoverage,
  };
}

export function deriveSetupSteps(truth: SetupTruth): DerivedSetupStep[] {
  const synced = truth.lastSyncedAt != null || truth.repoCount > 0;
  const hasTaxonomy = truth.categoryCount > 0;
  const assigned = truth.categorizedRepos > 0;
  const embedded = truth.embeddedRepos > 0;

  const github: DerivedSetupStep = {
    id: "github",
    title: "Connect GitHub",
    description: "Store a personal access token in the OS keyring.",
    optional: false,
    state: truth.githubConnected ? "complete" : "available",
    detail: truth.githubConnected
      ? truth.githubUsername
        ? `Connected as ${truth.githubUsername}`
        : "Connected"
      : "Paste a personal access token in Settings.",
  };

  const sync: DerivedSetupStep = {
    id: "sync",
    title: "Initial sync",
    description: "Pull starred repositories into the local library.",
    optional: false,
    state: synced
      ? "complete"
      : truth.githubConnected
        ? "available"
        : "blocked",
    detail: synced
      ? `${truth.repoCount.toLocaleString()} repositor${truth.repoCount === 1 ? "y" : "ies"}${
          truth.lastSyncedAt ? " · last sync recorded" : ""
        }`
      : truth.githubConnected
        ? "Pull your starred repositories."
        : "Connect GitHub first.",
  };

  let ollamaState: SetupStepState = "available";
  let ollamaDetail = "Set a chat model in Settings, then check the connection.";
  if (truth.ollamaAvailable === true) {
    ollamaState = "complete";
    ollamaDetail = truth.ollamaMessage || "Ollama is reachable.";
  } else if (truth.ollamaAvailable === false) {
    ollamaDetail = truth.ollamaConfigured
      ? truth.ollamaMessage ||
        "Ollama is unreachable. Keyword search still works."
      : "Optional — configure a base URL and chat model when you want AI features.";
  } else if (truth.ollamaConfigured) {
    ollamaDetail = "Checking Ollama…";
  }

  const ollama: DerivedSetupStep = {
    id: "ollama",
    title: "Configure Ollama",
    description: "Local LLM for taxonomy, assignment, and embeddings.",
    optional: true,
    state: ollamaState,
    detail: ollamaDetail,
  };

  const taxonomy: DerivedSetupStep = {
    id: "taxonomy",
    title: "Generate taxonomy",
    description:
      "Review and commit categories. You can start from a blank draft.",
    optional: true,
    state: hasTaxonomy ? "complete" : "available",
    detail: hasTaxonomy
      ? `${truth.categoryCount.toLocaleString()} categor${
          truth.categoryCount === 1 ? "y" : "ies"
        } committed`
      : "Generate with Ollama or start a blank draft in Categories.",
  };

  const assign: DerivedSetupStep = {
    id: "assign",
    title: "Assign categories",
    description: "Batch-assign repos, or drag a repo onto the library tree.",
    optional: true,
    state: assigned ? "complete" : hasTaxonomy ? "available" : "blocked",
    detail: assigned
      ? `${truth.categorizedRepos.toLocaleString()} of ${truth.repoCount.toLocaleString()} assigned`
      : hasTaxonomy
        ? "Start assignment from Categories, or assign manually in the library."
        : "Commit a taxonomy first.",
  };

  const embeddings: DerivedSetupStep = {
    id: "embeddings",
    title: "Build embeddings",
    description: "Vectors unlock Semantic and Hybrid search.",
    optional: true,
    state: embedded ? "complete" : synced ? "available" : "blocked",
    detail: embedded
      ? `${Math.round(truth.embeddingCoverage * 100)}% coverage (${truth.embeddedRepos.toLocaleString()} repos)`
      : synced
        ? "Build embeddings from the library when Ollama is online."
        : "Sync repositories first.",
  };

  return [github, sync, ollama, taxonomy, assign, embeddings];
}

export function requiredStepsComplete(steps: DerivedSetupStep[]): boolean {
  return steps
    .filter((step) => !step.optional)
    .every((step) => step.state === "complete");
}

/** Optional AI rows never gate dismissal or Keyword-only use. */
export function canDismissOnboarding(): boolean {
  return true;
}

export function onboardingSurface(input: {
  onboardingCompleted: boolean;
  githubConnected: boolean;
  repoCount: number;
}): SetupSurface {
  if (input.onboardingCompleted) {
    return "hidden";
  }
  if (!input.githubConnected || input.repoCount === 0) {
    return "full";
  }
  return "banner";
}

export function nextActionableStep(
  steps: DerivedSetupStep[],
): DerivedSetupStep | undefined {
  return (
    steps.find((step) => !step.optional && step.state !== "complete") ??
    steps.find((step) => step.state === "available")
  );
}
