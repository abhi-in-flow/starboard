import { create } from "zustand";
import type { RepoSort, ReviewPreset, SearchMode } from "@/types";

export type AppView = "library" | "categories" | "insights" | "settings";
export type LibraryLayout = "list" | "grid";
export type SettingsTab = "general" | "status";
export type SettingsSection = "github" | "ollama";
export type CategoriesSection = "taxonomy" | "assign";

type UiState = {
  view: AppView;
  setView: (view: AppView) => void;

  settingsTab: SettingsTab;
  setSettingsTab: (tab: SettingsTab) => void;
  settingsSection: SettingsSection | null;
  setSettingsSection: (section: SettingsSection | null) => void;
  categoriesSection: CategoriesSection | null;
  setCategoriesSection: (section: CategoriesSection | null) => void;
  openSettings: (section?: SettingsSection) => void;
  openCategories: (section?: CategoriesSection) => void;

  layout: LibraryLayout;
  setLayout: (layout: LibraryLayout) => void;

  query: string;
  setQuery: (query: string) => void;

  searchMode: SearchMode;
  setSearchMode: (mode: SearchMode) => void;
  searchModeInitialized: boolean;
  setSearchModeInitialized: (v: boolean) => void;

  sort: RepoSort;
  sortDesc: boolean;
  setSort: (sort: RepoSort) => void;
  setSortDesc: (desc: boolean) => void;

  hideUnstarred: boolean;
  hideArchived: boolean;
  setHideUnstarred: (v: boolean) => void;
  setHideArchived: (v: boolean) => void;

  language: string | null;
  topic: string | null;
  categoryId: number | null;
  reviewPreset: ReviewPreset | null;
  setLanguage: (v: string | null) => void;
  setTopic: (v: string | null) => void;
  setCategoryId: (v: number | null) => void;
  setReviewPreset: (preset: ReviewPreset | null) => void;
  openLibraryReview: (opts: {
    preset: ReviewPreset;
    categoryId?: number | null;
  }) => void;
  clearFilters: () => void;

  selectedRepoId: number | null;
  setSelectedRepoId: (id: number | null) => void;
};

function patchForPreset(preset: ReviewPreset | null): Partial<UiState> {
  if (preset == null) {
    return {
      reviewPreset: null,
      sort: "starredAt",
      sortDesc: true,
    };
  }
  const base: Partial<UiState> = {
    reviewPreset: preset,
    sort: "stale",
    sortDesc: false,
  };
  if (preset === "archived") {
    return { ...base, hideArchived: false, hideUnstarred: true };
  }
  if (preset === "unstarred") {
    return { ...base, hideUnstarred: false, hideArchived: false };
  }
  if (preset === "uncategorized") {
    return {
      ...base,
      hideUnstarred: true,
      hideArchived: true,
      categoryId: null,
    };
  }
  return { ...base, hideUnstarred: true };
}

export const useUiStore = create<UiState>((set) => ({
  view: "library",
  setView: (view) => set({ view }),

  settingsTab: "general",
  setSettingsTab: (settingsTab) => set({ settingsTab }),
  settingsSection: null,
  setSettingsSection: (settingsSection) => set({ settingsSection }),
  categoriesSection: null,
  setCategoriesSection: (categoriesSection) => set({ categoriesSection }),
  openSettings: (section) =>
    set({
      view: "settings",
      settingsTab: "general",
      settingsSection: section ?? null,
    }),
  openCategories: (section) =>
    set({
      view: "categories",
      categoriesSection: section ?? null,
    }),

  layout: "list",
  setLayout: (layout) => set({ layout }),

  query: "",
  setQuery: (query) => set({ query }),

  searchMode: "keyword",
  setSearchMode: (searchMode) => set({ searchMode }),
  searchModeInitialized: false,
  setSearchModeInitialized: (searchModeInitialized) =>
    set({ searchModeInitialized }),

  sort: "starredAt",
  sortDesc: true,
  setSort: (sort) => set({ sort }),
  setSortDesc: (sortDesc) => set({ sortDesc }),

  hideUnstarred: true,
  hideArchived: true,
  setHideUnstarred: (hideUnstarred) =>
    set((s) => ({
      hideUnstarred,
      reviewPreset:
        hideUnstarred && s.reviewPreset === "unstarred"
          ? null
          : s.reviewPreset,
    })),
  setHideArchived: (hideArchived) =>
    set((s) => ({
      hideArchived,
      reviewPreset:
        hideArchived && s.reviewPreset === "archived" ? null : s.reviewPreset,
    })),

  language: null,
  topic: null,
  categoryId: null,
  reviewPreset: null,
  setLanguage: (language) => set({ language }),
  setTopic: (topic) => set({ topic }),
  setCategoryId: (categoryId) =>
    set((s) => ({
      categoryId,
      reviewPreset:
        categoryId != null && s.reviewPreset === "uncategorized"
          ? null
          : s.reviewPreset,
    })),
  setReviewPreset: (preset) => set(patchForPreset(preset)),
  openLibraryReview: ({ preset, categoryId }) =>
    set((s) => ({
      view: "library",
      selectedRepoId: null,
      ...patchForPreset(preset),
      categoryId:
        preset === "uncategorized"
          ? null
          : (categoryId ?? s.categoryId),
    })),
  clearFilters: () => set({ language: null, topic: null, categoryId: null }),

  selectedRepoId: null,
  setSelectedRepoId: (selectedRepoId) => set({ selectedRepoId }),
}));
