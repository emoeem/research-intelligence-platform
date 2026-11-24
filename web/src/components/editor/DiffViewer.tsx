import { useMemo, useState } from "react";
import { useUiText } from "../../i18n";
type DiffLine = {
  kind: "same" | "add" | "remove";
  oldLine?: number;
  newLine?: number;
  text: string;
};
const MAX_DIFF_CELLS = 1_000_000;
function buildDiff(before: string, after: string): DiffLine[] | null {
  const left = before.replace(/\r\n/g, "\n").split("\n");
  const right = after.replace(/\r\n/g, "\n").split("\n");
  if (before.length + after.length > 2_000_000) return null;
  if ((left.length + 1) * (right.length + 1) > MAX_DIFF_CELLS) return null;
  const matrix = Array.from({ length: left.length + 1 }, () => new Uint32Array(right.length + 1));
  for (let i = left.length - 1; i >= 0; i--)
    for (let j = right.length - 1; j >= 0; j--)
      matrix[i]![j] =
        left[i] === right[j]
          ? matrix[i + 1]![j + 1]! + 1
          : Math.max(matrix[i + 1]![j]!, matrix[i]![j + 1]!);
  const result: DiffLine[] = [];
  let i = 0,
    j = 0,
    oldLine = 1,
    newLine = 1;
  while (i < left.length || j < right.length) {
    if (i < left.length && j < right.length && left[i] === right[j]) {
      result.push({ kind: "same", oldLine: oldLine++, newLine: newLine++, text: left[i]! });
      i++;
      j++;
    } else if (
      j < right.length &&
      (i === left.length || matrix[i]![j + 1]! >= matrix[i + 1]![j]!)
    ) {
      result.push({ kind: "add", newLine: newLine++, text: right[j]! });
      j++;
    } else {
      result.push({ kind: "remove", oldLine: oldLine++, text: left[i]! });
      i++;
    }
  }
  return result;
}
export function DiffViewer({
  before,
  after,
  beforeLabel = "原始版本",
  afterLabel = "修改版本",
  maxHeight = 520,
}: {
  before: string;
  after: string;
  beforeLabel?: string;
  afterLabel?: string;
  maxHeight?: number;
}) {
  const ui = useUiText();
  const [showUnchanged, setShowUnchanged] = useState(true);
  const diff = useMemo(() => buildDiff(before, after), [before, after]);
  const visible = useMemo(
    () => diff?.filter((line) => showUnchanged || line.kind !== "same"),
    [diff, showUnchanged],
  );
  if (!diff)
    return (
      <div className="rounded-surface border border-edge p-4 text-sm text-muted" role="status">
        {ui("差异内容过大，已停止逐行计算。请缩小比较范围后重试。")}
      </div>
    );
  const added = diff.filter((line) => line.kind === "add").length;
  const removed = diff.filter((line) => line.kind === "remove").length;
  return (
    <section
      className="diff-viewer rounded-surface border border-edge overflow-hidden"
      aria-label={ui("文本差异")}
    >
      <header className="flex flex-wrap items-center justify-between gap-2 border-b border-edge px-3 py-2 text-xs">
        <div className="flex gap-3">
          <span>{ui(beforeLabel)}</span>
          <span className="text-success" aria-label={`${ui("新增")} ${added} ${ui("行")}`}>
            +{added}
          </span>
          <span className="text-error" aria-label={`${ui("删除")} ${removed} ${ui("行")}`}>
            −{removed}
          </span>
          <span>{ui(afterLabel)}</span>
        </div>
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            className="checkbox checkbox-xs"
            checked={showUnchanged}
            onChange={(event) => setShowUnchanged(event.target.checked)}
          />
          {ui("显示未变化行")}
        </label>
      </header>
      <div style={{ maxHeight, overflow: "auto" }} role="table" aria-label={ui("逐行差异")}>
        {visible?.map((line, index) => (
          <div
            key={`${line.kind}-${line.oldLine ?? "x"}-${line.newLine ?? "y"}-${index}`}
            role="row"
            className={`grid grid-cols-[3.5rem_3.5rem_1.5rem_minmax(0,1fr)] font-mono text-xs ${line.kind === "add" ? "bg-success/10" : line.kind === "remove" ? "bg-error/10" : ""}`}
          >
            <span role="cell" className="select-none px-2 text-right text-muted">
              {line.oldLine ?? ""}
            </span>
            <span role="cell" className="select-none px-2 text-right text-muted">
              {line.newLine ?? ""}
            </span>
            <span
              role="cell"
              className={
                line.kind === "add"
                  ? "text-success"
                  : line.kind === "remove"
                    ? "text-error"
                    : "text-muted"
              }
            >
              {line.kind === "add" ? "+" : line.kind === "remove" ? "−" : " "}
            </span>
            <code role="cell" className="whitespace-pre-wrap break-all px-2">
              {line.text || " "}
            </code>
          </div>
        ))}
      </div>
    </section>
  );
}
