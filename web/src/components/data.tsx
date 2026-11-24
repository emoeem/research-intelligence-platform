/**
 * Shared data primitives used by research and administration workspaces.
 * TanStack Table owns sorting and row models; this layer owns semantic markup,
 * visual tokens and keyboard interactions.
 */
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { translateUi } from "../i18n";
import {
  createSortedRowModel,
  rowSortingFeature,
  columnSizingFeature,
  columnResizingFeature,
  columnVisibilityFeature,
  tableFeatures,
  useTable,
  type ColumnDef,
  type SortingState,
  type ColumnSizingState,
  type ColumnVisibilityState,
  type RowData,
} from "@tanstack/react-table";

export interface DataColumn<Row extends RowData> {
  id: string;
  label: string;
  width?: string;
  render?: (row: Row) => ReactNode;
  sortValue?: (row: Row) => string | number | null | undefined;
}

interface DataTableProps<Row extends RowData> {
  columns: readonly DataColumn<Row>[];
  rows: readonly Row[];
  rowKey: (row: Row) => string;
  emptyText: string;
  caption: string;
  layoutKey?: string;
  onRowActivate?: (row: Row) => void;
  selectedRowIds?: readonly string[];
  onSelectionChange?: (ids: string[]) => void;
}

const dataTableFeatures = tableFeatures({
  columnSizingFeature,
  columnResizingFeature,
  columnVisibilityFeature,
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
});

function readTableLayout(key: string, columnIds: readonly string[]) {
  const defaults = {
    columnSizing: {} as ColumnSizingState,
    columnVisibility: {} as ColumnVisibilityState,
  };
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return defaults;
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return defaults;
    const envelope = parsed as {
      version?: unknown;
      columnSizing?: unknown;
      columnVisibility?: unknown;
    };
    if (envelope.version !== 1) return defaults;
    const allowed = new Set(columnIds);
    const sizing: ColumnSizingState = {};
    const visibility: ColumnVisibilityState = {};
    if (envelope.columnSizing && typeof envelope.columnSizing === "object") {
      for (const [id, value] of Object.entries(envelope.columnSizing)) {
        if (
          allowed.has(id) &&
          typeof value === "number" &&
          Number.isFinite(value) &&
          value >= 80 &&
          value <= 900
        )
          sizing[id] = value;
      }
    }
    if (envelope.columnVisibility && typeof envelope.columnVisibility === "object") {
      for (const [id, value] of Object.entries(envelope.columnVisibility)) {
        if (allowed.has(id) && typeof value === "boolean") visibility[id] = value;
      }
    }
    return { columnSizing: sizing, columnVisibility: visibility };
  } catch {
    return defaults;
  }
}

export function DataTable<Row extends RowData>({
  columns,
  rows,
  rowKey,
  emptyText,
  caption,
  layoutKey,
  onRowActivate,
  selectedRowIds,
  onSelectionChange,
}: DataTableProps<Row>) {
  const storageKey = `rip_data_table_v1:${layoutKey ?? caption}`;
  const [initialLayout] = useState(() =>
    readTableLayout(
      storageKey,
      columns.map((column) => column.id),
    ),
  );
  const [sorting, setSorting] = useState<SortingState>([]);
  const [columnSizing, setColumnSizing] = useState<ColumnSizingState>(initialLayout.columnSizing);
  const [columnVisibility, setColumnVisibility] = useState<ColumnVisibilityState>(
    initialLayout.columnVisibility,
  );
  const [internalSelectedIds, setInternalSelectedIds] = useState<string[]>([]);
  const selectionEnabled = selectedRowIds !== undefined || onSelectionChange !== undefined;
  const activeSelectedIds = selectedRowIds ?? internalSelectedIds;
  const selected = useMemo(() => new Set(activeSelectedIds), [activeSelectedIds]);
  const updateSelection = (ids: string[]) => {
    if (selectedRowIds === undefined) setInternalSelectedIds(ids);
    onSelectionChange?.(ids);
  };
  const tableColumns = useMemo<ColumnDef<typeof dataTableFeatures, Row>[]>(
    () =>
      columns.map((column) => ({
        id: column.id,
        accessorFn: (row) =>
          column.sortValue
            ? column.sortValue(row)
            : ((row as Record<string, unknown>)[column.id] as string | number | null | undefined),
        header: column.label,
        enableSorting: true,
        enableHiding: true,
        enableResizing: true,
        size: Number.parseInt(column.width ?? "", 10) || 180,
        minSize: 80,
        maxSize: 900,
        cell: (context) =>
          column.render ? column.render(context.row.original) : String(context.getValue() ?? "-"),
      })),
    [columns],
  );
  const table = useTable({
    features: dataTableFeatures,
    columns: tableColumns,
    data: rows,
    getRowId: (row) => rowKey(row),
    state: { sorting, columnSizing, columnVisibility },
    onSortingChange: setSorting,
    onColumnSizingChange: setColumnSizing,
    onColumnVisibilityChange: setColumnVisibility,
    columnResizeMode: "onChange",
  });

  useEffect(() => {
    try {
      if (Object.keys(columnSizing).length === 0 && Object.keys(columnVisibility).length === 0) {
        localStorage.removeItem(storageKey);
      } else {
        localStorage.setItem(
          storageKey,
          JSON.stringify({ version: 1, columnSizing, columnVisibility }),
        );
      }
    } catch {
      // Table remains usable when browser storage is unavailable.
    }
  }, [columnSizing, columnVisibility, storageKey]);

  const resetLayout = () => {
    setColumnSizing({});
    setColumnVisibility({});
    try {
      localStorage.removeItem(storageKey);
    } catch {
      /* no-op */
    }
  };

  return (
    <div className="overflow-x-auto rounded-surface border border-edge bg-base-100">
      <div className="flex items-center justify-end gap-2 border-b border-edge px-3 py-2">
        <details className="relative">
          <summary className="btn btn-ghost btn-xs cursor-pointer">{translateUi("列设置")}</summary>
          <div className="absolute right-0 z-20 mt-1 min-w-48 rounded-surface border border-edge bg-base-100 p-3 shadow-lg">
            <p className="mb-2 text-xs font-semibold">{translateUi("显示列")}</p>
            {table.getAllLeafColumns().map((column) => (
              <label key={column.id} className="flex items-center gap-2 py-1 text-xs">
                <input
                  type="checkbox"
                  className="checkbox checkbox-xs"
                  checked={column.getIsVisible()}
                  disabled={
                    !column.getCanHide() ||
                    (column.getIsVisible() && table.getVisibleLeafColumns().length === 1)
                  }
                  onChange={column.getToggleVisibilityHandler()}
                />
                {String(column.columnDef.header ?? column.id)}
              </label>
            ))}
            <button
              type="button"
              className="btn btn-ghost btn-xs mt-2 w-full"
              onClick={resetLayout}
            >
              恢复默认
            </button>
          </div>
        </details>
        <button type="button" className="btn btn-ghost btn-xs" onClick={resetLayout}>
          重置列布局
        </button>
      </div>
      <table className="table table-sm data-table" style={{ minWidth: table.getTotalSize() }}>
        <caption className="sr-only">{caption}</caption>
        <thead>
          {table.getHeaderGroups().map((group) => (
            <tr key={group.id}>
              {selectionEnabled && (
                <th scope="col" className="w-10">
                  <input
                    type="checkbox"
                    className="checkbox checkbox-xs"
                    aria-label={translateUi("选择全部行")}
                    checked={
                      table.getRowModel().rows.length > 0 &&
                      table.getRowModel().rows.every((row) => selected.has(row.id))
                    }
                    ref={(node) => {
                      if (node)
                        node.indeterminate =
                          table.getRowModel().rows.some((row) => selected.has(row.id)) &&
                          !table.getRowModel().rows.every((row) => selected.has(row.id));
                    }}
                    onChange={(event) =>
                      updateSelection(
                        event.target.checked ? table.getRowModel().rows.map((row) => row.id) : [],
                      )
                    }
                  />
                </th>
              )}
              {group.headers.map((header) => {
                const sorted = header.column.getIsSorted();
                return (
                  <th
                    key={header.id}
                    scope="col"
                    aria-sort={
                      sorted === "asc" ? "ascending" : sorted === "desc" ? "descending" : "none"
                    }
                    style={{ width: header.getSize(), position: "relative" }}
                  >
                    {header.isPlaceholder ? null : (
                      <div className="flex min-w-0 items-center gap-1">
                        <button
                          type="button"
                          className="data-table-sort min-w-0 flex-1 text-left"
                          aria-label={`${String(header.column.columnDef.header ?? header.column.id)} ${translateUi("排序")}`}
                          onClick={header.column.getToggleSortingHandler()}
                        >
                          <table.FlexRender header={header} />
                          <span aria-hidden="true">
                            {sorted === "asc" ? " ↑" : sorted === "desc" ? " ↓" : " ↕"}
                          </span>
                        </button>
                        {header.column.getCanResize() && (
                          <div
                            role="separator"
                            aria-orientation="vertical"
                            aria-label={`调整 ${header.column.id} 列宽`}
                            aria-valuemin={80}
                            aria-valuemax={900}
                            aria-valuenow={Math.round(header.getSize())}
                            tabIndex={0}
                            className="data-table-resize-handle"
                            onMouseDown={header.getResizeHandler()}
                            onTouchStart={header.getResizeHandler()}
                            onKeyDown={(event) => {
                              if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
                                event.preventDefault();
                                const delta = event.key === "ArrowRight" ? 16 : -16;
                                table.setColumnSizing((old) => ({
                                  ...old,
                                  [header.column.id]: Math.max(
                                    80,
                                    Math.min(900, header.getSize() + delta),
                                  ),
                                }));
                              } else if (event.key === "Home") {
                                event.preventDefault();
                                header.column.resetSize();
                              }
                            }}
                          />
                        )}
                      </div>
                    )}
                  </th>
                );
              })}
            </tr>
          ))}
        </thead>
        <tbody>
          {table.getRowModel().rows.length === 0 ? (
            <tr>
              <td
                colSpan={table.getVisibleLeafColumns().length + (selectionEnabled ? 1 : 0)}
                className="py-8 text-center text-sm text-muted"
              >
                {emptyText}
              </td>
            </tr>
          ) : (
            table.getRowModel().rows.map((row) => {
              const id = row.id;
              const isSelected = selected.has(id);
              return (
                <tr
                  key={row.id}
                  tabIndex={onRowActivate ? 0 : undefined}
                  aria-selected={selectionEnabled ? isSelected : undefined}
                  aria-label={onRowActivate ? `打开 ${row.id}` : undefined}
                  className={`${onRowActivate ? "cursor-pointer " : ""}${isSelected ? "bg-base-200 " : ""}hover:bg-base-200/60 focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary`}
                  onClick={
                    onRowActivate
                      ? (event) => {
                          if (
                            event.target instanceof HTMLElement &&
                            ["BUTTON", "INPUT", "A"].includes(event.target.tagName)
                          )
                            return;
                          onRowActivate(row.original);
                        }
                      : undefined
                  }
                  onKeyDown={
                    onRowActivate
                      ? (event) => {
                          if (
                            (event.key === "Enter" || event.key === " ") &&
                            !(
                              event.target instanceof HTMLElement &&
                              ["BUTTON", "INPUT", "A"].includes(event.target.tagName)
                            )
                          ) {
                            event.preventDefault();
                            onRowActivate(row.original);
                          }
                        }
                      : undefined
                  }
                >
                  {selectionEnabled && (
                    <td className="w-10">
                      <input
                        type="checkbox"
                        className="checkbox checkbox-xs"
                        aria-label={`${translateUi("选择行")} ${row.id}`}
                        checked={isSelected}
                        disabled={selectedRowIds !== undefined && !onSelectionChange}
                        onClick={(event) => event.stopPropagation()}
                        onChange={(event) => {
                          const next = new Set(selected);
                          if (event.target.checked) next.add(row.id);
                          else next.delete(row.id);
                          updateSelection([...next]);
                        }}
                      />
                    </td>
                  )}
                  {row.getVisibleCells().map((cell) => (
                    <td
                      key={cell.id}
                      style={{ width: cell.column.getSize(), maxWidth: cell.column.getSize() }}
                    >
                      <table.FlexRender cell={cell} />
                    </td>
                  ))}
                </tr>
              );
            })
          )}
        </tbody>
      </table>
    </div>
  );
}

interface FilterSelectProps {
  label: string;
  value: string;
  options: readonly { value: string; label: string }[];
  onChange: (value: string) => void;
}

export function FilterSelect({ label, value, options, onChange }: FilterSelectProps) {
  return (
    <label className="flex items-center gap-1.5 text-xs text-muted">
      {label}
      <select
        className="select select-bordered select-sm w-36"
        aria-label={label}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}

interface FilterSearchProps {
  label: string;
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
}

export function FilterSearch({ label, value, placeholder, onChange }: FilterSearchProps) {
  return (
    <label className="flex items-center gap-1.5 text-xs text-muted">
      {label}
      <input
        type="search"
        className="input input-bordered input-sm w-56"
        aria-label={label}
        placeholder={placeholder}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
}

interface CountChipsProps {
  entries: readonly (readonly [string, number])[];
  tones?: Record<string, string>;
  label: string;
  outline?: boolean;
}

export function CountChips({ entries, tones = {}, label, outline = false }: CountChipsProps) {
  if (entries.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-1" aria-label={label}>
      {entries.map(([key, count]) => (
        <span
          key={key}
          className={`badge badge-sm ${outline ? "badge-outline" : (tones[key] ?? "badge-ghost")}`}
        >
          {key} {count}
        </span>
      ))}
    </div>
  );
}
