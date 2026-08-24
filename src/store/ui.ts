import { create } from "zustand";
import type { RepoSort, SearchMode } from "@/types";

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
  setLanguage: (v: string | null) => void;
  setTopic: (v: string | null) => void;
  setCategoryId: (v: number | null) => void;
  clearFilters: () => void;

  selectedRepoId: number | null;
  setSelectedRepoId: (id: number | null) => void;
};

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
  setHideUnstarred: (hideUnstarred) => set({ hideUnstarred }),
  setHideArchived: (hideArchived) => set({ hideArchived }),

  language: null,
  topic: null,
  categoryId: null,
  setLanguage: (language) => set({ language }),
  setTopic: (topic) => set({ topic }),
  setCategoryId: (categoryId) => set({ categoryId }),
  clearFilters: () => set({ language: null, topic: null, categoryId: null }),

  selectedRepoId: null,
  setSelectedRepoId: (selectedRepoId) => set({ selectedRepoId }),
}));
