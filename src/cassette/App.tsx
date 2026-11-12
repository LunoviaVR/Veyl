// The Cassette Futurism console: its own shell, pages and components. Root.tsx mounts it while
// that theme is selected; the standard interface is src/App.tsx.
import "@fontsource/vt323/latin-400.css";
import * as Dialog from "@radix-ui/react-dialog";
import {
  Activity,
  ClipboardList,
  Bell,
  Bomb,
  ChevronsLeft,
  ChevronsRight,
  Cpu,
  Gamepad2,
  History as HistoryIcon,
  LayoutDashboard,
  LifeBuoy,
  ListTree,
  Network as NetworkIcon,
  Settings as SettingsIcon,
  ShieldAlert,
  ShieldCheck,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from "react";
import type { PageId } from "../App";
import { usePref, useQuery } from "../lib/hooks";
import { api } from "../lib/ipc";
import { startLive, useLive } from "../lib/live";
import { knownUpdate, type Settings } from "../lib/types";
import { Button, cn, Lamp, Switch } from "./components/ui";
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
import Recovery from "./pages/Recovery";
import SettingsPage from "./pages/Settings";
import Specs from "./pages/Specs";
import Stability from "./pages/Stability";

interface NavItem {
  id: PageId;
  label: string;
  icon: typeof Activity;
}

// Grouped by what the owner came to do: watch the PC now, find out what went wrong, or use a tool.
const GROUPS: { label: string; items: NavItem[] }[] = [
  {
    label: "Monitor",
    items: [
      { id: "overview", label: "Overview", icon: LayoutDashboard },
      { id: "performance", label: "Performance", icon: Activity },
      { id: "processes", label: "Processes", icon: ListTree },
      { id: "hardware", label: "Hardware", icon: Cpu },
      { id: "gaming", label: "Gaming", icon: Gamepad2 },
    ],
  },
  {
    label: "Diagnose",
    items: [
      { id: "stability", label: "Stability", icon: ShieldCheck },
      { id: "crashes", label: "Crash Reports", icon: ShieldAlert },
      { id: "bsod", label: "Blue Screens", icon: Bomb },
      { id: "alerts", label: "Alerts", icon: Bell },
    ],
  },
  {
    label: "Tools",
    items: [
      { id: "history", label: "History", icon: HistoryIcon },
      { id: "network", label: "Packet Capture", icon: NetworkIcon },
      { id: "specs", label: "PC Specs", icon: ClipboardList },
      { id: "recovery", label: "Recovery", icon: LifeBuoy },
    ],
  },
];

/** Everything the jump box can reach, tabs included; `words` are what people type when they do not know the page name. */
const JUMPS: { label: string; page: PageId; params?: Record<string, string>; words?: string }[] = [
  { label: "Overview", page: "overview", words: "home status health lamps messages" },
  { label: "Performance › CPU", page: "performance", params: { item: "cpu" }, words: "processor cores clock utilization load" },
  { label: "Performance › Memory", page: "performance", params: { item: "memory" }, words: "ram commit hard faults" },
  { label: "Performance › GPU", page: "performance", params: { item: "gpu-0" }, words: "graphics vram video card" },
  { label: "Performance › Network", page: "performance", params: { item: "network" }, words: "throughput adapters ethernet wifi download upload" },
  { label: "Processes", page: "processes", params: { tab: "processes" }, words: "task manager end task kill pid apps" },
  { label: "Processes › Services", page: "processes", params: { tab: "services" }, words: "windows service" },
  { label: "Processes › Startup apps", page: "processes", params: { tab: "startup" }, words: "autostart boot run keys" },
  { label: "Processes › Network connections", page: "processes", params: { tab: "connections" }, words: "tcp udp ports sockets" },
  { label: "Hardware › Sensors", page: "hardware", params: { tab: "sensors" }, words: "temperature fan rpm voltage power pawnio" },
  { label: "Hardware › Storage health", page: "hardware", params: { tab: "storage" }, words: "smart nvme ssd drive disk wear" },
  { label: "Hardware › System", page: "hardware", params: { tab: "system" }, words: "cpu gpu identification windows build" },
  { label: "Gaming", page: "gaming", words: "fps frame time capture overlay presentmon stutter sessions" },
  { label: "Stability", page: "stability", words: "findings timeline events reliability whea" },
  { label: "Crash Reports", page: "crashes", words: "application crash hang exception faulting module" },
  { label: "Blue Screens", page: "bsod", words: "bsod bugcheck stop code minidump dump unexpected shutdown restart" },
  { label: "Alerts", page: "alerts", words: "thresholds rules notifications" },
  { label: "History", page: "history", words: "recorded telemetry charts tape export csv" },
  { label: "Packet Capture", page: "network", words: "network packets wireshark dns tls filter sniff" },
  { label: "PC Specs", page: "specs", words: "hardware list motherboard export" },
  { label: "Recovery", page: "recovery", words: "safe mode boot restart recovery environment winre startup settings repair system restore" },
  { label: "Settings", page: "settings", words: "theme sampling retention updates privacy tray administrator" },
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

/** The chassis is the console's light or dark: gunmetal or putty. Its screens stay dark either way. */
function applyChassis(chassis: "dark" | "putty") {
  const theme = chassis === "dark" ? "dark" : "light";
  document.documentElement.dataset.theme = theme;
  // The native title bar matches the chassis.
  getCurrentWindow()
    .setTheme(theme)
    .catch(() => undefined);
}

/** A latching selector key: in and lit for the current page, amber with a count while alerts are active. */
function NavKey({ item, active, collapsed, onClick, alerts }: { item: NavItem; active: boolean; collapsed: boolean; onClick: () => void; alerts?: number }) {
  return (
    <button
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      title={collapsed ? item.label : undefined}
      className={cn(
        "label relative flex items-center gap-2.5 h-9 w-full rounded-[4px] border text-[13.5px]",
        collapsed ? "justify-center" : "px-2.5",
        active ? "key bg-raised border-line-strong text-fg" : "border-transparent text-muted hover:text-fg hover:bg-raised/60",
      )}
    >
      <Lamp tone={alerts ? "warn" : active ? "ok" : null} className={collapsed ? "absolute top-1 right-1" : undefined} />
      <item.icon className="size-4 shrink-0" />
      {!collapsed && <span className="truncate">{item.label}</span>}
      {!!alerts && !collapsed && (
        <span className="ml-auto text-warn tabular" aria-label={`${alerts} active`}>
          {alerts}
        </span>
      )}
    </button>
  );
}

/** Command line that jumps to any page or tab (Ctrl+K). */
function Jump({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const { go } = useNav();
  const [query, setQuery] = useState("");
  const [at, setAt] = useState(0);
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  const hits = JUMPS.filter((j) => terms.every((t) => `${j.label} ${j.words ?? ""}`.toLowerCase().includes(t)));
  const current = hits[Math.min(at, hits.length - 1)];
  const pick = (j: (typeof JUMPS)[number]) => {
    go(j.page, j.params);
    onOpenChange(false);
  };
  useEffect(() => {
    if (open) {
      setQuery("");
      setAt(0);
    }
  }, [open]);
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/65 z-40" />
        <Dialog.Content aria-describedby={undefined} className="crt pop fixed z-50 left-1/2 top-[16vh] -translate-x-1/2 w-[min(580px,92vw)] flex flex-col">
          <Dialog.Title className="sr-only">Jump to a page</Dialog.Title>
          <label className="readout-title flex items-center gap-2.5 px-4 h-13 border-b border-line">
            <span aria-hidden>&gt;</span>
            <input
              autoFocus
              role="combobox"
              aria-expanded
              aria-controls="jump-list"
              aria-activedescendant={current ? `jump-${JUMPS.indexOf(current)}` : undefined}
              aria-label="Jump to a page or feature"
              placeholder="Jump to a page or feature"
              spellCheck={false}
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setAt(0);
              }}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                  e.preventDefault();
                  setAt((Math.min(at, hits.length - 1) + (e.key === "ArrowDown" ? 1 : hits.length - 1)) % Math.max(hits.length, 1));
                } else if (e.key === "Enter" && current) pick(current);
              }}
              className="flex-1 min-w-0 bg-transparent text-fg placeholder:text-faint focus:outline-none"
            />
          </label>
          <ul id="jump-list" role="listbox" aria-label="Pages" className="max-h-[52vh] overflow-y-auto p-1.5">
            {hits.map((j) => (
              <li
                key={j.label}
                id={`jump-${JUMPS.indexOf(j)}`}
                role="option"
                aria-selected={j === current}
                onMouseMove={() => setAt(hits.indexOf(j))}
                onClick={() => pick(j)}
                className={cn("flex items-center h-9 px-3 rounded-[3px] text-sm cursor-default", j === current && "bg-accent text-accent-fg")}
              >
                {j.label}
              </li>
            ))}
            {!hits.length && <li className="px-3 py-3 text-sm text-muted">Nothing matches “{query}”. Try a feature name, such as “temperature” or “fps”.</li>}
          </ul>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export default function App() {
  const [page, setPage] = usePref<PageId>("page", "overview");
  const [params, setParams] = useState<Record<string, string | number>>({});
  const [collapsed, setCollapsed] = usePref("sidebarCollapsed", false);
  // The console's two switches: screen phosphor and chassis colour.
  const [phosphor, setPhosphor] = usePref<"green" | "amber">("phosphor", "green");
  const [chassis, setChassis] = usePref<"dark" | "putty">("chassis", "dark");
  const [jumpOpen, setJumpOpen] = useState(false);
  const settings = useQuery(() => api.settings(), [], { events: ["settings-changed"] });
  const alerts = useQuery(() => api.activeAlerts(), [], { intervalMs: 5000, events: ["alert"] });
  const hasFrame = useLive((s) => s.frame !== null);
  const update = useQuery(() => api.updateStatus(), [], { events: ["update-status"], intervalMs: 600_000 });
  const [updateDismissed, setUpdateDismissed] = useState<string | null>(null);
  const known = knownUpdate(update.data);

  useEffect(() => {
    startLive().catch(() => undefined);
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setJumpOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => applyChassis(chassis), [chassis]);

  useEffect(() => {
    document.documentElement.dataset.phosphor = phosphor;
  }, [phosphor]);

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
    recovery: <Recovery />,
    alerts: <Alerts />,
    settings: <SettingsPage />,
  };

  return (
    <NavContext.Provider value={nav}>
      <div className="flex h-full">
        <nav aria-label="Main" className={cn("flex flex-col shrink-0 border-r border-line shadow-[1px_0_0_var(--bezel-hi)] transition-[width] duration-150", collapsed ? "w-[58px]" : "w-[232px]")}>
          <div className={cn("flex items-center h-14 shrink-0 gap-2.5", collapsed ? "justify-center" : "px-3.5")}>
            <span className="screen field grid place-items-center size-8 shrink-0 rounded-[5px] text-accent">
              <svg viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <polyline points="2,12.5 7,12.5 9.4,7 12.3,18 14.8,9.4 16.4,12.5 22,12.5" />
              </svg>
            </span>
            {!collapsed && (
              <span className="min-w-0 leading-none">
                <span className="label brand block">Veyl</span>
                <span className="label block mt-1 text-[10.5px] text-faint">PC health monitor</span>
              </span>
            )}
          </div>
          <div className="px-2 pb-1">
            <button
              onClick={() => setJumpOpen(true)}
              title="Jump to a page or feature (Ctrl+K)"
              aria-label="Jump to a page or feature"
              className={cn("screen field flex items-center gap-2 h-8 w-full rounded-[4px] text-sm text-muted hover:text-fg", collapsed ? "justify-center" : "px-2.5")}
            >
              <span aria-hidden>&gt;</span>
              {!collapsed && (
                <>
                  <span className="truncate">Jump to…</span>
                  <span className="ml-auto text-2xs text-faint">Ctrl K</span>
                </>
              )}
            </button>
          </div>
          <div className="flex-1 overflow-y-auto px-2 pb-2">
            {GROUPS.map((g) => (
              <div key={g.label} className="mt-3">
                {collapsed ? <div className="mx-2 mb-2 border-t border-line" /> : <div className="label flex items-center gap-2 px-2.5 pb-1.5 text-[11px] text-faint after:h-px after:flex-1 after:bg-line">{g.label}</div>}
                <div className="flex flex-col gap-1">
                  {g.items.map((it) => (
                    <NavKey key={it.id} item={it} active={page === it.id} collapsed={collapsed} onClick={() => nav.go(it.id)} alerts={it.id === "alerts" ? alerts.data?.length : undefined} />
                  ))}
                </div>
              </div>
            ))}
          </div>
          <div className="flex flex-col gap-1 px-2 pt-2 pb-2 border-t border-line">
            {!collapsed && (
              <>
                <div className="label flex items-center gap-2 h-8 px-2.5 text-[11px] text-faint" title="Screen phosphor: green or amber">
                  <span>Phosphor</span>
                  <span className={cn("ml-auto", phosphor === "green" && "text-fg")}>Grn</span>
                  <Switch plain label="Amber phosphor" checked={phosphor === "amber"} onChange={(amber) => setPhosphor(amber ? "amber" : "green")} />
                  <span className={cn("w-6", phosphor === "amber" && "text-fg")}>Amb</span>
                </div>
                <div className="label flex items-center gap-2 h-8 px-2.5 text-[11px] text-faint" title="Chassis colour: dark gunmetal or putty">
                  <span>Chassis</span>
                  <span className={cn("ml-auto", chassis === "dark" && "text-fg")}>Drk</span>
                  <Switch plain label="Putty chassis" checked={chassis === "putty"} onChange={(putty) => setChassis(putty ? "putty" : "dark")} />
                  <span className={cn("w-6", chassis === "putty" && "text-fg")}>Pty</span>
                </div>
              </>
            )}
            <NavKey item={{ id: "settings", label: "Settings", icon: SettingsIcon }} active={page === "settings"} collapsed={collapsed} onClick={() => nav.go("settings")} />
            <button
              onClick={() => setCollapsed(!collapsed)}
              className={cn("label flex items-center gap-2.5 h-8 rounded-[4px] text-xs text-faint hover:text-fg hover:bg-raised/60", collapsed ? "justify-center" : "pl-[30px] pr-2.5")}
              aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            >
              {collapsed ? <ChevronsRight className="size-4" /> : <ChevronsLeft className="size-4" />}
              {!collapsed && "Collapse"}
            </button>
            <div aria-hidden className="vents h-2.5 mx-2 mt-1.5 opacity-70" />
          </div>
        </nav>
        <main className="flex-1 min-w-0 flex flex-col overflow-hidden">
          {update.data && known && updateDismissed !== known.version && (
            <div className="flex items-center gap-3 px-5 py-2 border-b border-line bg-raised/50 text-sm" role="status">
              <Lamp tone="info" />
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
            <div className="flex items-center gap-2.5 px-5 py-2 border-b border-line bg-raised/50 text-sm text-muted" role="status">
              <Lamp tone="warn" blink /> Collecting the first samples…
            </div>
          )}
          <div className="flex-1 min-h-0 flex flex-col">{pages[page]}</div>
        </main>
      </div>
      <Jump open={jumpOpen} onOpenChange={setJumpOpen} />
    </NavContext.Provider>
  );
}
