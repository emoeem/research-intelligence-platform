import { useEffect, useMemo, useRef, useState, useId } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { RowData } from "@tanstack/react-table";
import type { DataColumn } from "../data";

export function VirtualTable<Row extends RowData>({
  rows,
  columns,
  rowKey,
  label,
  rowHeight = 36,
  height = 480,
  selectedRowKey,
  onRowActivate,
  onReachEnd,
  hasMore = false,
}: {
  rows: readonly Row[];
  columns: readonly DataColumn<Row>[];
  rowKey: (row: Row) => string;
  label: string;
  rowHeight?: number;
  height?: number;
  selectedRowKey?: string;
  onRowActivate?: (row: Row) => void;
  onReachEnd?: () => void;
  hasMore?: boolean;
}) {
  const gridId = useId();
  const scrollRef = useRef<HTMLDivElement>(null);
  const requestedEndCount = useRef(-1);
  const keys = useMemo(() => rows.map(rowKey), [rows, rowKey]);
  const [activeKey, setActiveKey] = useState<string | undefined>(() => keys[0]);
  const activeIndex = Math.max(
    0,
    keys.findIndex((key) => key === activeKey),
  );
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    initialRect: { width: 1024, height },
    overscan: 10,
    getItemKey: (index) => keys[index] ?? index,
    useFlushSync: false,
  });
  useEffect(() => {
    if (rows.length === 0) setActiveKey(undefined);
    else if (!keys.includes(activeKey ?? ""))
      setActiveKey(keys[Math.min(activeIndex, rows.length - 1)]);
  }, [activeIndex, activeKey, keys, rows.length]);
  const measuredItems = virtualizer.getVirtualItems();
  // A zero-size first paint (SSR, hidden tabs, or DOM test environments without
  // ResizeObserver) still gets a small deterministic first window, never all rows.
  const virtualItems =
    measuredItems.length > 0
      ? measuredItems
      : rows.slice(0, Math.min(10, rows.length)).map((_, index) => ({
          index,
          start: index * rowHeight,
          size: rowHeight,
          key: keys[index] ?? index,
        }));
  const totalSize = virtualizer.getTotalSize() || rows.length * rowHeight;
  const lastVirtualIndex = measuredItems.at(-1)?.index ?? -1;
  useEffect(() => {
    if (!hasMore || rows.length === 0) {
      requestedEndCount.current = -1;
      return;
    }
    if (lastVirtualIndex >= rows.length - 8 && requestedEndCount.current !== rows.length) {
      requestedEndCount.current = rows.length;
      onReachEnd?.();
    }
  }, [hasMore, lastVirtualIndex, onReachEnd, rows.length]);
  const move = (next: number) => {
    if (rows.length === 0) return;
    const index = Math.max(0, Math.min(rows.length - 1, next));
    setActiveKey(keys[index]);
    virtualizer.scrollToIndex(index, { align: "auto" });
  };
  const activeRow = rows[activeIndex];
  return (
    <div
      className="virtual-table border border-edge rounded-surface overflow-hidden"
      role="grid"
      aria-label={label}
      aria-rowcount={rows.length + 1}
      aria-colcount={columns.length}
      aria-activedescendant={
        activeRow
          ? `${gridId}-row-${encodeURIComponent(keys[activeIndex] ?? String(activeIndex))}`
          : undefined
      }
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key === "ArrowDown") {
          event.preventDefault();
          move(activeIndex + 1);
        } else if (event.key === "ArrowUp") {
          event.preventDefault();
          move(activeIndex - 1);
        } else if (event.key === "Home") {
          event.preventDefault();
          move(0);
        } else if (event.key === "End") {
          event.preventDefault();
          move(rows.length - 1);
        } else if ((event.key === "Enter" || event.key === " ") && activeRow && onRowActivate) {
          event.preventDefault();
          onRowActivate(activeRow);
        }
      }}
    >
      <div
        className="grid border-b border-edge bg-base-200 text-xs font-semibold"
        role="row"
        style={{
          gridTemplateColumns: columns.map((column) => column.width ?? "minmax(0, 1fr)").join(" "),
        }}
      >
        {columns.map((column) => (
          <div role="columnheader" key={column.id} className="px-3 py-2">
            {column.label}
          </div>
        ))}
      </div>
      <div
        ref={scrollRef}
        style={{ height, overflow: "auto", position: "relative" }}
        data-testid="virtual-table-scroll"
      >
        <div style={{ height: totalSize, width: "100%", position: "relative" }}>
          {virtualItems.map((item) => {
            const row = rows[item.index];
            if (!row) return null;
            const key = keys[item.index] ?? String(item.key);
            const selected = selectedRowKey === key;
            return (
              <div
                key={item.key}
                id={`${gridId}-row-${encodeURIComponent(key)}`}
                role="row"
                aria-rowindex={item.index + 2}
                aria-selected={selected}
                data-row-key={key}
                data-testid="virtual-table-row"
                className={`grid border-b border-edge-soft text-xs ${selected ? "bg-base-200" : "hover:bg-base-200/50"}`}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  height: item.size,
                  transform: `translateY(${item.start}px)`,
                  gridTemplateColumns: columns
                    .map((column) => column.width ?? "minmax(0, 1fr)")
                    .join(" "),
                }}
                onMouseEnter={() => setActiveKey(key)}
                onClick={() => {
                  setActiveKey(key);
                  onRowActivate?.(row);
                }}
              >
                {columns.map((column) => (
                  <div role="gridcell" key={column.id} className="px-3 py-2 truncate">
                    {column.render
                      ? column.render(row)
                      : String((row as Record<string, unknown>)[column.id] ?? "-")}
                  </div>
                ))}
              </div>
            );
          })}
        </div>
      </div>
      <div className="sr-only" aria-live="polite">
        {rows.length ? `第 ${activeIndex + 1} 行，共 ${rows.length} 行` : "没有记录"}
      </div>
    </div>
  );
}
