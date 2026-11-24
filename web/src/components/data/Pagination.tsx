import { useUiText } from "../../i18n";

/** Cursor pagination: deliberately has no page-number/offset semantics. */
export function Pagination({
  loadedCount,
  totalCount,
  hasNext,
  loading = false,
  onNext,
  label = "分页",
  nextLabel = "下一页",
  loadingLabel = "加载中…",
}: {
  loadedCount: number;
  totalCount?: number;
  hasNext: boolean;
  loading?: boolean;
  onNext: () => void;
  label?: string;
  nextLabel?: string;
  loadingLabel?: string;
}) {
  const ui = useUiText();
  const summary =
    typeof totalCount === "number"
      ? `${ui("已加载")} ${loadedCount} / ${totalCount}`
      : `${ui("已加载")} ${loadedCount}`;
  return (
    <nav className="flex flex-wrap items-center justify-between gap-2 py-3" aria-label={ui(label)}>
      <p className="text-xs text-muted" aria-live="polite">
        {summary}
      </p>
      {hasNext && (
        <button type="button" className="btn btn-sm" disabled={loading} onClick={onNext}>
          {loading ? ui(loadingLabel) : ui(nextLabel)}
        </button>
      )}
    </nav>
  );
}
