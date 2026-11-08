import {
  Activity,
  BarChart3,
  ClipboardList,
  Bell,
  Bomb,
  ChevronsLeft,
  ChevronsRight,
  Cpu,
  Download,
  Gamepad2,
  Gauge,
  History as HistoryIcon,
  LayoutDashboard,
  ListTree,
  Network as NetworkIcon,
  Settings as SettingsIcon,
  ShieldAlert,
  ShieldCheck,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from "react";
import { Button, cn } from "./components/ui";
import { usePref, useQuery } from "./lib/hooks";
import { api } from "./lib/ipc";
import { startLive, useLive } from "./lib/live";
import { knownUpdate, type Settings } from "./lib/types";
import Alerts from "./pages/Alerts";
import Bsod from "./pages/Bsod";
import Crashes from "./pages/Crashes";
import Gaming from "./pages/Gaming";
import Hardware from "./pages/Hardware";
import NetworkPage from "./pages/Network";
import HistoryPage from "./pages/History";
import Overview from "./pages/Overview";
import Performance from "./pages/Performance";
import Processes from "./pages/Processes";
import SettingsPage from "./pages/Settings";
import Specs from "./pages/Specs";
import Stability from "./pages/Stability";

export type PageId = "overview" | "processes" | "performance" | "hardware" | "gaming" | "stability" | "crashes" | "bsod" | "network" | "history" | "specs" | "alerts" | "settings";

interface NavItem {
  id: PageId;
  label: string;
  icon: typeof Activity;
}

const GROUPS: { label: string; items: NavItem[] }[] = [
  {
    label: "Monitor",
    items: [
      { id: "overview", label: "Overview", icon: LayoutDashboard },
      { id: "processes", label: "Processes", icon: ListTree },
      { id: "performance", label: "Performance", icon: Activity },
      { id: "hardware", label: "Hardware", icon: Cpu },
      { id: "gaming", label: "Gaming", icon: Gamepad2 },
    ],
  },
  {
    label: "Diagnose",
    items: [
      { id: "stability", label: "Stability", icon: ShieldCheck },
      { id: "crashes", label: "Crash Reports", icon: ShieldAlert },
      { id: "bsod", label: "Blue Screen Analyzer", icon: Bomb },
      { id: "network", label: "Network", icon: NetworkIcon },
      { id: "history", label: "History", icon: HistoryIcon },
      { id: "specs", label: "PC Specs", icon: ClipboardList },
      { id: "alerts", label: "Alerts", icon: Bell },
    ],
  },
];

interface Nav {
  page: PageId;
  params: Record<string, string | number>;
  go: (page: PageId, params?: Record<string, string | number>) => void;
  settings: Settings | undefined;
  refreshSettings: () => void;
}

const NavContext = createContext<Nav | null>(null);

export function useNav(): Nav {
  const n = useContext(NavContext);
  if (!n) throw new Error("useNav outside App");
  return n;
}

function applyTheme(theme: Settings["theme"] | undefined) {
  const dark = theme === "dark" || ((theme ?? "system") === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  // Native title bar follows the app theme; null = follow Windows.
  getCurrentWindow()
    .setTheme(theme === "dark" || theme === "light" ? theme : null)
    .catch(() => undefined);
}

function SidebarButton({ item, active, collapsed, onClick, badge }: { item: NavItem; active: boolean; collapsed: boolean; onClick: () => void; badge?: number }) {
  return (
    <button
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      title={collapsed ? item.label : undefined}
      className={cn(
        "relative flex items-center gap-2.5 h-8 rounded-sm text-[12.5px] w-full",
        collapsed ? "justify-center" : "px-2.5",
        active ? "bg-hover text-fg font-medium" : "text-muted hover:text-fg hover:bg-hover/60",
      )}
    >
      {active && <span className="absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded bg-accent" />}
      <item.icon className="size-4 shrink-0" />
      {!collapsed && <span className="truncate">{item.label}</span>}
      {!!badge && <span className={cn("rounded-full bg-warn text-[10px] font-semibold text-black px-1.5 leading-4", collapsed ? "absolute top-0.5 right-0.5" : "ml-auto")}>{badge}</span>}
    </button>
  );
}

export default function App() {
  const [page, setPage] = usePref<PageId>("page", "overview");
  const [params, setParams] = useState<Record<string, string | number>>({});
  const [collapsed, setCollapsed] = usePref("sidebarCollapsed", false);
  const settings = useQuery(() => api.settings(), [], { events: ["settings-changed"] });
  const alerts = useQuery(() => api.activeAlerts(), [], { intervalMs: 5000, events: ["alert"] });
  const hasFrame = useLive((s) => s.frame !== null);
  const update = useQuery(() => api.updateStatus(), [], { events: ["update-status"], intervalMs: 600_000 });
  const [updateDismissed, setUpdateDismissed] = useState<string | null>(null);
  const known = knownUpdate(update.data);

  useEffect(() => {
    startLive().catch(() => undefined);
  }, []);

  useEffect(() => {
    applyTheme(settings.data?.theme);
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => applyTheme(settings.data?.theme);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [settings.data?.theme]);

  const nav = useMemo<Nav>(
    () => ({
      page,
      params,
      go: (p, prm) => {
        setParams(prm ?? {});
        setPage(p);
      },
      settings: settings.data,
      refreshSettings: settings.refresh,
    }),
    [page, params, setPage, settings.data, settings.refresh],
  );

  const pages: Record<PageId, ReactNode> = {
    overview: <Overview />,
    processes: <Processes />,
    performance: <Performance />,
    hardware: <Hardware />,
    gaming: <Gaming />,
    stability: <Stability />,
    crashes: <Crashes />,
    bsod: <Bsod />,
    network: <NetworkPage />,
    history: <HistoryPage />,
    specs: <Specs />,
    alerts: <Alerts />,
    settings: <SettingsPage />,
  };

  return (
    <NavContext.Provider value={nav}>
      <div className="flex h-full">
        <nav aria-label="Main" className={cn("flex flex-col shrink-0 border-r border-line bg-panel transition-[width] duration-150", collapsed ? "w-[52px]" : "w-[208px]")}>
          <div className={cn("flex items-center h-12 shrink-0 gap-2", collapsed ? "justify-center" : "px-3.5")}>
            <Gauge className="size-5 text-accent shrink-0" />
            {!collapsed && <span className="font-semibold text-[13.5px] truncate">Veyl</span>}
          </div>
          <div className="flex-1 overflow-y-auto px-2 pb-2">
            {GROUPS.map((g) => (
              <div key={g.label} className="mt-2">
                {!collapsed && <div className="px-2.5 pb-1 pt-1 text-[10.5px] uppercase tracking-wider text-faint font-semibold">{g.label}</div>}
                {collapsed && <div className="mx-2 my-2 border-t border-line" />}
                <div className="flex flex-col gap-0.5">
                  {g.items.map((it) => (
                    <SidebarButton key={it.id} item={it} active={page === it.id} collapsed={collapsed} onClick={() => nav.go(it.id)} badge={it.id === "alerts" ? alerts.data?.length : undefined} />
                  ))}
                </div>
              </div>
            ))}
          </div>
          <div className="px-2 pb-2 flex flex-col gap-0.5 border-t border-line pt-2">
            <SidebarButton item={{ id: "settings", label: "Settings", icon: SettingsIcon }} active={page === "settings"} collapsed={collapsed} onClick={() => nav.go("settings")} />
            <button
              onClick={() => setCollapsed(!collapsed)}
              className={cn("flex items-center gap-2.5 h-8 rounded-sm text-[12px] text-faint hover:text-fg hover:bg-hover/60", collapsed ? "justify-center" : "px-2.5")}
              aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            >
              {collapsed ? <ChevronsRight className="size-4" /> : <ChevronsLeft className="size-4" />}
              {!collapsed && "Collapse"}
            </button>
          </div>
        </nav>
        <main className="flex-1 min-w-0 flex flex-col overflow-hidden">
          {update.data && known && updateDismissed !== known.version && (
            <div className="px-5 py-1.5 text-[12.5px] border-b border-line bg-accent/8 flex items-center gap-3" role="status">
              <Download className="size-3.5 text-accent" />
              <span>
                Veyl {known.version} is available (you have {update.data.currentVersion}).
              </span>
              {update.data.state === "failed" && <span className="text-bad">{update.data.error}</span>}
              {/* The outcome arrives as the next update state, so errors need no handling here. */}
              <Button variant="primary" className="ml-auto" disabled={update.data.state === "installing"} onClick={() => api.installUpdate().catch(() => undefined)}>
                {update.data.state === "installing" ? "Installing…" : "Install and restart"}
              </Button>
              <Button variant="ghost" onClick={() => setUpdateDismissed(known.version)}>
                Later
              </Button>
            </div>
          )}
          {!hasFrame && page !== "settings" && (
            <div className="px-5 py-1.5 text-[12px] text-info border-b border-line bg-info/5 flex items-center gap-2">
              <BarChart3 className="size-3.5" /> Collecting the first samples…
            </div>
          )}
          <div className="flex-1 min-h-0 flex flex-col">{pages[page]}</div>
        </main>
      </div>
    </NavContext.Provider>
  );
}
