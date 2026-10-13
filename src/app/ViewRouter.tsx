import { DependencyGraph } from "@/features/dependencies/DependencyGraph";
import { ErdView } from "@/features/erd/ErdView";
import { Explorer } from "@/features/explorer/Explorer";
import { ObjectList } from "@/features/explorer/Lists";
import { HealthView } from "@/features/health/HealthView";
import { IndexAnalysis } from "@/features/health/IndexAnalysis";
import { ImpactView } from "@/features/impact/ImpactView";
import { Overview } from "@/features/overview/Overview";
import { Placeholder } from "@/features/shell/Placeholder";
import { useUi } from "@/stores/ui";

export function ViewRouter() {
  const view = useUi((s) => s.view);
  switch (view) {
    case "overview":
      return <Overview />;
    case "explorer":
      return <Explorer />;
    case "tables":
      return <ObjectList kind="tables" />;
    case "views":
      return <ObjectList kind="views" />;
    case "indexes":
      return <ObjectList kind="indexes" />;
    case "erd":
      return <ErdView />;
    case "dependencies":
      return <DependencyGraph />;
    case "impact":
      return <ImpactView />;
    case "health":
      return <HealthView />;
    case "index-analysis":
      return <IndexAnalysis />;
    default:
      return <Placeholder view={view} />;
  }
}
