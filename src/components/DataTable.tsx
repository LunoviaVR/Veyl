// Virtualized, sortable table for large datasets (process lists, events).
// Only visible rows are in the DOM. Keyboard: Up/Down move the selection,
// Enter activates, the context-menu key / Shift+F10 opens the row menu.
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp } from "lucide-react";
import { type KeyboardEvent, type MouseEvent, type ReactNode, useMemo, useRef } from "react";
import { cn } from "./ui";

export interface Column<T> {
  id: string;
  header: string;
  width: number | string;
  align?: "left" | "right";
  cell: (row: T) => ReactNode;
  sortValue?: (row: T) => number | string | null;
  title?: string;
}

export interface SortState {
  id: string;
  desc: boolean;
}

export function sortRows<T>(rows: T[], columns: Column<T>[], sort: SortState | null): T[] {
  const col = sort && columns.find((c) => c.id === sort.id);
  if (!col?.sortValue) return rows;
  const dir = sort!.desc ? -1 : 1;
  const get = col.sortValue;
  return [...rows].sort((a, b) => {
    const va = get(a);
    const vb = get(b);
    // Missing values always sort last, regardless of direction.
    if (va == null && vb == null) return 0;
    if (va == null) return 1;
    if (vb == null) return -1;
    if (typeof va === "number" && typeof vb === "number") return (va - vb) * dir;
    return String(va).localeCompare(String(vb), undefined, { numeric: true, sensitivity: "base" }) * dir;
  });
}

export function DataTable<T>({
  rows,
  columns,
  rowKey,
  sort,
  onSort,
  selected,
  onSelect,
  onActivate,
  onContextMenu,
  rowHeight = 26,
  rowClass,
  indent,
  label,
  empty = "Nothing to show",
}: {
  rows: T[];
  columns: Column<T>[];
  rowKey: (r: T) => string;
  sort?: SortState | null;
  onSort?: (s: SortState) => void;
  selected?: string | null;
  onSelect?: (key: string, row: T) => void;
  onActivate?: (row: T) => void;
  onContextMenu?: (row: T, e: MouseEvent | KeyboardEvent) => void;
  rowHeight?: number;
  rowClass?: (r: T) => string | undefined;
  indent?: (r: T) => number;
  label: string;
  empty?: string;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({ count: rows.length, getScrollElement: () => scroller.current, estimateSize: () => rowHeight, overscan: 12 });
  const template = useMemo(() => columns.map((c) => (typeof c.width === "number" ? `${c.width}px` : c.width)).join(" "), [columns]);
  const selIndex = selected == null ? -1 : rows.findIndex((r) => rowKey(r) === selected);

  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!rows.length) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = Math.max(0, Math.min(rows.length - 1, selIndex + (e.key === "ArrowDown" ? 1 : -1)));
      const r = rows[next];
      if (r) {
        onSelect?.(rowKey(r), r);
        v.scrollToIndex(next, { align: "auto" });
      }
    } else if (e.key === "Enter" && selIndex >= 0) {
      const r = rows[selIndex];
      if (r) onActivate?.(r);
    } else if ((e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) && selIndex >= 0) {
      e.preventDefault();
      const r = rows[selIndex];
      if (r) onContextMenu?.(r, e);
    }
  };

  return (
    <div className="flex flex-col min-h-0 h-full text-[12.5px]" role="grid" aria-label={label} aria-rowcount={rows.length}>
      <div className="grid shrink-0 border-b border-line bg-panel text-muted text-[11.5px]" style={{ gridTemplateColumns: template }} role="row">
        {columns.map((c) => {
          const active = sort?.id === c.id;
          return (
            <button
              key={c.id}
              role="columnheader"
              aria-sort={active ? (sort!.desc ? "descending" : "ascending") : "none"}
              title={c.title}
              disabled={!c.sortValue}
              onClick={() => onSort?.({ id: c.id, desc: active ? !sort!.desc : c.align === "right" })}
              className={cn("h-7 px-2 flex items-center gap-1 truncate hover:text-fg disabled:hover:text-muted", c.align === "right" && "justify-end", active && "text-fg")}
            >
              <span className="truncate">{c.header}</span>
              {active && (sort!.desc ? <ArrowDown className="size-3 shrink-0" /> : <ArrowUp className="size-3 shrink-0" />)}
            </button>
          );
        })}
      </div>
      <div ref={scroller} tabIndex={0} onKeyDown={onKey} className="flex-1 min-h-0 overflow-auto focus-visible:outline-none">
        {!rows.length && <p className="px-3 py-6 text-center text-muted">{empty}</p>}
        <div style={{ height: v.getTotalSize(), position: "relative" }}>
          {v.getVirtualItems().map((vi) => {
            const r = rows[vi.index]!;
            const key = rowKey(r);
            const isSel = key === selected;
            return (
              <div
                key={key}
                role="row"
                aria-selected={isSel}
                onClick={() => onSelect?.(key, r)}
                onDoubleClick={() => onActivate?.(r)}
                onContextMenu={(e) => {
                  onSelect?.(key, r);
                  onContextMenu?.(r, e);
                }}
                className={cn("grid absolute left-0 right-0 items-center border-b border-line/50 cursor-default", isSel ? "bg-accent/12" : "hover:bg-hover", rowClass?.(r))}
                style={{ gridTemplateColumns: template, height: rowHeight, transform: `translateY(${vi.start}px)` }}
              >
                {columns.map((c, ci) => (
                  <div
                    key={c.id}
                    role="gridcell"
                    className={cn("px-2 truncate tabular", c.align === "right" && "text-right")}
                    style={ci === 0 && indent ? { paddingLeft: 8 + indent(r) * 14 } : undefined}
                  >
                    {c.cell(r)}
                  </div>
                ))}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
