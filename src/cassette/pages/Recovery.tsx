import { RotateCcw, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { Button, ErrorNote, Lamp, Modal, PageHeader } from "../components/ui";
import { useQuery } from "../../lib/hooks";
import { api, errorText } from "../../lib/ipc";
import type { BootStatus, RestartTarget } from "../../lib/types";
import { BITLOCKER_NOTE, MODE_LABEL, RECOVERY_OPTIONS, RESTART_WARNING } from "../../pages/Recovery";

/** Annunciator legends for the boot modes; lit for the one Windows is running in. */
const MODE_TILES: [BootStatus["mode"], string][] = [
  ["normal", "Normal"],
  ["safeMode", "Safe Mode"],
  ["safeModeNetworking", "Safe + Net"],
];

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
  const readout = !s ? "Reading boot mode" : safe ? `Running in ${MODE_LABEL[s.mode]}` : s.safeModePending ? "Safe Mode armed for next restart" : "Normal boot. Select a restart below";

  return (
    <div className="flex-1 overflow-y-auto">
      <PageHeader title="Recovery" subtitle="Restart straight into Safe Mode or the Windows recovery options, without pressing keys while the PC starts." />
      <div className="flex flex-col gap-4 px-5 pb-6 max-w-[1200px]">
        <ErrorNote error={status.error ?? error} />
        <section className="plate flex flex-col gap-3 p-3.5 min-w-0" aria-label="Boot mode">
          <header className="flex items-center gap-2.5 px-1 h-6">
            <Lamp tone={s ? (safe || s.safeModePending ? "warn" : "ok") : null} blink={s?.safeModePending} />
            <h2 className="label text-sm">Boot mode</h2>
            {s && <span className="label ml-auto text-xs text-faint">{s.elevated ? "Administrator" : "Standard user"}</span>}
          </header>
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-1.5">
            {MODE_TILES.map(([mode, label]) => {
              const on = s?.mode === mode;
              return (
                <div key={mode} className="annun" data-tone={on ? (mode === "normal" ? "ok" : "warn") : undefined} aria-label={`${MODE_LABEL[mode]}: ${on ? "running" : "off"}`}>
                  <span className="label text-xs font-bold leading-tight">{label}</span>
                  {on && <span className="text-2xs font-semibold leading-tight">Running</span>}
                </div>
              );
            })}
            <div className="annun" data-tone={s?.safeModePending ? "warn" : undefined} data-blink={s?.safeModePending ? "" : undefined} aria-label={`Safe Mode on next restart: ${s?.safeModePending ? "armed" : "off"}`}>
              <span className="label text-xs font-bold leading-tight">Safe next boot</span>
              {s?.safeModePending && <span className="text-2xs font-semibold leading-tight">Armed</span>}
            </div>
          </div>
          <div className="crt flex flex-wrap items-center gap-3 px-4 py-3">
            <p className="readout-sm flex-1 min-w-[16ch]" role="status">
              &gt; {readout}
              <span className="cursor" />
            </p>
            {safe && (
              <Button variant="primary" onClick={() => setConfirm("normal")}>
                <RotateCcw className="size-3.5" /> Restart normally
              </Button>
            )}
            {s?.safeModePending && <Button onClick={cancel}>Cancel Safe Mode</Button>}
            {s && !s.elevated && (
              <Button onClick={() => api.restartAsAdmin().catch((e) => setError(errorText(e)))} title="Safe Mode changes how Windows starts, which needs administrator rights">
                <ShieldCheck className="size-3.5" /> Restart as administrator
              </Button>
            )}
          </div>
        </section>
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
          {RECOVERY_OPTIONS.map((o) => (
            <section key={o.target} className="plate flex flex-col gap-3 p-3.5 min-w-0">
              <header className="flex items-center gap-2 px-1 h-6 min-w-0">
                <o.icon className="size-4 text-muted shrink-0" aria-hidden />
                <h2 className="label text-sm truncate">{o.title}</h2>
                {o.admin && s && !s.elevated && <span className="label ml-auto text-2xs text-warn shrink-0">Needs admin</span>}
              </header>
              <div className="crt flex-1 p-4 text-sm text-muted">{o.body}</div>
              <Button variant="primary" className="h-10" disabled={!s || (o.admin && !s.elevated) || (safe && o.target !== "recovery")} onClick={() => setConfirm(o.target)}>
                {o.action}
              </Button>
            </section>
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
        <div className="flex flex-col gap-3">
          <div aria-hidden className="hazard h-2.5 rounded-[2px]" />
          <p className="readout-sm">&gt; Target: {chosen?.title ?? "Normal Windows"}</p>
          <p className="text-muted">{RESTART_WARNING}</p>
          {confirm === "recovery" && <p className="text-muted">{BITLOCKER_NOTE}</p>}
        </div>
      </Modal>
    </div>
  );
}
