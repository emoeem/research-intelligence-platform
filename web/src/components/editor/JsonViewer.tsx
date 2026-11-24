import { useMemo } from "react";
import { useUiText } from "../../i18n";

type JsonPrimitive = string | number | boolean | null;
function JsonNode({
  value,
  depth,
  ancestors,
  maxDepth,
  maxEntries,
}: {
  value: unknown;
  depth: number;
  ancestors: readonly object[];
  maxDepth: number;
  maxEntries: number;
}) {
  const ui = useUiText();
  if (value === null) return <span className="text-muted">null</span>;
  if (typeof value === "string") return <span className="text-success break-all">"{value}"</span>;
  if (typeof value === "number" || typeof value === "boolean")
    return <span className="text-info">{String(value)}</span>;
  if (typeof value !== "object") return <span className="text-muted">{String(value)}</span>;
  if (ancestors.includes(value)) return <span className="text-warning">[循环引用]</span>;
  const nextAncestors = [...ancestors, value];
  const entries = Array.isArray(value)
    ? value.map((item, index) => [String(index), item] as const)
    : Object.entries(value);
  const label = Array.isArray(value) ? `Array(${entries.length})` : `Object(${entries.length})`;
  if (depth >= maxDepth)
    return (
      <span className="text-muted">
        {label} · {ui("已达到嵌套深度上限")}
      </span>
    );
  return (
    <details open={depth < 1} className="json-viewer-node">
      <summary className="cursor-pointer text-xs font-medium">{label}</summary>
      <div className="ml-4 border-l border-edge pl-3">
        {entries.slice(0, maxEntries).map(([key, child]) => (
          <div
            key={key}
            className="grid grid-cols-[minmax(5rem,auto)_minmax(0,1fr)] gap-2 py-0.5 text-xs"
          >
            <span className="break-all text-muted">{Array.isArray(value) ? `[${key}]` : key}</span>
            <span className="min-w-0">
              <JsonNode
                value={child}
                depth={depth + 1}
                ancestors={nextAncestors}
                maxDepth={maxDepth}
                maxEntries={maxEntries}
              />
            </span>
          </div>
        ))}
        {entries.length > maxEntries && (
          <p className="text-xs text-muted">
            {ui("其余条目已折叠")} ({entries.length - maxEntries})
          </p>
        )}
      </div>
    </details>
  );
}

export function JsonViewer({
  value,
  label = "JSON 查看器",
  maxDepth = 8,
  maxEntries = 200,
  maxBytes = 1_000_000,
}: {
  value: unknown;
  label?: string;
  maxDepth?: number;
  maxEntries?: number;
  maxBytes?: number;
}) {
  const ui = useUiText();
  const serialized = useMemo(() => {
    try {
      const output = JSON.stringify(value, null, 2);
      return typeof output === "string" ? output : null;
    } catch {
      return null;
    }
  }, [value]);
  const byteLength = serialized ? new TextEncoder().encode(serialized).byteLength : 0;
  const tooLarge = byteLength > maxBytes;
  const copy = async () => {
    if (!serialized) return;
    try {
      await navigator.clipboard.writeText(serialized);
    } catch {
      /* clipboard is optional */
    }
  };
  if (serialized === null)
    return (
      <p role="alert" className="text-xs text-error">
        {ui("无法序列化此 JSON 值")}
      </p>
    );
  if (tooLarge)
    return (
      <p role="status" className="text-xs text-warning">
        {ui("JSON 内容过大，已停止树形展开。请缩小数据范围。")}
      </p>
    );
  const safeValue = value as JsonPrimitive | Record<string, unknown> | unknown[];
  return (
    <section className="json-viewer rounded-surface border border-edge p-3" aria-label={ui(label)}>
      <div className="mb-2 flex items-center justify-between gap-2">
        <span className="text-xs text-muted">{serialized.length.toLocaleString()} B</span>
        <button type="button" className="btn btn-ghost btn-xs" onClick={() => void copy()}>
          {ui("复制 JSON")}
        </button>
      </div>
      <JsonNode
        value={safeValue}
        depth={0}
        ancestors={[]}
        maxDepth={maxDepth}
        maxEntries={maxEntries}
      />
    </section>
  );
}
