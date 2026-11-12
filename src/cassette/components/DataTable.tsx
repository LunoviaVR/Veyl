// The console's virtualized, sortable table. Sorting is the standard table's; only the drawing differs.
// Only visible rows are in the DOM. Keyboard: Up/Down move the selection,
// Enter activates, the context-menu key / Shift+F10 opens the row menu.
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp } from "lucide-react";
import { type KeyboardEvent, type MouseEvent, useMemo, useRef } from "react";
import { type Column as BaseColumn, type SortState, sortRows } from "../../components/DataTable";
import { cn } from "./ui";

export { type SortState, sortRows };

/** A standard column, plus the console's bar graph behind a cell. */
export interface Column<T> extends BaseColumn<T> {
  /** 0..1: draws a bar behind the cell, so the heavy rows stand out without reading the numbers. */
  bar?: (row: T) => number | null | undefined;
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
  rowHeight = 30,
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
  // Column widths are shared with the standard pages, which set smaller proportional text; the terminal face needs about a fifth more.
  const template = useMemo(() => columns.map((c) => (typeof c.width === "number" ? `${Math.round(c.width * 1.2)}px` : c.width)).join(" "), [columns]);
  const selIndex = selected == null ? -1 : rows.findIndex((r) => rowKey(r) === selected);

  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!rows.length) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      // With nothing selected, either arrow starts at the first row.
      const i = selIndex + (e.key === "ArrowDown" || selIndex < 0 ? 1 : -1);
      const r = rows[i];
      if (r) {
        onSelect?.(rowKey(r), r);
        v.scrollToIndex(i, { align: "auto" });
      }
    } else if (e.key === "Enter") {
      const r = rows[selIndex];
      if (r) onActivate?.(r);
    } else if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
      const r = rows[selIndex];
      if (r) {
        e.preventDefault();
        onContextMenu?.(r, e);
      }
    }
  };

  return (
    <div className="flex flex-col min-h-0 h-full text-sm" role="grid" aria-label={label} aria-rowcount={rows.length}>
      <div className="label grid shrink-0 border-b border-line-strong text-muted text-2xs" style={{ gridTemplateColumns: template }} role="row">
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
              className={cn("label h-8 px-2.5 flex items-center gap-1 truncate hover:text-fg disabled:hover:text-muted", c.align === "right" && "justify-end", active && "text-fg")}
            >
              <span className="truncate">{c.header}</span>
              {active && (sort!.desc ? <ArrowDown className="size-3 shrink-0" /> : <ArrowUp className="size-3 shrink-0" />)}
            </button>
          );
        })}
      </div>
      <div ref={scroller} tabIndex={0} onKeyDown={onKey} className="flex-1 min-h-0 overflow-auto focus-visible:outline-none">
        {!rows.length && <p className="px-3 py-8 text-center text-muted">&gt; {empty}</p>}
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
                className={cn("grid absolute left-0 right-0 items-center border-b border-line/60 cursor-default", isSel ? "bg-accent/20 shadow-[inset_3px_0_0_var(--color-accent)]" : "hover:bg-hover", rowClass?.(r))}
                style={{ gridTemplateColumns: template, height: rowHeight, transform: `translateY(${vi.start}px)` }}
              >
                {columns.map((c, ci) => {
                  const fill = Math.min(1, Math.max(0, c.bar?.(r) ?? 0)) * 100;
                  return (
                    <div
                      key={c.id}
                      role="gridcell"
                      className={cn("px-2.5 truncate tabular h-full content-center", c.align === "right" && "text-right")}
                      style={{
                        paddingLeft: ci === 0 && indent ? 10 + indent(r) * 16 : undefined,
                        backgroundImage: fill ? `linear-gradient(to left, var(--color-line-strong) ${fill}%, transparent ${fill}%)` : undefined,
                      }}
                    >
                      {c.cell(r)}
                    </div>
                  );
                })}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
