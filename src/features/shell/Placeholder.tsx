import { EmptyState } from "@/components/ui";
import { ALL_NAV_ITEMS } from "./nav";
import type { ViewId } from "@/stores/ui";

export function Placeholder({ view }: { view: ViewId }) {
  const item = ALL_NAV_ITEMS.find((i) => i.id === view);
  return (
    <EmptyState title={`${item?.label ?? view} · not available yet`}>
      This area is on the roadmap and intentionally shows no sample data.
    </EmptyState>
  );
}
