import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { EmptyState, ErrorNote } from "@/components/ui";
import { CommandPalette } from "@/features/palette/CommandPalette";
import { ConnectScreen } from "@/features/connect/ConnectScreen";
import { Header } from "@/features/shell/Header";
import { Sidebar } from "@/features/shell/Sidebar";
import { StatusBar } from "@/features/shell/StatusBar";
import { ViewRouter } from "@/app/ViewRouter";
import { useStatus } from "@/hooks/queries";
import type { AppError } from "@/types/schema";

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
});

function Root() {
  const status = useStatus();
  if (status.isLoading) return <EmptyState title="Starting DB-BUTURE…" />;
  if (status.error && !status.data) {
    return (
      <div className="p-6">
        <ErrorNote error={status.error as unknown as AppError} />
      </div>
    );
  }
  if (!status.data) return <ConnectScreen />;
  return (
    <div className="flex h-full">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <Header info={status.data} />
        <main className="bp-grid min-h-0 flex-1 overflow-auto">
          <ViewRouter />
        </main>
        <StatusBar info={status.data} />
      </div>
      <CommandPalette />
    </div>
  );
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <Root />
    </QueryClientProvider>
  );
}
