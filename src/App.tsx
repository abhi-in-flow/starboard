import { Sidebar } from "@/components/Sidebar";
import { SyncHeader } from "@/components/SyncHeader";
import { useUiStore } from "@/store/ui";
import { CategoriesView } from "@/views/CategoriesView";
import { InsightsView } from "@/views/InsightsView";
import { LibraryView } from "@/views/LibraryView";
import { SettingsView } from "@/views/SettingsView";

function App() {
  const view = useUiStore((s) => s.view);

  return (
    <div className="flex min-h-screen bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <SyncHeader />
        <main className="min-w-0 flex-1 overflow-auto">
          {view === "library" ? <LibraryView /> : null}
          {view === "categories" ? <CategoriesView /> : null}
          {view === "insights" ? <InsightsView /> : null}
          {view === "settings" ? <SettingsView /> : null}
        </main>
      </div>
    </div>
  );
}

export default App;
