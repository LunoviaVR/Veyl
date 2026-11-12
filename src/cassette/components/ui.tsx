// Console primitives for the Cassette Futurism interface. Styling comes only from theme tokens
// and the classes in cassette.css; the tokens mean painted chassis outside a screen and
// phosphor on glass inside one.
import * as Dialog from "@radix-ui/react-dialog";
import { AlertTriangle, CircleCheck, CircleDashed, Info, Lock, OctagonAlert, PlugZap, X } from "lucide-react";
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from "react";
import { splitUnit } from "../../lib/format";
import type { Availability, Severity, Status, Tone } from "../../lib/types";

export function cn(...parts: (string | false | null | undefined)[]) {
  return parts.filter(Boolean).join(" ");
}

/** Indicator light; a dark lens when it has no tone. */
export function Lamp({ tone, blink, className }: { tone?: Tone | null; blink?: boolean; className?: string }) {
  return <span aria-hidden className={cn("lamp", className)} data-tone={tone ?? undefined} data-blink={tone && blink ? "" : undefined} />;
}

/** A monitor with its label plate: the title and actions sit on the chassis, the body is the screen. */
export function Panel({ title, actions, children, className, bodyClass }: { title?: ReactNode; actions?: ReactNode; children: ReactNode; className?: string; bodyClass?: string }) {
  // Lists and tables ask for a flush screen with `p-0`; everything else gets the default inset.
  const flush = /\bp-0\b/.test(bodyClass ?? "");
  return (
    <section className={cn("flex flex-col min-w-0", className)}>
      {(title || actions) && (
        <header className="flex items-end justify-between gap-3 px-1.5 pb-1.5 min-h-9 shrink-0">
          <h2 className="label text-xs text-muted truncate pb-0.5">{title}</h2>
          <div className="flex items-center gap-1.5 shrink-0">{actions}</div>
        </header>
      )}
      <div className={cn("crt min-h-0 flex-1", !flush && "p-4", bodyClass)}>{children}</div>
    </section>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="flex items-end justify-between gap-6 px-5 pt-5 pb-4 shrink-0">
      <div className="min-w-0 border-l-4 border-accent pl-3">
        <h1 className="label text-[24px] leading-none font-bold">{title}</h1>
        {subtitle && <p className="text-muted mt-1.5 text-sm max-w-[88ch]">{subtitle}</p>}
      </div>
      <div className="flex items-center justify-end flex-wrap gap-2 shrink-0">{actions}</div>
    </div>
  );
}

type Variant = "default" | "primary" | "danger" | "ghost";

export function Button({ variant = "default", className, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  const styles: Record<Variant, string> = {
    default: "key bg-raised border-line-strong text-fg hover:bg-hover",
    primary: "key bg-accent border-line text-accent-fg hover:brightness-110",
    danger: "key bg-danger border-line text-accent-fg hover:brightness-110",
    ghost: "bg-transparent border-transparent text-muted hover:text-fg hover:bg-hover",
  };
  return (
    <button
      type="button"
      {...rest}
      className={cn("label inline-flex items-center justify-center gap-1.5 h-8 px-3 rounded-[4px] border text-xs whitespace-nowrap disabled:opacity-45 disabled:pointer-events-none", styles[variant], className)}
    />
  );
}

export function IconButton({ label, children, className, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button type="button" aria-label={label} title={label} {...rest} className={cn("inline-flex items-center justify-center size-8 rounded-[4px] text-muted hover:text-fg hover:bg-hover disabled:opacity-40", className)}>
      {children}
    </button>
  );
}

/** Entry fields are one-line displays let into whatever they sit on. */
const FIELD = "screen field h-8 rounded-[4px] text-sm text-fg placeholder:text-faint focus:outline-none focus:border-accent disabled:opacity-50";

export function SearchInput(props: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <label className="screen relative flex items-center">
      <span aria-hidden className="absolute left-2.5 text-muted pointer-events-none">
        &gt;
      </span>
      <input type="search" {...props} className={cn(FIELD, "w-72 pl-7 pr-2", props.className)} />
    </label>
  );
}

export function TextInput(props: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={cn(FIELD, "px-2.5", props.className)} />;
}

export function Select<T extends string>({ value, onChange, options, label, className, disabled }: { value: T; onChange: (v: T) => void; options: { value: T; label: string }[]; label?: string; className?: string; disabled?: boolean }) {
  return (
    <select aria-label={label} disabled={disabled} value={value} onChange={(e) => onChange(e.target.value as T)} className={cn(FIELD, "px-2", className)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

/** `plain`: a two-position selector rather than on/off, so no pilot light. */
export function Switch({ checked, onChange, label, disabled, plain }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean; plain?: boolean }) {
  return (
    <button type="button" role="switch" aria-checked={checked} aria-label={label} disabled={disabled} onClick={() => onChange(!checked)} className="switch" data-plain={plain ? "" : undefined}>
      <span />
    </button>
  );
}

/** A row of latching selector keys; the lit one is in. */
export function Tabs<T extends string>({ value, onChange, tabs }: { value: T; onChange: (v: T) => void; tabs: { value: T; label: string; count?: number }[] }) {
  return (
    <div role="tablist" className="flex flex-wrap items-center gap-1.5 px-5 pb-3 shrink-0">
      {tabs.map((t) => (
        <button
          key={t.value}
          role="tab"
          aria-selected={value === t.value}
          onClick={() => onChange(t.value)}
          className={cn("key label inline-flex items-center gap-2 h-8 px-3 rounded-[4px] border border-line-strong text-xs", value === t.value ? "bg-hover text-fg" : "bg-raised text-muted hover:text-fg")}
        >
          <Lamp tone={value === t.value ? "ok" : null} />
          {t.label}
          {t.count !== undefined && <span className="text-faint tabular">{t.count}</span>}
        </button>
      ))}
    </div>
  );
}

const AVAIL: Record<Availability, { label: string; cls: string; Icon: typeof Info }> = {
  available: { label: "Available", cls: "text-ok", Icon: CircleCheck },
  unsupported: { label: "Unsupported", cls: "text-faint", Icon: CircleDashed },
  permissionRequired: { label: "Permission required", cls: "text-warn", Icon: Lock },
  providerUnavailable: { label: "Provider unavailable", cls: "text-muted", Icon: PlugZap },
  stale: { label: "Data stale", cls: "text-warn", Icon: AlertTriangle },
  waitingForSamples: { label: "Waiting for samples", cls: "text-info", Icon: CircleDashed },
  error: { label: "Error", cls: "text-bad", Icon: OctagonAlert },
};

/** Status label with its explanation; renders nothing for plain "available" unless `always`. */
export function StatusNote({ status, always, compact }: { status: Status | null | undefined; always?: boolean; compact?: boolean }) {
  if (!status || (status.state === "available" && !always)) return null;
  const a = AVAIL[status.state];
  return (
    <div className={cn("flex items-start gap-1.5 text-xs", a.cls)} title={status.message ?? undefined}>
      <a.Icon className="size-4 mt-px shrink-0" />
      <span className={cn(compact && "truncate")}>
        <span className="font-medium">{a.label}</span>
        {status.message && <span className="text-muted"> — {status.message}</span>}
      </span>
    </div>
  );
}

const SEVERITY_CLASS: Record<Severity, string> = { info: "bg-info", warning: "bg-warn", error: "bg-bad", critical: "bg-crit" };

/** Severity in reverse video, like a terminal's highlighted field. Always a lit tile, wherever it sits. */
export function SeverityBadge({ severity }: { severity: Severity }) {
  return <span className={cn("screen label inline-flex items-center justify-center h-5 min-w-[72px] px-1.5 rounded-[2px] text-2xs text-accent-fg", SEVERITY_CLASS[severity])}>{severity[0]!.toUpperCase() + severity.slice(1)}</span>;
}

export function Badge({ children, tone = "neutral", className }: { children: ReactNode; tone?: "neutral" | Tone; className?: string }) {
  const tones = { neutral: "text-muted", ok: "text-ok", warn: "text-warn", bad: "text-bad", info: "text-info" };
  return <span className={cn("screen label inline-flex items-center h-5 px-1.5 rounded-[2px] border border-current bg-panel text-2xs whitespace-nowrap", tones[tone], className)}>{children}</span>;
}

/** A terminal line for "nothing here". Meant for a screen; on the chassis, wrap it in `crt`. */
export function Empty({ icon, title, children }: { icon?: ReactNode; title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center text-center gap-2 py-10 px-6 text-muted">
      {icon}
      <p className="readout-title text-fg">
        &gt; {title}
        <span className="cursor" />
      </p>
      {children && <div className="max-w-md text-sm">{children}</div>}
    </div>
  );
}

export function ErrorNote({ error }: { error: string | null | undefined }) {
  if (!error) return null;
  return (
    <div role="alert" className="flex items-stretch text-bad text-sm border border-bad/50 bg-bad/10 rounded-[3px] overflow-hidden">
      <span aria-hidden className="hazard w-2.5 shrink-0" />
      <span className="flex items-start gap-2 px-3 py-2 min-w-0">
        <OctagonAlert className="size-4 mt-0.5 shrink-0" />
        <span className="selectable break-words min-w-0">{error}</span>
      </span>
    </div>
  );
}

/** Label/value grid for detail panes. */
export function Fields({ rows }: { rows: [string, ReactNode][] }) {
  return (
    <dl className="grid grid-cols-[max-content_1fr] gap-x-5 gap-y-1.5 text-sm">
      {rows.map(([k, v]) => (
        <div key={k} className="contents">
          <dt className="text-muted">{k}</dt>
          <dd className="min-w-0 break-words selectable tabular">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

/** LED bar graph. Plain phosphor while normal; amber from 75 %, red from 90 %, unless a tone is given. */
export function Meter({ value, max = 100, tone }: { value: number | null | undefined; max?: number; tone?: "ok" | "warn" | "bad" | "accent" }) {
  const pct = value == null ? 0 : Math.max(0, Math.min(100, (value / max) * 100));
  const auto = pct >= 90 ? "bad" : pct >= 75 ? "warn" : "accent";
  return (
    <div className="meter" data-tone={tone ?? auto} role="meter" aria-valuenow={value ?? undefined} aria-valuemin={0} aria-valuemax={max}>
      {value != null && <i style={{ width: `${pct}%` }} />}
    </div>
  );
}

/** Modal dialog (focus-trapped, Escape closes): a terminal window in front of the console. */
export function Modal({ open, onOpenChange, title, children, footer, wide }: { open: boolean; onOpenChange: (v: boolean) => void; title: string; children: ReactNode; footer?: ReactNode; wide?: boolean }) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/65 z-40" />
        <Dialog.Content className={cn("crt pop fixed z-50 left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 max-h-[85vh] flex flex-col", wide ? "w-[min(960px,92vw)]" : "w-[min(520px,92vw)]")}>
          <div className="flex items-center justify-between gap-3 pl-5 pr-3 h-12 border-b border-line shrink-0">
            <Dialog.Title className="readout-title truncate">{title}</Dialog.Title>
            <Dialog.Close asChild>
              <IconButton label="Close">
                <X className="size-4" />
              </IconButton>
            </Dialog.Close>
          </div>
          <Dialog.Description asChild>
            <div className="p-5 overflow-auto text-sm">{children}</div>
          </Dialog.Description>
          {footer && <div className="flex justify-end gap-2 px-5 py-3 border-t border-line shrink-0">{footer}</div>}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** Screen readout: the number large in the terminal face, its unit smaller beside it. */
export function Kpi({ label, value, sub, tone }: { label: string; value: ReactNode; sub?: ReactNode; tone?: "warn" | "bad" | "ok" }) {
  const t = tone === "bad" ? "text-bad" : tone === "warn" ? "text-warn" : tone === "ok" ? "text-ok" : "";
  const [number, unit] = typeof value === "string" ? splitUnit(value) : [value, ""];
  return (
    <div className="min-w-0">
      <div className="label text-2xs text-muted truncate">{label}</div>
      <div className={cn("readout tabular truncate", t)}>
        {number}
        {unit && <span className="ml-1 text-[0.63em] opacity-70">{unit}</span>}
      </div>
      {sub && <div className="text-2xs text-faint truncate tabular">{sub}</div>}
    </div>
  );
}

const SWEEP = 240;

function polar(deg: number, r: number): [number, number] {
  const a = ((deg - 90) * Math.PI) / 180;
  return [60 + r * Math.cos(a), 60 + r * Math.sin(a)];
}

function arc(from: number, to: number, r: number) {
  const [x0, y0] = polar(from, r);
  const [x1, y1] = polar(to, r);
  return `M${x0.toFixed(1)} ${y0.toFixed(1)}A${r} ${r} 0 ${to - from > 180 ? 1 : 0} 1 ${x1.toFixed(1)} ${y1.toFixed(1)}`;
}

/**
 * Analog panel meter. `zones` (ascending) paint the caution and limit bands on the scale and
 * colour the digital window once the needle is inside one. No reading parks the needle below zero.
 */
export function Dial({ label, value, display, min = 0, max = 100, zones = [], title }: { label: string; value: number | null | undefined; display: string; min?: number; max?: number; zones?: { from: number; tone: "warn" | "bad" }[]; title?: string }) {
  const angle = (v: number) => -SWEEP / 2 + SWEEP * Math.max(0, Math.min(1, (v - min) / (max - min)));
  const tone = value == null ? undefined : zones.findLast((z) => value >= z.from)?.tone;
  return (
    <figure className="flex flex-col items-center gap-1.5 m-0" title={title}>
      <div className="dial" role="meter" aria-label={label} aria-valuemin={min} aria-valuemax={max} aria-valuenow={value ?? undefined} aria-valuetext={display}>
        <svg viewBox="0 0 120 120" aria-hidden>
          <circle cx="60" cy="60" r="57" className="fill-panel stroke-line-strong" strokeWidth="2" />
          <path d={arc(-SWEEP / 2, SWEEP / 2, 47)} className="stroke-muted" fill="none" />
          {zones.map((z, i) => (
            <path key={z.from} d={arc(angle(z.from), angle(zones[i + 1]?.from ?? max), 51.5)} className={z.tone === "bad" ? "stroke-bad" : "stroke-warn"} fill="none" strokeWidth="4" />
          ))}
          {Array.from({ length: 11 }, (_, i) => {
            const [x1, y1] = polar(-SWEEP / 2 + i * (SWEEP / 10), i % 5 === 0 ? 36 : 41);
            const [x2, y2] = polar(-SWEEP / 2 + i * (SWEEP / 10), 47);
            return <line key={i} x1={x1} y1={y1} x2={x2} y2={y2} className="stroke-muted" strokeWidth={i % 5 === 0 ? 2 : 1} />;
          })}
        </svg>
        <div className="dial-needle" style={{ transform: `rotate(${value == null ? -SWEEP / 2 - 8 : angle(value)}deg)` }} />
        <div className="dial-hub" />
        <div className={cn("dial-readout screen", tone === "bad" ? "text-bad" : tone === "warn" ? "text-warn" : undefined)}>{display}</div>
      </div>
      <figcaption className="label text-2xs text-muted">{label}</figcaption>
    </figure>
  );
}

/** Tape transport: the reels turn while something is being recorded. */
export function Reels({ spinning, className }: { spinning?: boolean; className?: string }) {
  const reel = (cx: number) => (
    <g className="reel">
      <circle cx={cx} cy="14" r="11.5" className="fill-panel stroke-muted" strokeWidth="1.5" />
      {[0, 120, 240].map((a) => (
        <rect key={a} x={cx - 1.25} y="4.5" width="2.5" height="6" className="fill-muted" transform={`rotate(${a} ${cx} 14)`} />
      ))}
      <circle cx={cx} cy="14" r="2.5" className="fill-muted" />
    </g>
  );
  return (
    <svg viewBox="0 0 68 28" className={cn("reels shrink-0", className ?? "w-[68px] h-7")} data-spinning={spinning ? "" : undefined} role="img" aria-label={spinning ? "Tape running" : "Tape stopped"}>
      <path d="M14 27h40" className="stroke-faint" strokeWidth="1.5" />
      {reel(14)}
      {reel(54)}
    </svg>
  );
}
