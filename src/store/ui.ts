import { create } from "zustand";

export type AppView = "library" | "insights" | "settings";

type UiState = {
  view: AppView;
  setView: (view: AppView) => void;
};

export const useUiStore = create<UiState>((set) => ({
  view: "library",
  setView: (view) => set({ view }),
}));
