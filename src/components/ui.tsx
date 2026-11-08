// Small, consistent UI primitives. Styling comes only from theme tokens.
import * as Dialog from "@radix-ui/react-dialog";
import { AlertTriangle, CircleCheck, CircleDashed, Info, Lock, OctagonAlert, PlugZap, Search, X } from "lucide-react";
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from "react";
import type { Availability, Severity, Status } from "../lib/types";

export function cn(...parts: (string | false | null | undefined)[]) {
  return parts.filter(Boolean).join(" ");
}

export function Panel({ title, actions, children, className, bodyClass }: { title?: ReactNode; actions?: ReactNode; children: ReactNode; className?: string; bodyClass?: string }) {
  return (
    <section className={cn("rounded-md border border-line bg-panel flex flex-col min-w-0", className)}>
      {(title || actions) && (
        <header className="flex items-center justify-between gap-2 px-3 h-9 border-b border-line shrink-0">
          <h2 className="text-[12px] font-semibold tracking-wide text-muted uppercase truncate">{title}</h2>
          <div className="flex items-center gap-1.5">{actions}</div>
        </header>
      )}
      <div className={cn("p-3 min-h-0", bodyClass)}>{children}</div>
    </section>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="flex items-end justify-between gap-4 px-5 pt-4 pb-3 shrink-0">
      <div className="min-w-0">
        <h1 className="text-[18px] font-semibold leading-tight">{title}</h1>
        {subtitle && <p className="text-muted mt-0.5 text-[12.5px]">{subtitle}</p>}
      </div>
      <div className="flex items-center gap-2 shrink-0">{actions}</div>
    </div>
  );
}

type Variant = "default" | "primary" | "danger" | "ghost";

export function Button({ variant = "default", className, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  const styles: Record<Variant, string> = {
    default: "bg-raised border-line-strong hover:bg-hover text-fg",
    primary: "bg-accent border-accent text-accent-fg hover:brightness-110",
    danger: "bg-bad/15 border-bad/50 text-bad hover:bg-bad/25",
    ghost: "bg-transparent border-transparent hover:bg-hover text-muted hover:text-fg",
  };
  return (
    <button
      type="button"
      {...rest}
      className={cn(
        "inline-flex items-center gap-1.5 h-7 px-2.5 rounded-sm border text-[12.5px] font-medium whitespace-nowrap disabled:opacity-50 disabled:pointer-events-none",
        styles[variant],
        className,
      )}
    />
  );
}

export function IconButton({ label, children, className, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button type="button" aria-label={label} title={label} {...rest} className={cn("inline-flex items-center justify-center size-7 rounded-sm text-muted hover:text-fg hover:bg-hover disabled:opacity-40", className)}>
      {children}
    </button>
  );
}

export function SearchInput(props: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <label className="relative flex items-center">
      <Search className="absolute left-2 size-3.5 text-faint pointer-events-none" />
      <input
        type="search"
        {...props}
        className={cn("h-7 w-56 pl-7 pr-2 rounded-sm bg-raised border border-line-strong text-[12.5px] placeholder:text-faint focus:outline-none focus:border-accent", props.className)}
      />
    </label>
  );
}

export function TextInput(props: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={cn("h-7 px-2 rounded-sm bg-raised border border-line-strong text-[12.5px] focus:outline-none focus:border-accent", props.className)} />;
}

export function Select<T extends string>({ value, onChange, options, label, className }: { value: T; onChange: (v: T) => void; options: { value: T; label: string }[]; label?: string; className?: string }) {
  return (
    <select
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value as T)}
      className={cn("h-7 px-2 rounded-sm bg-raised border border-line-strong text-[12.5px] focus:outline-none focus:border-accent", className)}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

export function Switch({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn("relative inline-flex h-[18px] w-8 shrink-0 items-center rounded-full border transition-colors disabled:opacity-50", checked ? "bg-accent border-accent" : "bg-raised border-line-strong")}
    >
      <span className={cn("inline-block size-3 rounded-full transition-transform", checked ? "translate-x-[15px] bg-accent-fg" : "translate-x-[2px] bg-muted")} />
    </button>
  );
}

export function Tabs<T extends string>({ value, onChange, tabs }: { value: T; onChange: (v: T) => void; tabs: { value: T; label: string; count?: number }[] }) {
  return (
    <div role="tablist" className="flex items-center gap-0.5 border-b border-line px-5 shrink-0">
      {tabs.map((t) => (
        <button
          key={t.value}
          role="tab"
          aria-selected={value === t.value}
          onClick={() => onChange(t.value)}
          className={cn("h-8 px-3 -mb-px border-b-2 text-[12.5px]", value === t.value ? "border-accent text-fg font-medium" : "border-transparent text-muted hover:text-fg")}
        >
          {t.label}
          {t.count !== undefined && <span className="ml-1.5 text-faint tabular">{t.count}</span>}
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
    <div className={cn("flex items-start gap-1.5 text-[12px]", a.cls)} title={status.message ?? undefined}>
      <a.Icon className="size-3.5 mt-px shrink-0" />
      <span className={cn(compact && "truncate")}>
        <span className="font-medium">{a.label}</span>
        {status.message && <span className="text-muted"> — {status.message}</span>}
      </span>
    </div>
  );
}

export const SEVERITY_CLASS: Record<Severity, string> = {
  info: "text-info bg-info/12 border-info/30",
  warning: "text-warn bg-warn/12 border-warn/30",
  error: "text-bad bg-bad/12 border-bad/30",
  critical: "text-crit bg-crit/12 border-crit/35",
};

export function SeverityBadge({ severity }: { severity: Severity }) {
  return <span className={cn("inline-flex items-center h-[18px] px-1.5 rounded-sm border text-[11px] font-medium capitalize", SEVERITY_CLASS[severity])}>{severity}</span>;
}

export function Badge({ children, tone = "neutral" }: { children: ReactNode; tone?: "neutral" | "ok" | "warn" | "bad" | "info" }) {
  const tones = { neutral: "text-muted border-line-strong", ok: "text-ok border-ok/40", warn: "text-warn border-warn/40", bad: "text-bad border-bad/40", info: "text-info border-info/40" };
  return <span className={cn("inline-flex items-center h-[18px] px-1.5 rounded-sm border text-[11px] whitespace-nowrap", tones[tone])}>{children}</span>;
}

export function Empty({ icon, title, children }: { icon?: ReactNode; title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center text-center gap-2 py-10 px-6 text-muted">
      {icon}
      <p className="text-fg font-medium">{title}</p>
      {children && <div className="max-w-md text-[12.5px]">{children}</div>}
    </div>
  );
}

export function ErrorNote({ error }: { error: string | null | undefined }) {
  if (!error) return null;
  return (
    <div role="alert" className="flex items-start gap-1.5 text-bad text-[12px] px-3 py-2 border border-bad/30 bg-bad/8 rounded-sm">
      <OctagonAlert className="size-3.5 mt-px shrink-0" />
      <span className="selectable">{error}</span>
    </div>
  );
}

/** Label/value grid for detail panes. */
export function Fields({ rows }: { rows: [string, ReactNode][] }) {
  return (
    <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-[12.5px]">
      {rows.map(([k, v]) => (
        <div key={k} className="contents">
          <dt className="text-muted">{k}</dt>
          <dd className="min-w-0 break-words selectable tabular">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

export function Meter({ value, max = 100, tone }: { value: number | null | undefined; max?: number; tone?: "ok" | "warn" | "bad" | "accent" }) {
  const pct = value == null ? 0 : Math.max(0, Math.min(100, (value / max) * 100));
  const auto = pct >= 90 ? "bad" : pct >= 75 ? "warn" : "accent";
  const colors = { ok: "bg-ok", warn: "bg-warn", bad: "bg-bad", accent: "bg-accent" };
  return (
    <div className="h-1.5 rounded-full bg-raised overflow-hidden" role="meter" aria-valuenow={value ?? undefined} aria-valuemin={0} aria-valuemax={max}>
      {value != null && <div className={cn("h-full rounded-full", colors[tone ?? auto])} style={{ width: `${pct}%` }} />}
    </div>
  );
}

/** Modal dialog (focus-trapped, Escape closes). */
export function Modal({ open, onOpenChange, title, children, footer, wide }: { open: boolean; onOpenChange: (v: boolean) => void; title: string; children: ReactNode; footer?: ReactNode; wide?: boolean }) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/50 z-40" />
        <Dialog.Content className={cn("fixed z-50 left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 max-h-[85vh] flex flex-col rounded-md border border-line-strong bg-panel shadow-2xl", wide ? "w-[min(900px,92vw)]" : "w-[min(480px,92vw)]")}>
          <div className="flex items-center justify-between px-4 h-11 border-b border-line shrink-0">
            <Dialog.Title className="font-semibold text-[14px]">{title}</Dialog.Title>
            <Dialog.Close asChild>
              <IconButton label="Close">
                <X className="size-4" />
              </IconButton>
            </Dialog.Close>
          </div>
          <Dialog.Description asChild>
            <div className="p-4 overflow-auto text-[13px]">{children}</div>
          </Dialog.Description>
          {footer && <div className="flex justify-end gap-2 px-4 py-3 border-t border-line shrink-0">{footer}</div>}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function Kpi({ label, value, sub, tone }: { label: string; value: ReactNode; sub?: ReactNode; tone?: "warn" | "bad" | "ok" }) {
  const t = tone === "bad" ? "text-bad" : tone === "warn" ? "text-warn" : tone === "ok" ? "text-ok" : "";
  return (
    <div className="min-w-0">
      <div className="text-[11.5px] text-muted truncate">{label}</div>
      <div className={cn("text-[19px] font-semibold tabular leading-tight truncate", t)}>{value}</div>
      {sub && <div className="text-[11.5px] text-faint truncate tabular">{sub}</div>}
    </div>
  );
}
