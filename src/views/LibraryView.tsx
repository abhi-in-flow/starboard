export function LibraryView() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-8 text-center">
      <h1 className="text-2xl font-semibold tracking-tight">Library</h1>
      <p className="max-w-md text-sm text-muted-foreground">
        Your starred repositories will appear here after sync is available in
        Phase 1. Connect a GitHub token in Settings to get started.
      </p>
    </div>
  );
}
