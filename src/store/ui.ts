import { create } from "zustand";
import {
  applyCategoryId,
  applyHideArchived,
  applyHideUnstarred,
  applyReviewPreset,
  clearedLibraryFilters,
  type LibraryFilterState,
} from "@/lib/libraryFilters";
import type { RepoSort, ReviewPreset, SearchMode } from "@/types";

export type AppView = "library" | "categories" | "insights" | "settings";
export type LibraryLayout = "list" | "grid";
export type SettingsTab = "general" | "status";
export type SettingsSection = "github" | "ollama";
export type CategoriesSection = "taxonomy" | "assign";

type UiState = LibraryFilterState & {
  view: AppView;
  setView: (view: AppView) => void;

  settingsTab: SettingsTab;
  setSettingsTab: (tab: SettingsTab) => void;
  settingsSection: SettingsSection | null;
  setSettingsSection: (section: SettingsSection | null) => void;
  categoriesSection: CategoriesSection | null;
  setCategoriesSection: (section: CategoriesSection | null) => void;
  openSettings: (section?: SettingsSection, tab?: SettingsTab) => void;
  openCategories: (section?: CategoriesSection) => void;

  layout: LibraryLayout;
  setLayout: (layout: LibraryLayout) => void;

  query: string;
  setQuery: (query: string) => void;

  searchMode: SearchMode;
  setSearchMode: (mode: SearchMode) => void;
  searchModeInitialized: boolean;
  setSearchModeInitialized: (v: boolean) => void;

  setSort: (sort: RepoSort) => void;
  setSortDesc: (desc: boolean) => void;

  setHideUnstarred: (v: boolean) => void;
  setHideArchived: (v: boolean) => void;

  setLanguage: (v: string | null) => void;
  setTopic: (v: string | null) => void;
  setCategoryId: (v: number | null) => void;
  setReviewPreset: (preset: ReviewPreset | null) => void;
  openLibraryReview: (opts: {
    preset: ReviewPreset;
    categoryId?: number | null;
  }) => void;
  openLibraryCategory: (categoryId: number) => void;
  openLibraryLanguage: (language: string) => void;
  applyFilterPatch: (patch: Partial<LibraryFilterState>) => void;
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
  openSettings: (section, tab) =>
    set({
      view: "settings",
      settingsTab: tab ?? "general",
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

  ...clearedLibraryFilters(),
  setSort: (sort) => set({ sort }),
  setSortDesc: (sortDesc) => set({ sortDesc }),

  setHideUnstarred: (hideUnstarred) =>
    set((s) => applyHideUnstarred(s, hideUnstarred)),
  setHideArchived: (hideArchived) =>
    set((s) => applyHideArchived(s, hideArchived)),

  setLanguage: (language) => set({ language }),
  setTopic: (topic) => set({ topic }),
  setCategoryId: (categoryId) => set((s) => applyCategoryId(s, categoryId)),
  setReviewPreset: (preset) => set((s) => applyReviewPreset(s, preset)),
  openLibraryReview: ({ preset, categoryId }) =>
    set((s) => ({
      view: "library",
      selectedRepoId: null,
      ...applyReviewPreset(s, preset),
      categoryId:
        preset === "uncategorized" ? null : (categoryId ?? s.categoryId),
    })),
  openLibraryCategory: (categoryId) =>
    set((s) => ({
      view: "library",
      selectedRepoId: null,
      ...applyCategoryId(s, categoryId),
    })),
  openLibraryLanguage: (language) =>
    set({
      view: "library",
      selectedRepoId: null,
      language,
    }),
  applyFilterPatch: (patch) => set(patch),
  clearFilters: () => set(clearedLibraryFilters()),

  selectedRepoId: null,
  setSelectedRepoId: (selectedRepoId) => set({ selectedRepoId }),
}));
