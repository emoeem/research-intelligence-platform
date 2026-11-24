import { translateUi } from "../../i18n";
/** 证据档页（P5-9b，Phase 5 §4.7 / §7 P5-7 完成标准）：/api/research/evidence
 *  真实列表（rp-verify evidence-list 子进程读面，计划域）+ 单条详情
 *  （evidence-get 子进程）+ Raw/Pretty 双视图 + SHA256/类型/采集时间 +
 *  来源跳转（plan → 验证计划页；tool_run → Run 详情；trace → Trace 页）。
 *
 *  §4.7 边界（本批如实范围）：payload_json 是采集时点原始快照（021
 *  append-only，知识更新不覆盖）；页面呈现原始与派生分离（Raw 直出 JSON，
 *  Pretty 格式化视图）；大 payload 截断展示（不整段渲染）；脱敏状态以采集
 *  侧为准，页面明示原始快照未经二次脱敏。HTTP Request/Response 对照、
 *  Headers/Cookies 分区、截图 Artifact 预览随 WSW 流量面批次，不在本批
 *  编造。 */

import { useMemo, useState } from "react";
import { Link } from "react-router";
import { useApi } from "../../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../../components/ui";
import { formatDateTime } from "../../lib/format";
import {
  EVIDENCE_TYPE_TONES,
  type ResearchEvidenceRow,
  type ResearchEvidenceList,
  type ResearchPlanList,
} from "./shared";

/** 详情 payload 展示上限（字符口径；超出截断 + 明示，不整段渲染）。 */
const PAYLOAD_PREVIEW_LIMIT = 8000;

function renderPayload(row: ResearchEvidenceRow, pretty: boolean): string {
  if (typeof row.payload === "string") return row.payload;
  try {
    return pretty ? JSON.stringify(row.payload, null, 2) : JSON.stringify(row.payload);
  } catch {
    return String(row.payload ?? "");
  }
}

export function ResearchEvidencePage() {
  const plans = useApi<ResearchPlanList>("/api/research/plans?limit=100");
  const [planId, setPlanId] = useState<string>("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pretty, setPretty] = useState(true);

  const enabled = planId !== "";
  const list = useApi<ResearchEvidenceList>(
    enabled ? `/api/research/evidence?plan=${encodeURIComponent(planId)}` : null,
  );
  const evidence = list.data?.items ?? [];
  const selected = useMemo(
    () => evidence.find((row) => row.evidence_id === selectedId) ?? null,
    [evidence, selectedId],
  );

  const payloadText = selected ? renderPayload(selected, pretty) : "";
  const payloadTruncated = selected !== null && payloadText.length > PAYLOAD_PREVIEW_LIMIT;
  const payloadShown = payloadTruncated
    ? `${payloadText.slice(0, PAYLOAD_PREVIEW_LIMIT)}\n…${translateUi("截断：完整原文经 Raw 下载或 evidence-get 直读，本页不整段渲染")}`
    : payloadText;

  if (plans.loading) {
    return (
      <div>
        <PageHeader title={translateUi("证据档")} />
        <SkeletonPanel lines={6} />
      </div>
    );
  }
  if (plans.error !== null) {
    return (
      <div>
        <PageHeader title={translateUi("证据档")} />
        <ErrorPanel error={plans.error} onRetry={plans.retry} />
      </div>
    );
  }

  const planOptions = (plans.data?.items ?? []).map((plan) => ({
    value: plan.plan_id,
    label: `${plan.title ?? plan.plan_id}（${plan.plan_id}）`,
  }));

  return (
    <div className="space-y-4">
      <PageHeader title={`${translateUi("证据档")}${enabled ? `（${evidence.length}）` : ""}`} />
      <div className="flex flex-wrap items-center gap-3">
        <label className="form-control">
          <span className="label-text pb-1 text-xs">{translateUi("按计划过滤")}</span>
          <select
            data-testid="evidence-plan-select"
            className="select select-bordered select-sm w-72 max-w-full text-xs"
            value={planId}
            onChange={(event) => {
              setPlanId(event.target.value);
              setSelectedId(null);
            }}
          >
            <option value="">{translateUi("选择计划（evidence-list 计划域）")}</option>
            {planOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <p className="text-[11px] text-muted">
          {translateUi(
            "原始快照（append-only，021 语义：知识更新不覆盖原始证据）；脱敏状态以采集侧为准，本页不二次改写。",
          )}
        </p>
      </div>

      {!enabled ? (
        <EmptyPanel
          text={translateUi(
            "选择一个验证计划后加载其证据档；证据在 execute-plan 逐项执行时采集（P5-7 面）。",
          )}
        />
      ) : list.loading ? (
        <SkeletonPanel lines={6} />
      ) : list.error !== null ? (
        <ErrorPanel error={list.error} onRetry={list.retry} />
      ) : evidence.length === 0 ? (
        <EmptyPanel
          text={translateUi(
            "该计划暂无证据档；execute-plan 派发执行后逐项落档（evidence_collected 事件联动）。",
          )}
        />
      ) : (
        <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
          <div className="space-y-2" data-testid="evidence-list-section">
            {evidence.map((row) => (
              <button
                key={row.evidence_id}
                type="button"
                data-testid={`evidence-row-${row.evidence_id}`}
                onClick={() => setSelectedId(row.evidence_id)}
                className={`w-full rounded-control border p-3 text-left transition-colors ${
                  selectedId === row.evidence_id
                    ? "border-primary bg-base-200"
                    : "border-edge bg-base-100 hover:border-primary"
                }`}
              >
                <div className="flex flex-wrap items-center gap-2">
                  <span
                    className={`badge badge-sm badge-outline ${EVIDENCE_TYPE_TONES[row.evidence_type] ?? "badge-ghost"}`}
                  >
                    {row.evidence_type}
                  </span>
                  <span className="font-mono text-[11px]">{row.evidence_id}</span>
                </div>
                <p className="mt-1 font-mono text-[10px] text-muted">
                  item {row.item_id} · {formatDateTime(row.created_at)}
                </p>
              </button>
            ))}
          </div>

          <div
            className="rounded-surface border border-edge bg-base-100 p-3"
            data-testid="evidence-detail-section"
          >
            {selected === null ? (
              <EmptyPanel text={translateUi("左侧选择一条证据查看详情。")} />
            ) : (
              <div className="space-y-3">
                <div className="flex flex-wrap items-center gap-2">
                  <span
                    className={`badge badge-sm badge-outline ${EVIDENCE_TYPE_TONES[selected.evidence_type] ?? "badge-ghost"}`}
                  >
                    {selected.evidence_type}
                  </span>
                  <span className="font-mono text-xs">{selected.evidence_id}</span>
                </div>
                <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[11px]">
                  <dt className="text-muted">SHA256</dt>
                  <dd className="break-all font-mono">{selected.content_hash}</dd>
                  <dt className="text-muted">{translateUi("采集时间")}</dt>
                  <dd>{formatDateTime(selected.created_at)}</dd>
                  <dt className="text-muted">{translateUi("来源计划")}</dt>
                  <dd>
                    <Link className="link link-primary font-mono text-[11px]" to="/research/plans">
                      {selected.plan_id}
                    </Link>
                  </dd>
                  <dt className="text-muted">{translateUi("来源计划项")}</dt>
                  <dd className="font-mono">{selected.item_id}</dd>
                  <dt className="text-muted">tool_run</dt>
                  <dd>
                    {selected.tool_run_id ? (
                      <Link
                        className="link link-primary font-mono text-[11px]"
                        to={`/runs/${encodeURIComponent(selected.tool_run_id)}`}
                      >
                        {selected.tool_run_id}
                      </Link>
                    ) : (
                      <span className="text-muted">{translateUi("无（采集面未挂 run）")}</span>
                    )}
                  </dd>
                  <dt className="text-muted">trace</dt>
                  <dd>
                    {selected.trace_id ? (
                      <Link className="link link-primary font-mono text-[11px]" to="/traces">
                        {selected.trace_id}
                      </Link>
                    ) : (
                      <span className="text-muted">{translateUi("无（015 前落库行为 NULL）")}</span>
                    )}
                  </dd>
                </dl>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    className={`btn btn-xs ${pretty ? "btn-outline" : "btn-primary"}`}
                    onClick={() => setPretty(false)}
                  >
                    Raw
                  </button>
                  <button
                    type="button"
                    className={`btn btn-xs ${pretty ? "btn-primary" : "btn-outline"}`}
                    onClick={() => setPretty(true)}
                  >
                    Pretty
                  </button>
                </div>
                <pre
                  data-testid="evidence-payload"
                  className="max-h-80 overflow-auto rounded-control border border-edge-soft bg-base-200 p-2 font-mono text-[11px] leading-4"
                >
                  {payloadShown}
                </pre>
                <p className="text-[10px] text-muted">
                  {translateUi(
                    "原始 payload 快照直出（Raw = 紧凑序列化原文；Pretty = 格式化视图）；本页不做二次脱敏。",
                  )}
                </p>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
