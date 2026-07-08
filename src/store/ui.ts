import { create } from "zustand";
import type { RepoSort } from "@/types";

export type AppView = "library" | "categories" | "insights" | "settings";
export type LibraryLayout = "list" | "grid";

type UiState = {
  view: AppView;
  setView: (view: AppView) => void;

  layout: LibraryLayout;
  setLayout: (layout: LibraryLayout) => void;

  query: string;
  setQuery: (query: string) => void;

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

  layout: "list",
  setLayout: (layout) => set({ layout }),

  query: "",
  setQuery: (query) => set({ query }),

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
