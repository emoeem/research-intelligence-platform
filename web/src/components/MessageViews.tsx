import { useMemo, useState } from "react";

/**
 * 消息详情六视图共享组件（WSW-3-ADR W3-D5：从 ResearchEvidencePage
 * Raw/Pretty `<pre>` 雏形抽取——03 UI 审计 :171 明文「需抽成通用
 * MessageViewer/MessageEditor」；零新依赖；深浅主题沿 RIP Token）。
 *
 * 六视图：Raw / Pretty（JSON 美化）/ Headers / Params / Body / Hex。
 * 大文本纪律（W3-D5 表）：详情体 >256KiB 时折叠为按需加载提示（防一次性
 * 注入 DOM）；Hex 分块渲染（每块 64 行）。
 */

export const BODY_PREVIEW_CAP = 256 * 1024;
const HEX_BLOCK_ROWS = 64;
const HEX_BYTES_PER_ROW = 16;

export type MessageViewKind = "raw" | "pretty" | "headers" | "params" | "body" | "hex";

export interface HeaderPair {
  name: string;
  value: string;
}

const VIEW_TABS: Array<{ kind: MessageViewKind; label: string }> = [
  { kind: "raw", label: "Raw" },
  { kind: "pretty", label: "Pretty" },
  { kind: "headers", label: "Headers" },
  { kind: "params", label: "Params" },
  { kind: "body", label: "Body" },
  { kind: "hex", label: "Hex" },
];

/** URL query 派生（path 上的 ?a=b&c=d → 键值行；无 query = 空表）。 */
export function parseQueryParams(target: string | null | undefined): HeaderPair[] {
  if (!target) return [];
  const idx = target.indexOf("?");
  if (idx < 0) return [];
  const qs = target.slice(idx + 1);
  const out: HeaderPair[] = [];
  for (const pair of qs.split("&")) {
    if (!pair) continue;
    const eq = pair.indexOf("=");
    const key = eq >= 0 ? pair.slice(0, eq) : pair;
    const value = eq >= 0 ? pair.slice(eq + 1) : "";
    out.push({ name: decodeSafe(key), value: decodeSafe(value) });
  }
  return out;
}

function decodeSafe(s: string): string {
  try {
    return decodeURIComponent(s.replace(/\+/g, " "));
  } catch {
    return s;
  }
}

/** 体字节 → 文本（base64 解码失败 = 二进制面提示）。 */
export function decodeBodyBase64(b64: string | null | undefined): Uint8Array | null {
  if (!b64) return null;
  try {
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i += 1) bytes[i] = bin.charCodeAt(i);
    return bytes;
  } catch {
    return null;
  }
}

function toHexDump(bytes: Uint8Array): string {
  const lines: string[] = [];
  for (let offset = 0; offset < bytes.length; offset += HEX_BYTES_PER_ROW) {
    const chunk = bytes.subarray(offset, offset + HEX_BYTES_PER_ROW);
    const hex = Array.from(chunk)
      .map((b) => b.toString(16).padStart(2, "0"))
      .join(" ")
      .padEnd(HEX_BYTES_PER_ROW * 3 - 1, " ");
    const ascii = Array.from(chunk)
      .map((b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : "."))
      .join("");
    lines.push(`${offset.toString(16).padStart(8, "0")}  ${hex}  |${ascii}|`);
  }
  return lines.join("\n");
}

/** 六视图标签条 + 视图本体。原始行面与脱敏展示分离（PLAN :543）——本组件
 * 只渲染 API 面给到的投影，凭据头值掩码由 rp_server/wsw_message::redact
 * 链路负责（展示层不二次脱敏也不还原）。 */
export function MessageViews(props: {
  headers?: HeaderPair[];
  params?: HeaderPair[];
  /** Raw 体文本（展示投影；空串 = 无体）。 */
  rawBody: string;
  /** 体是否二进制（Raw/Pretty 视图给出提示而非乱码）。 */
  bodyIsBinary?: boolean;
  /** 体是否超预览帽（256KiB）——明示截断（W3-D5 大文本纪律）。 */
  bodyTruncated?: boolean;
  /** 可选中视图（外层受控，如详情默认 Raw）。 */
  initialView?: MessageViewKind;
  testIdPrefix?: string;
}) {
  const {
    headers = [],
    params = [],
    rawBody,
    bodyIsBinary = false,
    bodyTruncated = false,
    initialView = "raw",
    testIdPrefix = "wsw-message-view",
  } = props;
  const [view, setView] = useState<MessageViewKind>(initialView);
  const [hexBlock, setHexBlock] = useState(0);

  const pretty = useMemo(() => {
    const text = rawBody.trim();
    if (!text) return "";
    try {
      return JSON.stringify(JSON.parse(text), null, 2);
    } catch {
      return "";
    }
  }, [rawBody]);

  return (
    <div data-testid={testIdPrefix}>
      <div role="tablist" className="tabs" style={{ marginBottom: "0.5rem" }}>
        {VIEW_TABS.map((tab) => (
          <button
            key={tab.kind}
            type="button"
            role="tab"
            aria-selected={view === tab.kind}
            className={`tab ${view === tab.kind ? "tab-active" : ""}`}
            data-testid={`${testIdPrefix}-tab-${tab.kind}`}
            onClick={() => setView(tab.kind)}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {view === "raw" && (
        <pre data-testid={`${testIdPrefix}-raw`} className="text-xs overflow-auto max-h-96">
          {bodyIsBinary && !rawBody
            ? "（二进制体：Raw 视图不渲染非 UTF-8 字节；切 Hex 视图）"
            : rawBody || "（空体）"}
          {bodyTruncated ? "\n…（超 256KiB 预览帽，截断明示）" : ""}
        </pre>
      )}
      {view === "pretty" && (
        <pre data-testid={`${testIdPrefix}-pretty`} className="text-xs overflow-auto max-h-96">
          {pretty || "（非 JSON 体或空体——切 Raw/Hex）"}
        </pre>
      )}
      {view === "headers" && (
        <table data-testid={`${testIdPrefix}-headers`}>
          <thead>
            <tr>
              <th>头名</th>
              <th>值</th>
            </tr>
          </thead>
          <tbody>
            {headers.length === 0 && (
              <tr>
                <td colSpan={2} className="muted">
                  （无头）
                </td>
              </tr>
            )}
            {headers.map((h, i) => (
              <tr key={`${h.name}-${i}`}>
                <td>{h.name}</td>
                <td className="break-all">{h.value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {view === "params" && (
        <table data-testid={`${testIdPrefix}-params`}>
          <thead>
            <tr>
              <th>参数</th>
              <th>值</th>
            </tr>
          </thead>
          <tbody>
            {params.length === 0 && (
              <tr>
                <td colSpan={2} className="muted">
                  （请求目标无 query 参数）
                </td>
              </tr>
            )}
            {params.map((p, i) => (
              <tr key={`${p.name}-${i}`}>
                <td>{p.name}</td>
                <td className="break-all">{p.value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {view === "body" && (
        <pre data-testid={`${testIdPrefix}-body`} className="text-xs overflow-auto max-h-96">
          {bodyIsBinary
            ? "（二进制体：Body 文本视图不渲染；切 Hex 视图）"
            : rawBody || "（空体）"}
        </pre>
      )}
      {view === "hex" && <HexView rawBody={rawBody} testId={testIdPrefix} block={hexBlock} onBlock={setHexBlock} />}
    </div>
  );
}

function HexView(props: { rawBody: string; testId: string; block: number; onBlock: (n: number) => void }) {
  const { rawBody, testId, block, onBlock } = props;
  // Hex 数据源 = Raw 体文本的 UTF-8 字节（与传输字节的偏差 = capture 面本就
  // 只 tee 文本投影的 lossy 展示——二进制体走 Body 端点 base64 原字节路径）。
  const bytes = useMemo(() => new TextEncoder().encode(rawBody), [rawBody]);
  if (bytes.length === 0) {
    return (
      <pre data-testid={`${testId}-hex`} className="text-xs">
        （无字节可展示）
      </pre>
    );
  }
  const totalRows = Math.ceil(bytes.length / HEX_BYTES_PER_ROW);
  const totalBlocks = Math.ceil(totalRows / HEX_BLOCK_ROWS);
  const start = block * HEX_BLOCK_ROWS * HEX_BYTES_PER_ROW;
  const end = Math.min(bytes.length, start + HEX_BLOCK_ROWS * HEX_BYTES_PER_ROW);
  const dump = toHexDump(bytes.subarray(start, end));
  return (
    <div data-testid={`${testId}-hex`}>
      <pre className="text-xs overflow-auto max-h-96">{dump}</pre>
      {totalBlocks > 1 && (
        <div className="flex gap-2 items-center" style={{ marginTop: "0.5rem" }}>
          <button type="button" className="btn btn-xs" disabled={block === 0} onClick={() => onBlock(block - 1)}>
            上一块
          </button>
          <span className="muted text-xs">
            块 {block + 1}/{totalBlocks}（字节 {start}–{end - 1}，共 {bytes.length}）
          </span>
          <button
            type="button"
            className="btn btn-xs"
            disabled={block >= totalBlocks - 1}
            onClick={() => onBlock(block + 1)}
          >
            下一块
          </button>
        </div>
      )}
    </div>
  );
}

