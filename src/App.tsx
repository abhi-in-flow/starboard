import { Sidebar } from "@/components/Sidebar";
import { useUiStore } from "@/store/ui";
import { InsightsView } from "@/views/InsightsView";
import { LibraryView } from "@/views/LibraryView";
import { SettingsView } from "@/views/SettingsView";

function App() {
  const view = useUiStore((s) => s.view);

  return (
    <div className="flex min-h-screen bg-background text-foreground">
      <Sidebar />
      <main className="min-w-0 flex-1 overflow-auto">
        {view === "library" ? <LibraryView /> : null}
        {view === "insights" ? <InsightsView /> : null}
        {view === "settings" ? <SettingsView /> : null}
      </main>
    </div>
  );
}

export default App;
