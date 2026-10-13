import { Overview } from "@/features/overview/Overview";
import { Placeholder } from "@/features/shell/Placeholder";
import { useUi } from "@/stores/ui";

export function ViewRouter() {
  const view = useUi((s) => s.view);
  switch (view) {
    case "overview":
      return <Overview />;
    default:
      return <Placeholder view={view} />;
  }
}
