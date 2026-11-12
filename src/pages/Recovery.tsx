import { LifeBuoy, RotateCcw, ShieldCheck, Wifi, Wrench } from "lucide-react";
import { useState } from "react";
import { Badge, Button, ErrorNote, Modal, PageHeader, Panel } from "../components/ui";
import { useQuery } from "../lib/hooks";
import { api, errorText } from "../lib/ipc";
import type { BootStatus, RestartTarget } from "../lib/types";

/** The restart choices, shared with the Cassette Futurism page. */
export const RECOVERY_OPTIONS: { target: RestartTarget; title: string; action: string; icon: typeof LifeBuoy; admin: boolean; body: string }[] = [
  {
    target: "safeMode",
    title: "Safe Mode",
    action: "Restart in Safe Mode",
    icon: ShieldCheck,
    admin: true,
    body: "Starts Windows with only the basic drivers and services. Use it to remove a driver, update or app that stops Windows from starting or keeps crashing it. Lasts one boot: the restart after it starts Windows normally.",
  },
  {
    target: "safeModeNetworking",
    title: "Safe Mode with Networking",
    action: "Restart with networking",
    icon: Wifi,
    admin: true,
    body: "Safe Mode plus network drivers, for downloading a driver or running an online virus scan. Also lasts one boot.",
  },
  {
    target: "recovery",
    title: "Recovery options",
    action: "Restart to recovery options",
    icon: Wrench,
    admin: false,
    body: "Opens the Windows recovery screen (Choose an option): Startup Settings for Safe Mode with Command Prompt or turning off driver signature checks, plus System Restore, Startup Repair, Uninstall Updates, Command Prompt and UEFI firmware settings.",
  },
];

export const MODE_LABEL: Record<BootStatus["mode"], string> = { normal: "Normal", safeMode: "Safe Mode", safeModeNetworking: "Safe Mode with Networking" };

export const RESTART_WARNING = "Windows restarts right away. Save your work first: open apps are asked to close, and an app with unsaved changes can hold up or cancel the restart.";
export const BITLOCKER_NOTE = "Some recovery tools ask for the BitLocker recovery key when the drive is encrypted; have it at hand.";

export default function Recovery() {
  const status = useQuery(() => api.bootStatus(), []);
  const [confirm, setConfirm] = useState<RestartTarget | null>(null);
  const [error, setError] = useState<string | null>(null);
  const s = status.data;
  const safe = s && s.mode !== "normal";

  const restart = (target: RestartTarget) => {
    setConfirm(null);
    setError(null);
    // Windows may still be asking apps to close, or the user cancelled there: show the pending state.
    api.restartInto(target).then(status.refresh, (e) => setError(errorText(e)));
  };
  const cancel = () => {
    setError(null);
    api.cancelSafeMode().then(status.refresh, (e) => setError(errorText(e)));
  };
  const chosen = RECOVERY_OPTIONS.find((o) => o.target === confirm);

  return (
    <div className="flex-1 overflow-y-auto">
      <PageHeader title="Recovery" subtitle="Restart straight into Safe Mode or the Windows recovery options, without pressing keys while the PC starts." actions={s && <Badge tone={safe ? "warn" : "neutral"}>Running: {MODE_LABEL[s.mode]}</Badge>} />
      <div className="px-5 pb-5 flex flex-col gap-3 max-w-[1100px]">
        <ErrorNote error={status.error ?? error} />
        {safe && (
          <div role="status" className="flex items-center gap-3 px-3 py-2 rounded-sm border border-warn/40 bg-warn/8 text-[12.5px]">
            <LifeBuoy className="size-4 text-warn shrink-0" />
            <span>Windows is running in {MODE_LABEL[s.mode]}. A restart from here starts Windows normally.</span>
            <Button variant="primary" className="ml-auto" onClick={() => setConfirm("normal")}>
              <RotateCcw className="size-3.5" /> Restart normally
            </Button>
          </div>
        )}
        {s?.safeModePending && (
          <div role="status" className="flex items-center gap-3 px-3 py-2 rounded-sm border border-warn/40 bg-warn/8 text-[12.5px]">
            <LifeBuoy className="size-4 text-warn shrink-0" />
            <span>The next restart starts Safe Mode.</span>
            <Button className="ml-auto" onClick={cancel}>
              Cancel Safe Mode
            </Button>
          </div>
        )}
        {s && !s.elevated && (
          <div className="flex items-center gap-3 px-3 py-2 rounded-sm border border-line bg-panel text-[12.5px] text-muted">
            <ShieldCheck className="size-4 shrink-0" />
            <span>Safe Mode changes how Windows starts, which needs administrator rights.</span>
            <Button className="ml-auto" onClick={() => api.restartAsAdmin().catch((e) => setError(errorText(e)))}>
              <ShieldCheck className="size-3.5" /> Restart as administrator
            </Button>
          </div>
        )}
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-3">
          {RECOVERY_OPTIONS.map((o) => (
            <Panel key={o.target} title={o.title} bodyClass="flex flex-col gap-3 flex-1">
              <o.icon className="size-6 text-accent" aria-hidden />
              <p className="text-[12.5px] text-muted flex-1">{o.body}</p>
              <Button variant="primary" className="self-start" disabled={!s || (o.admin && !s.elevated) || (safe && o.target !== "recovery")} onClick={() => setConfirm(o.target)}>
                {o.action}
              </Button>
            </Panel>
          ))}
        </div>
      </div>
      <Modal
        open={!!confirm}
        onOpenChange={(o) => !o && setConfirm(null)}
        title="Restart now?"
        footer={
          <>
            <Button onClick={() => setConfirm(null)}>Cancel</Button>
            <Button variant="primary" autoFocus onClick={() => confirm && restart(confirm)}>
              {chosen?.action ?? "Restart normally"}
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-2">
          <p>
            The PC restarts into <b>{chosen?.title ?? "normal Windows"}</b>.
          </p>
          <p className="text-muted">{RESTART_WARNING}</p>
          {confirm === "recovery" && <p className="text-muted">{BITLOCKER_NOTE}</p>}
        </div>
      </Modal>
    </div>
  );
}
