import { useState } from "react";
import { apiSend, ApiError } from "../lib/api";
import { PageHeader } from "../components/ui";
import { useUiText } from "../i18n";

const OPERATIONS = [
  "url_encode",
  "url_decode",
  "b64_encode",
  "b64_decode",
  "b64url_encode",
  "b64url_decode",
  "hex_encode",
  "hex_decode",
  "html_entity_encode",
  "html_entity_decode",
  "unicode_escape",
  "unicode_unescape",
  "json_pretty",
  "json_compact",
  "json_validate",
  "jwt_decode",
  "sha256",
] as const;
type Operation = (typeof OPERATIONS)[number];

export function DecoderPage() {
  const ui = useUiText();
  const [input, setInput] = useState("");
  const [operation, setOperation] = useState<Operation>("b64_decode");
  const [result, setResult] = useState<unknown>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [inputBase64, setInputBase64] = useState(false);
  async function decode() {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(
        await apiSend<unknown>("POST", "/api/wsw/decode", {
          [inputBase64 ? "input_base64" : "input"]: input,
          ops: [operation],
        }),
      );
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="wsw-workspace-page" data-testid="wsw-decoder-page">
      <PageHeader
        title={ui("Decoder")}
        actions={<span className="badge badge-outline">{ui("本地数据转换")}</span>}
      />
      <p className="text-sm text-muted">
        {ui(
          "使用本机 rp-wsw Decoder 转换输入内容，不会向目标网站发送请求。仅开放后端当前支持的 17 种操作。",
        )}
      </p>
      <section className="grid gap-4 xl:grid-cols-2">
        <div className="rounded-surface border border-edge bg-base-100 p-4">
          <label className="wsw-field">
            <span>{ui("转换操作")}</span>
            <select
              aria-label={ui("转换操作")}
              className="select select-bordered select-sm"
              value={operation}
              onChange={(e) => setOperation(e.target.value as Operation)}
            >
              {OPERATIONS.map((op) => (
                <option key={op}>{op}</option>
              ))}
            </select>
          </label>
          <label className="mt-3 flex items-center gap-2 text-xs text-muted">
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={inputBase64}
              onChange={(e) => setInputBase64(e.target.checked)}
            />
            {ui("输入为 Base64 字节")}
          </label>
          <label className="wsw-field mt-3">
            <span>{ui("输入")}</span>
            <textarea
              aria-label={ui("输入")}
              className="textarea textarea-bordered min-h-64 w-full font-mono text-xs"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder={ui("粘贴需要转换的内容…")}
            />
          </label>
          <button
            className="btn btn-primary btn-sm mt-3"
            type="button"
            disabled={busy || input.length === 0}
            onClick={() => void decode()}
          >
            {busy ? ui("处理中…") : ui("执行转换")}
          </button>
        </div>
        <div className="rounded-surface border border-edge bg-base-100 p-4">
          <h2 className="mb-3 text-sm font-semibold">{ui("转换结果")}</h2>
          {error ? (
            <pre role="alert" className="whitespace-pre-wrap text-xs text-error">
              {error}
            </pre>
          ) : result !== null ? (
            <pre className="max-h-[32rem] overflow-auto whitespace-pre-wrap break-all rounded-control bg-base-200 p-3 text-xs">
              {JSON.stringify(result, null, 2)}
            </pre>
          ) : (
            <p className="text-xs text-muted">{ui("执行转换后，结果会显示在这里。")}</p>
          )}
        </div>
      </section>
    </div>
  );
}
