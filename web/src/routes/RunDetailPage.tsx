import { Link, useParams } from "react-router";
import { useApi } from "../lib/hooks";
import { ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { formatDateTime } from "../lib/format";

/** Run 详情（P5-8）：/api/runs?run_id= 全字段（usage/environment/replay/budget
 *  JSON 列解析）+ input_hash + artifact 列表与预览入口（uri 新标签打开）。
 *  artifact 表缺失时显服务端 artifacts_note 说明。
 *  P7-5：概要区渲染六 ID 关联面（correlation：trace_id/task_id/agent_run_id/
 *  tool_run_id/artifact_ids/evidence_ids），全部可点击跳转 /traces?id=（任务
 *  跳 projects）。 */

interface RunCorrelation {
  trace_id?: string | null;
  task_id?: string | null;
  agent_run_id?: string | null;
  tool_run_id?: string | null;
  artifact_ids?: (string | null)[];
  evidence_ids?: (string | null)[];
}

interface RunFull {
  run_id: string;
  tool_id: string;
  tool_version?: string | null;
  task_id?: string | null;
  workspace_id?: string | null;
  submit_key?: string | null;
  input_hash?: string | null;
  // JSON 列：服务端已解析为对象（字段名保留 *_json 后缀，rp_server.py detail 分支）
  input_refs_json?: unknown;
  permission_ctx?: unknown;
  status?: string;
  error_category?: string | null;
  error_json?: unknown;
  budget_json?: unknown;
  duration_ms?: number | null;
  usage_json?: unknown;
  environment_json?: unknown;
  replay_json?: unknown;
  created_at?: string | null;
  started_at?: string | null;
  finished_at?: string | null;
  updated_at?: string | null;
}

interface ArtifactRow {
  artifact_id: string;
  run_id: string;
  role?: string | null;
  uri: string;
  sha256: string;
  size_bytes: number;
  mime_type: string;
  provenance?: string | null;
  created_at?: string | null;
}

interface RunDetailData {
  available?: boolean;
  source?: string;
  run?: RunFull;
  artifacts?: ArtifactRow[];
  artifacts_note?: string | null;
  correlation?: RunCorrelation;
}

const STATUS_BADGE: Record<string, string> = {
  SUCCEEDED: "badge-success",
  FAILED: "badge-error",
  TIMED_OUT: "badge-error",
  CANCELLED: "badge-warning",
  RUNNING: "badge-info",
  PREPARING: "badge-info",
  WAITING_FOR_RUNNER: "badge-ghost",
  QUEUED: "badge-ghost",
};

function MetaRow({
  label,
  value,
  linkTemplate,
}: {
  label: string;
  value: string | null | undefined;
  linkTemplate?: string;
}) {
  if (value === null || value === undefined || value === "") return null;
  return (
    <div className="flex gap-2 text-xs">
      <span className="w-24 shrink-0 text-muted">{label}</span>
      <span className="min-w-0 flex-1 break-all">
        {linkTemplate ? (
          <Link
            to={linkTemplate.replace("{id}", encodeURIComponent(value))}
            className="text-primary underline underline-offset-2"
          >
            {value}
          </Link>
        ) : (
          value
        )}
      </span>
    </div>
  );
}

/** P7-5 六 ID 多值行（artifact_ids/evidence_ids）：每个 id 可点击跳
 *  /traces?id= 关联链。空列表不渲染。 */
function IdListRow({ label, ids }: { label: string; ids: string[] }) {
  if (ids.length === 0) return null;
  return (
    <div className="flex gap-2 text-xs">
      <span className="w-24 shrink-0 text-muted">{label}</span>
      <span className="flex min-w-0 flex-1 flex-wrap gap-x-3 gap-y-1 break-all">
        {ids.map((id) => (
          <Link
            key={id}
            to={`/traces?id=${encodeURIComponent(id)}`}
            className="break-all text-primary underline underline-offset-2"
          >
            {id}
          </Link>
        ))}
      </span>
    </div>
  );
}

function JsonBlock({ title, value }: { title: string; value: unknown }) {
  if (value === null || value === undefined) return null;
  return (
    <section className="rounded-surface border border-edge bg-base-100 p-4">
      <h2 className="text-sm font-semibold">{title}</h2>
      <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap rounded-control bg-base-200 p-3 text-[11px] leading-4">
        {JSON.stringify(value, null, 2)}
      </pre>
    </section>
  );
}

function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / 1024 / 1024).toFixed(1)} MB`;
}

export function RunDetailPage() {
  const { runId } = useParams<{ runId: string }>();
  const detail = useApi<RunDetailData>(
    runId ? `/api/runs?run_id=${encodeURIComponent(runId)}` : null,
  );

  if (!runId) {
    return (
      <div>
        <PageHeader title="Run 详情" />
        <p className="text-sm text-muted">缺少 run_id。</p>
      </div>
    );
  }
  if (detail.loading) {
    return (
      <div>
        <PageHeader title="Run 详情" />
        <SkeletonPanel lines={6} />
      </div>
    );
  }
  if (detail.error !== null) {
    return (
      <div>
        <PageHeader title="Run 详情" />
        <ErrorPanel error={detail.error} onRetry={detail.retry} />
      </div>
    );
  }
  const run = detail.data?.run;
  if (!run) {
    return (
      <div>
        <PageHeader title="Run 详情" />
        <p className="text-sm text-muted">条目不存在。</p>
      </div>
    );
  }
  const artifacts = detail.data?.artifacts ?? [];
  const artifactsNote = detail.data?.artifacts_note ?? null;
  const correlation = detail.data?.correlation;

  return (
    <div className="space-y-4">
      <PageHeader
        title={run.run_id}
        actions={
          <div className="flex flex-wrap items-center gap-1">
            {run.status ? (
              <span className={`badge badge-sm ${STATUS_BADGE[run.status] ?? "badge-ghost"}`}>
                {run.status}
              </span>
            ) : null}
            {run.error_category ? (
              <span className="badge badge-sm badge-error">{run.error_category}</span>
            ) : null}
          </div>
        }
      />

      <div className="rounded-surface border border-edge bg-base-100 p-4">
        <h2 className="text-sm font-semibold">概要</h2>
        <div className="mt-3 space-y-1">
          <MetaRow
            label="tool"
            value={
              run.tool_id
                ? `${run.tool_id}${run.tool_version ? ` @ v${run.tool_version}` : ""}`
                : null
            }
          />
          <MetaRow label="workspace" value={run.workspace_id} />
          {run.task_id ? (
            <div className="flex gap-2 text-xs">
              <span className="w-24 shrink-0 text-muted">task</span>
              <Link
                to={`/projects?task_id=${encodeURIComponent(run.task_id)}`}
                className="text-primary underline underline-offset-2"
              >
                {run.task_id}
              </Link>
            </div>
          ) : null}
          <MetaRow label="submit_key" value={run.submit_key} />
          <IdListRow label="trace" ids={correlation?.trace_id ? [correlation.trace_id] : []} />
          <MetaRow
            label="agent_run"
            value={correlation?.agent_run_id ?? null}
            linkTemplate="/traces?id={id}"
          />
          <MetaRow
            label="tool_run"
            value={correlation?.tool_run_id ?? run.run_id}
            linkTemplate="/traces?id={id}"
          />
          <IdListRow
            label="artifacts"
            ids={(correlation?.artifact_ids ?? []).filter(Boolean) as string[]}
          />
          <IdListRow
            label="evidence"
            ids={(correlation?.evidence_ids ?? []).filter(Boolean) as string[]}
          />
          <MetaRow label="input_hash" value={run.input_hash} />
          <MetaRow
            label="duration"
            value={
              run.duration_ms === null || run.duration_ms === undefined
                ? null
                : `${run.duration_ms} ms`
            }
          />
          <MetaRow label="发起" value={run.created_at ? formatDateTime(run.created_at) : null} />
          <MetaRow label="开始" value={run.started_at ? formatDateTime(run.started_at) : null} />
          <MetaRow label="完成" value={run.finished_at ? formatDateTime(run.finished_at) : null} />
          <MetaRow label="更新" value={run.updated_at ? formatDateTime(run.updated_at) : null} />
        </div>
      </div>

      <section className="rounded-surface border border-edge bg-base-100 p-4">
        <h2 className="text-sm font-semibold">Artifact 列表</h2>
        {artifactsNote ? (
          <p className="mt-2 text-xs text-muted">{artifactsNote}</p>
        ) : null}
        {artifacts.length === 0 ? (
          <p className="mt-2 text-xs text-muted">
            {artifactsNote
              ? "本 Run 无已登记 artifact。"
              : "本 Run 暂无 artifact 记录（产物未登记或已归档）。"}
          </p>
        ) : (
          <ul className="mt-2 space-y-2">
            {artifacts.map((artifact) => (
              <li
                key={artifact.artifact_id}
                className="rounded-control border border-edge-soft p-3"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <span className="badge badge-sm badge-outline">
                    {artifact.role ?? "artifact"}
                  </span>
                  <span className="text-[11px] text-muted">{artifact.mime_type}</span>
                  <span className="text-[11px] text-muted">
                    {formatBytes(artifact.size_bytes)}
                  </span>
                  <a
                    href={artifact.uri}
                    target="_blank"
                    rel="noreferrer"
                    className="ml-auto text-xs text-primary underline underline-offset-2"
                  >
                    预览 / 下载
                  </a>
                </div>
                <p className="mt-1 break-all text-xs">{artifact.uri}</p>
                <p className="mt-1 break-all text-[11px] text-muted">
                  {artifact.artifact_id} · sha256 {artifact.sha256.slice(0, 16)}…
                  {artifact.created_at ? ` · ${formatDateTime(artifact.created_at)}` : ""}
                </p>
              </li>
            ))}
          </ul>
        )}
      </section>

      <JsonBlock title="usage" value={run.usage_json} />
      <JsonBlock title="environment" value={run.environment_json} />
      <JsonBlock title="budget" value={run.budget_json} />
      <JsonBlock title="input_refs" value={run.input_refs_json} />
      <JsonBlock title="error" value={run.error_json} />
      <JsonBlock title="replay" value={run.replay_json} />
    </div>
  );
}
