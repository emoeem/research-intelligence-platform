import { useState } from "react";
import { apiSend, ApiError } from "../lib/api";
import { PageHeader } from "../components/ui";
import { useUiText } from "../i18n";
import { DiffViewer } from "../components/editor/DiffViewer";
import { SuccessState } from "../components/feedback/StatePanels";

type CompareMode = "text" | "json" | "header" | "hex";

export function ComparerPage() {
  const ui = useUiText();
  const [mode, setMode] = useState<CompareMode>("text");
  const [left, setLeft] = useState("");
  const [right, setRight] = useState("");
  const [result, setResult] = useState<unknown>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  async function compare() {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(
        await apiSend<unknown>("POST", "/api/wsw/compare", {
          mode,
          left: { text: left },
          right: { text: right },
        }),
      );
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="wsw-workspace-page" data-testid="wsw-comparer-page">
      <PageHeader
        title={ui("Comparer")}
        actions={<span className="badge badge-outline">{ui("本地数据比较")}</span>}
      />
      <p className="text-sm text-muted">
        {ui(
          "比较两段用户提供的文本，通过本机 rp-wsw 处理。当前提供 Text、JSON、Header、Hex 四种模式；不自动读取 History 内容。",
        )}
      </p>
      <div className="mb-3 flex flex-wrap items-center gap-3">
        <label className="wsw-field">
          <span>{ui("比较模式")}</span>
          <select
            aria-label={ui("比较模式")}
            className="select select-bordered select-sm"
            value={mode}
            onChange={(e) => setMode(e.target.value as CompareMode)}
          >
            {(["text", "json", "header", "hex"] as const).map((v) => (
              <option key={v}>{v}</option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="btn btn-primary btn-sm"
          disabled={busy || !left || !right}
          onClick={() => void compare()}
        >
          {busy ? ui("比较中…") : ui("比较")}
        </button>
      </div>
      <div className="grid gap-3 xl:grid-cols-2">
        <label className="wsw-field rounded-surface border border-edge bg-base-100 p-3">
          <span>{ui("左侧内容")}</span>
          <textarea
            aria-label={ui("左侧内容")}
            className="textarea textarea-bordered min-h-64 w-full font-mono text-xs"
            value={left}
            onChange={(e) => setLeft(e.target.value)}
          />
        </label>
        <label className="wsw-field rounded-surface border border-edge bg-base-100 p-3">
          <span>{ui("右侧内容")}</span>
          <textarea
            aria-label={ui("右侧内容")}
            className="textarea textarea-bordered min-h-64 w-full font-mono text-xs"
            value={right}
            onChange={(e) => setRight(e.target.value)}
          />
        </label>
      </div>
      {mode === "text" && left.length > 0 && right.length > 0 ? (
        <section className="mt-4 rounded-surface border border-edge bg-base-100 p-4">
          <h2 className="mb-2 text-sm font-semibold">{ui("本地逐行差异预览")}</h2>
          <p className="mb-3 text-xs text-muted">
            {ui("此预览仅比较当前输入文本，不替代服务端比较结果，也不代表原始网络字节。")}
          </p>
          <DiffViewer
            before={left}
            after={right}
            beforeLabel={ui("左侧内容")}
            afterLabel={ui("右侧内容")}
          />
        </section>
      ) : null}
      <section className="mt-4 rounded-surface border border-edge bg-base-100 p-4">
        <h2 className="mb-2 text-sm font-semibold">{ui("比较结果")}</h2>
        {error ? (
          <pre role="alert" className="whitespace-pre-wrap text-xs text-error">
            {error}
          </pre>
        ) : result !== null ? (
          <>
            <SuccessState label={ui("比较完成")} />
            <pre className="max-h-[32rem] overflow-auto whitespace-pre-wrap break-all rounded-control bg-base-200 p-3 text-xs">
              {JSON.stringify(result, null, 2)}
            </pre>
          </>
        ) : (
          <p className="text-xs text-muted">{ui("输入两段内容并点击比较。")}</p>
        )}
      </section>
    </div>
  );
}
