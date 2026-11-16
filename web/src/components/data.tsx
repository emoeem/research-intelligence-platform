/** 数据表与过滤条基元（P5-1，Phase 5 页面族共用，差距清单 G9 收口）：
 *  - DataTable：列配置驱动的桌面表格（行 key、空态、可选行主列渲染）。
 *    样式沿用 token.css/shell.css 表面变量（rounded-surface/border-edge），
 *    不引入新色板；行高按长时间桌面操作口径（py-2.5）。
 *  - FilterSelect：带 aria-label 的下拉过滤（daisyUI select，受控）。
 *  - FilterSearch：带 aria-label 的文本过滤输入（受控，清空按钮）。
 *  - CountChips：计数芯片组（by_state/by_type 面，状态徽标 tone 注入）。
 *
 *  本文件零业务 import：不发请求、不 import workspaces.ts，只承接已取好的
 *  数据；分页由调用方决定（服务端 limit 或调用方切片），本组件不隐藏分母。 */

import type { ReactNode } from "react";

export interface DataColumn<Row> {
  /** 列标识与表头。 */
  id: string;
  label: string;
  /** 列宽 class（缺省自适应）。 */
  width?: string;
  /** 单元格渲染（缺省 String(row[id])）。 */
  render?: (row: Row) => ReactNode;
}

interface DataTableProps<Row> {
  columns: readonly DataColumn<Row>[];
  rows: readonly Row[];
  /** 行 key 提取（唯一性由调用方保证）。 */
  rowKey: (row: Row) => string;
  /** 空态文案（rows 为空时渲染 EmptyPanel 同风格行）。 */
  emptyText: string;
  /** 无障碍：表格标题（caption / sr-only）。 */
  caption: string;
  /** 行点击（如打开详情）；传了才启用行级 button 语义。 */
  onRowActivate?: (row: Row) => void;
}

export function DataTable<Row>({
  columns,
  rows,
  rowKey,
  emptyText,
  caption,
  onRowActivate,
}: DataTableProps<Row>) {
  return (
    <div className="overflow-x-auto rounded-surface border border-edge bg-base-100">
      <table className="table table-sm">
        <caption className="sr-only">{caption}</caption>
        <thead>
          <tr>
            {columns.map((column) => (
              <th key={column.id} scope="col" className={column.width}>
                {column.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 ? (
            <tr>
              <td colSpan={columns.length} className="py-8 text-center text-sm text-muted">
                {emptyText}
              </td>
            </tr>
          ) : (
            rows.map((row) => (
              <tr
                key={rowKey(row)}
                className={onRowActivate ? "cursor-pointer hover:bg-base-200/60" : undefined}
                onClick={onRowActivate ? () => onRowActivate(row) : undefined}
              >
                {columns.map((column) => (
                  <td key={column.id}>
                    {column.render
                      ? column.render(row)
                      : String((row as Record<string, unknown>)[column.id] ?? "-")}
                  </td>
                ))}
              </tr>
            ))
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
  /** 状态到徽标 tone class 的映射（调用方注入，缺省 badge-ghost）。 */
  tones?: Record<string, string>;
  /** aria-label 前缀（如「状态计数」）。 */
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
