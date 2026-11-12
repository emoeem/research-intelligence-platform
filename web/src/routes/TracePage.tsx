import { Link, useSearchParams } from "react-router";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";

/** Trace 关联链（P7-5）：/api/traces?id=<任一关联 ID> 统一只读查询（与
 *  rp-agent trace 子命令、agent-engine trace::trace_query 同一拓扑）。
 *  六 ID（trace_id/task_id/agent_run_id/tool_run_id/artifact_id/evidence_id）
 *  任一入参汇出任务/意图/上下文/工具/证据/审计全链；每行 id 带 `/traces?id=`
 *  跳转链接，链间互查即点即达。 */

interface TraceRow {
  [key: string]: unknown;
}

interface TraceChainData {
  available?: boolean;
  query_id?: string;
  trace_ids?: string[];
  matched_via?: string | null;
  message?: string;
  reason?: string;
  counts?: Record<string, number>;
  tasks?: TraceRow[];
  agent_runs?: TraceRow[];
  manifests?: TraceRow[];
  context_items?: TraceRow[];
  tool_runs?: TraceRow[];
  artifacts?: TraceRow[];
  evidence?: TraceRow[];
  audit_events?: TraceRow[];
}

const SECTIONS: {
  title: string;
  key: keyof TraceChainData;
  idKey: string;
  columns: string[];
}[] = [
  {
    title: "任务（research_task）",
    key: "tasks",
    idKey: "task_id",
    columns: ["state", "task_type", "target"],
  },
  {
    title: "Agent 运行（agent_run）",
    key: "agent_runs",
    idKey: "run_id",
    columns: ["state", "agent_id", "agent_version", "created_at"],
  },
  {
    title: "意图清单（context_manifest，intent_json 为意图面快照）",
    key: "manifests",
    idKey: "manifest_id",
    columns: ["intent_status", "task_id", "created_at"],
  },
  {
    title: "上下文条目（context_manifest_item）",
    key: "context_items",
    idKey: "item_id",
    columns: ["layer", "kind", "included", "ref_id", "source"],
  },
  {
    title: "工具运行（tool_run）",
    key: "tool_runs",
    idKey: "run_id",
    columns: ["status", "tool_id", "tool_version", "error_category"],
  },
  {
    title: "产物（artifact）",
    key: "artifacts",
    idKey: "artifact_id",
    columns: ["role", "mime_type", "run_id"],
  },
  {
    title: "证据（evidence）",
    key: "evidence",
    idKey: "evidence_id",
    columns: ["evidence_type", "task_ref", "source_pk"],
  },
  {
    title: "审计事件（audit_event，event_id 升序）",
    key: "audit_events",
    idKey: "event_id",
    columns: ["action", "actor", "correlation_id"],
  },
];

function cell(row: TraceRow, key: string): string {
  const v = row[key];
  if (v === null || v === undefined || v === "") return "";
  return String(v);
}

function IdLink({ id }: { id: string }) {
  return (
    <Link
      to={`/traces?id=${encodeURIComponent(id)}`}
      className="break-all text-primary underline underline-offset-2"
    >
      {id}
    </Link>
  );
}

function TraceSection({
  title,
  rows,
  idKey,
  columns,
}: {
  title: string;
  rows: TraceRow[];
  idKey: string;
  columns: string[];
}) {
  return (
    <section className="rounded-surface border border-edge bg-base-100 p-4">
      <h2 className="text-sm font-semibold">
        {title}
        <span className="ml-2 text-xs font-normal text-base-content/50">{rows.length} 行</span>
      </h2>
      {rows.length === 0 ? (
        <p className="mt-2 text-xs text-base-content/50">
          空段（015 之前落库行 trace_id 为 NULL，不参与关联，如实呈现）。
        </p>
      ) : (
        <ul className="mt-2 space-y-1.5">
          {rows.map((row, i) => (
            <li key={i} className="rounded-control border border-edge-soft p-2 text-xs">
              <IdLink id={cell(row, idKey) || `#${i + 1}`} />
              <span className="ml-2 text-base-content/60">
                {columns
                  .filter((c) => cell(row, c))
                  .map((c) => `${c}=${cell(row, c)}`)
                  .join(" · ")}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function TracePage() {
  const [params] = useSearchParams();
  const queryId = params.get("id") ?? "";
  const chain = useApi<TraceChainData>(
    queryId ? `/api/traces?id=${encodeURIComponent(queryId)}` : null,
  );

  return (
    <div className="space-y-4">
      <PageHeader title="Trace 关联链" />
      {!queryId ? (
        <EmptyPanel text="需 ?id=<关联 ID>：六 ID（trace_id / task_id / agent_run_id / tool_run_id / artifact_id / evidence_id）任一即可。" />
      ) : chain.loading ? (
        <SkeletonPanel lines={6} />
      ) : chain.error !== null ? (
        <ErrorPanel error={chain.error} onRetry={chain.retry} />
      ) : chain.data?.available === false ? (
        <EmptyPanel
          text={chain.data.message ?? `关联面不可读（${chain.data.reason ?? "unknown"}）。`}
        />
      ) : (chain.data?.trace_ids?.length ?? 0) === 0 ? (
        <EmptyPanel text={chain.data?.message ?? `${queryId} 未解析到任何 trace_id。`} />
      ) : (
        <>
          <div className="rounded-surface border border-edge bg-base-100 p-4">
            <div className="flex flex-wrap items-center gap-2 text-xs">
              <span className="text-base-content/50">查询入参</span>
              <span className="break-all font-medium">{chain.data?.query_id}</span>
              <span className="text-base-content/50">命中面</span>
              <span className="badge badge-sm badge-info">{chain.data?.matched_via}</span>
            </div>
            <p className="mt-2 break-all text-xs text-base-content/60">
              trace_ids: {(chain.data?.trace_ids ?? []).join(", ")}
            </p>
            <p className="mt-1 text-[11px] text-base-content/40">
              {Object.entries(chain.data?.counts ?? {})
                .map(([k, v]) => `${k} ${v}`)
                .join(" · ")}
            </p>
          </div>
          {SECTIONS.map((s) => {
            const rows = (chain.data?.[s.key] as TraceRow[] | undefined) ?? [];
            return (
              <TraceSection
                key={s.title}
                title={s.title}
                rows={rows}
                idKey={s.idKey}
                columns={s.columns}
              />
            );
          })}
        </>
      )}
    </div>
  );
}
