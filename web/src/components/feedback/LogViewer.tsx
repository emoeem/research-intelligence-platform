import { useId, useMemo, useState } from "react";
import { useUiText } from "../../i18n";
import { VirtualList } from "../VirtualList";
import { EmptyState } from "./StatePanels";
export interface LogEntry {
  id: string;
  timestamp?: string;
  level?: string;
  source?: string;
  message: string;
}
const LEVELS = ["all", "debug", "info", "warn", "error"] as const;
export function LogViewer({
  entries,
  label = "运行日志",
  height = 420,
  emptyText = "暂无日志",
  offline = false,
}: {
  entries: readonly LogEntry[];
  label?: string;
  height?: number;
  emptyText?: string;
  offline?: boolean;
}) {
  const ui = useUiText();
  const id = useId();
  const [level, setLevel] = useState<(typeof LEVELS)[number]>("all");
  const [query, setQuery] = useState("");
  const [followTail, setFollowTail] = useState(true);
  const filtered = useMemo(
    () =>
      entries.filter(
        (entry) =>
          (level === "all" || entry.level?.toLowerCase() === level) &&
          (!query ||
            `${entry.source ?? ""} ${entry.message}`.toLowerCase().includes(query.toLowerCase())),
      ),
    [entries, level, query],
  );
  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      /* clipboard may be unavailable; content remains readable */
    }
  };
  return (
    <section className="log-viewer flex min-h-0 flex-col gap-2" aria-label={ui(label)}>
      <div className="flex flex-wrap items-center gap-2">
        <label className="sr-only" htmlFor={`${id}-level`}>
          {ui("日志级别")}
        </label>
        <select
          id={`${id}-level`}
          className="select select-bordered select-sm"
          value={level}
          onChange={(event) => setLevel(event.target.value as (typeof LEVELS)[number])}
        >
          {LEVELS.map((item) => (
            <option key={item} value={item}>
              {item === "all" ? ui("全部级别") : item.toUpperCase()}
            </option>
          ))}
        </select>
        <input
          className="input input-bordered input-sm min-w-40 flex-1"
          type="search"
          aria-label={ui("搜索日志")}
          placeholder={ui("搜索日志内容")}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <label className="flex items-center gap-2 text-xs">
          <input
            type="checkbox"
            className="checkbox checkbox-xs"
            checked={followTail}
            onChange={(event) => setFollowTail(event.target.checked)}
          />
          {ui("跟随最新")}
        </label>
        <button
          type="button"
          className="btn btn-ghost btn-xs"
          onClick={() =>
            void copy(
              filtered
                .map((entry) =>
                  [entry.timestamp, entry.level?.toUpperCase(), entry.source, entry.message]
                    .filter(Boolean)
                    .join(" "),
                )
                .join("\n"),
            )
          }
          disabled={filtered.length === 0}
        >
          {ui("复制筛选结果")}
        </button>
      </div>
      {offline && (
        <p role="status" className="text-xs text-warning">
          {ui("日志源离线，以下为当前已缓存记录。")}
        </p>
      )}
      {filtered.length === 0 ? (
        <EmptyState label={emptyText} />
      ) : (
        <div style={{ height, minHeight: 120 }}>
          <VirtualList
            itemCount={filtered.length}
            rowHeight={30}
            itemKey={(index) => filtered[index]?.id ?? index}
            scrollToEndKey={followTail ? filtered.length : undefined}
            className="h-full"
            testId="log-viewer-list"
            renderItem={(index) => {
              const entry = filtered[index];
              if (!entry) return null;
              const tone =
                entry.level?.toLowerCase() === "error"
                  ? "text-error"
                  : entry.level?.toLowerCase() === "warn"
                    ? "text-warning"
                    : "text-muted";
              return (
                <div
                  className="flex h-[30px] items-center gap-2 border-b border-edge-soft px-2 font-mono text-xs"
                  data-testid="log-viewer-row"
                >
                  <time className="w-36 shrink-0 text-muted">{entry.timestamp ?? ""}</time>
                  <span className={`w-12 shrink-0 ${tone}`}>
                    {entry.level?.toUpperCase() ?? "LOG"}
                  </span>
                  <span className="w-24 shrink-0 truncate text-muted">{entry.source ?? ""}</span>
                  <span className="min-w-0 flex-1 truncate" title={entry.message}>
                    {entry.message}
                  </span>
                  <button
                    type="button"
                    className="btn btn-ghost btn-xs"
                    aria-label={`复制日志 ${entry.id}`}
                    onClick={() => void copy(entry.message)}
                  >
                    {ui("复制")}
                  </button>
                </div>
              );
            }}
          />
        </div>
      )}
    </section>
  );
}
