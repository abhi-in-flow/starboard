import { lazy, Suspense } from "react";
import { Sidebar } from "@/components/Sidebar";
import { SyncHeader } from "@/components/SyncHeader";
import { useUiStore } from "@/store/ui";
import { LibraryView } from "@/views/LibraryView";

const CategoriesView = lazy(() =>
  import("@/views/CategoriesView").then((m) => ({ default: m.CategoriesView })),
);
const InsightsView = lazy(() =>
  import("@/views/InsightsView").then((m) => ({ default: m.InsightsView })),
);
const SettingsView = lazy(() =>
  import("@/views/SettingsView").then((m) => ({ default: m.SettingsView })),
);

function ViewFallback() {
  return (
    <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">
      Loading…
    </div>
  );
}

function App() {
  const view = useUiStore((s) => s.view);

  return (
    <div className="flex h-svh min-h-0 bg-background text-foreground">
      <Sidebar />
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <SyncHeader />
        <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          {view === "library" ? <LibraryView /> : null}
          {view !== "library" ? (
            <div className="min-h-0 flex-1 overflow-auto">
              <Suspense fallback={<ViewFallback />}>
                {view === "categories" ? <CategoriesView /> : null}
                {view === "insights" ? <InsightsView /> : null}
                {view === "settings" ? <SettingsView /> : null}
              </Suspense>
            </div>
          ) : null}
        </main>
      </div>
    </div>
  );
}

export default App;
