import {
  Activity,
  BarChart3,
  Braces,
  Camera,
  Cog,
  Eye,
  FileCode2,
  GitCompare,
  GitFork,
  Layers,
  LayoutDashboard,
  ListTree,
  Network,
  Radar,
  Route,
  Server,
  Table2,
  Workflow,
  type LucideIcon,
} from "lucide-react";
import type { ViewId } from "@/stores/ui";

export interface NavItem {
  id: ViewId;
  label: string;
  icon: LucideIcon;
}

export interface NavGroup {
  title: string;
  items: NavItem[];
}

export const NAV: NavGroup[] = [
  {
    title: "Database",
    items: [
      { id: "overview", label: "Overview", icon: LayoutDashboard },
      { id: "explorer", label: "Explorer", icon: ListTree },
      { id: "tables", label: "Tables", icon: Table2 },
      { id: "views", label: "Views", icon: Eye },
      { id: "indexes", label: "Indexes", icon: Layers },
    ],
  },
  {
    title: "Visualize",
    items: [
      { id: "erd", label: "ER Diagram", icon: Network },
      { id: "dependencies", label: "Dependencies", icon: GitFork },
      { id: "lineage", label: "Data Lineage", icon: Workflow },
    ],
  },
  {
    title: "Analyze",
    items: [
      { id: "impact", label: "Impact Analysis", icon: Radar },
      { id: "health", label: "Health", icon: Activity },
      { id: "index-analysis", label: "Index Analysis", icon: BarChart3 },
      { id: "plans", label: "Query Plans", icon: Route },
    ],
  },
  {
    title: "Versioning",
    items: [
      { id: "snapshots", label: "Snapshots", icon: Camera },
      { id: "diff", label: "Schema Diff", icon: GitCompare },
      { id: "environments", label: "Environments", icon: Server },
    ],
  },
  {
    title: "Developer",
    items: [
      { id: "sql", label: "SQL Editor", icon: FileCode2 },
      { id: "raw", label: "Raw Metadata", icon: Braces },
    ],
  },
  { title: "", items: [{ id: "settings", label: "Settings", icon: Cog }] },
];

export const ALL_NAV_ITEMS: NavItem[] = NAV.flatMap((g) => g.items);
