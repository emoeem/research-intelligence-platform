import { useParams } from "react-router";
import { useApi } from "../lib/hooks";
import { ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { formatDateTime } from "../lib/format";

/** 工具详情（P5-7）：/api/tools?tool_id= 全字段，input/output Schema 渲染后端
 *  返回的原始 JSON（不造结构）；governance/health 与 tool_version 历史（含
 *  provenance）。试运行入口为禁用占位：本阶段只读 API 无执行面，派发链依赖
 *  Phase 1/3（Tool Engine + Podman Runtime），不造假交互。 */

interface ToolFull {
  tool_id: string;
  name: string;
  kind?: string | null;
  lifecycle?: string | null;
  governance?: string | null;
  install_state?: string | null;
  health_state?: string | null;
  permission_level?: string | null;
  input_schema?: string | null;
  output_schema?: string | null;
  timeout_s?: number | null;
  binary_ref?: string | null;
  source_system?: string | null;
  source_pk?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
  schema_version?: number | null;
}

interface ToolVersionRow {
  tv_id: number;
  tool_id: string;
  version: string;
  status?: string | null;
  provenance?: unknown;
  content_hash?: string | null;
  validated_at?: string | null;
  created_at?: string | null;
}

interface ToolDetailData {
  available?: boolean;
  source?: string;
  tool?: ToolFull;
  input_schema?: unknown;
  output_schema?: unknown;
  versions?: ToolVersionRow[];
}

const LIFECYCLE_BADGE: Record<string, string> = {
  REGISTERED: "badge-info",
  VALIDATED: "badge-primary",
  ENABLED: "badge-success",
  DISABLED: "badge-warning",
  DEPRECATED: "badge-error",
};

const GOVERNANCE_BADGE: Record<string, string> = {
  ACTIVE: "badge-success",
  QUARANTINED: "badge-warning",
  REJECTED: "badge-error",
  ARCHIVED: "badge-ghost",
};

function MetaRow({ label, value }: { label: string; value: string | null | undefined }) {
  if (value === null || value === undefined || value === "") return null;
  return (
    <div className="flex gap-2 text-xs">
      <span className="w-24 shrink-0 text-base-content/50">{label}</span>
      <span className="min-w-0 flex-1 break-all">{value}</span>
    </div>
  );
}

function SchemaBlock({ title, schema }: { title: string; schema: unknown }) {
  return (
    <section className="rounded-surface border border-edge bg-base-100 p-4">
      <h2 className="text-sm font-semibold">{title}</h2>
      {schema === null || schema === undefined ? (
        <p className="mt-2 text-xs text-base-content/50">
          该工具未登记 Schema（M 期盘点面记录，Phase 1 契约登记后此处显示真实 JSON Schema）。
        </p>
      ) : (
        <pre
          aria-label={`${title} 原始 JSON`}
          className="mt-2 max-h-96 overflow-auto whitespace-pre-wrap rounded-control bg-base-200 p-3 text-[11px] leading-4"
        >
          {JSON.stringify(schema, null, 2)}
        </pre>
      )}
    </section>
  );
}

export function ToolDetailPage() {
  const { toolId } = useParams<{ toolId: string }>();
  const detail = useApi<ToolDetailData>(
    toolId ? `/api/tools?tool_id=${encodeURIComponent(toolId)}` : null,
  );

  if (!toolId) {
    return (
      <ErrorPanel error={{ status: 404, message: "缺少 tool_id" } as never} onRetry={() => {}} />
    );
  }
  if (detail.loading) {
    return (
      <div>
        <PageHeader title="工具详情" />
        <SkeletonPanel lines={6} />
      </div>
    );
  }
  if (detail.error !== null) {
    return (
      <div>
        <PageHeader title="工具详情" />
        <ErrorPanel error={detail.error} onRetry={detail.retry} />
      </div>
    );
  }
  const tool = detail.data?.tool;
  if (!tool) {
    return (
      <div>
        <PageHeader title="工具详情" />
        <p className="text-sm text-base-content/60">条目不存在。</p>
      </div>
    );
  }
  const versions = detail.data?.versions ?? [];

  return (
    <div className="space-y-4">
      <PageHeader
        title={tool.name}
        actions={
          <div className="flex flex-wrap items-center gap-1">
            {tool.lifecycle ? (
              <span
                className={`badge badge-sm ${LIFECYCLE_BADGE[tool.lifecycle] ?? "badge-ghost"}`}
              >
                {tool.lifecycle}
              </span>
            ) : null}
            {tool.governance ? (
              <span
                className={`badge badge-sm ${GOVERNANCE_BADGE[tool.governance] ?? "badge-ghost"}`}
              >
                {tool.governance}
              </span>
            ) : null}
            {tool.permission_level ? (
              <span className="badge badge-sm badge-outline">{tool.permission_level}</span>
            ) : null}
          </div>
        }
      />

      <div className="rounded-surface border border-edge bg-base-100 p-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2 className="text-sm font-semibold">概要</h2>
          {/* 试运行占位：本阶段只读 API 无执行面，派发链依赖 Phase 1/3。 */}
          <div className="flex items-center gap-2">
            <button
              type="button"
              className="btn btn-sm"
              disabled
              aria-disabled="true"
              title="试运行暂不可用"
            >
              试运行
            </button>
            <span className="text-[11px] text-base-content/50">
              依赖 Phase 1/3 派发链（本阶段只读 API 无执行面）
            </span>
          </div>
        </div>
        <div className="mt-3 space-y-1">
          <MetaRow label="tool_id" value={tool.tool_id} />
          <MetaRow label="kind" value={tool.kind} />
          <MetaRow label="health" value={tool.health_state} />
          <MetaRow label="install_state" value={tool.install_state} />
          <MetaRow
            label="timeout"
            value={
              tool.timeout_s === null || tool.timeout_s === undefined ? null : `${tool.timeout_s}s`
            }
          />
          <MetaRow label="binary_ref" value={tool.binary_ref} />
          <MetaRow label="source_system" value={tool.source_system} />
          <MetaRow label="source_pk" value={tool.source_pk} />
          <MetaRow
            label="登记时间"
            value={tool.created_at ? formatDateTime(tool.created_at) : null}
          />
          <MetaRow
            label="更新时间"
            value={tool.updated_at ? formatDateTime(tool.updated_at) : null}
          />
          <MetaRow
            label="schema_version"
            value={
              tool.schema_version === null || tool.schema_version === undefined
                ? null
                : String(tool.schema_version)
            }
          />
        </div>
      </div>

      <SchemaBlock title="Input Schema" schema={detail.data?.input_schema ?? null} />
      <SchemaBlock title="Output Schema" schema={detail.data?.output_schema ?? null} />

      <section className="rounded-surface border border-edge bg-base-100 p-4">
        <h2 className="text-sm font-semibold">tool_version 历史</h2>
        {versions.length === 0 ? (
          <p className="mt-2 text-xs text-base-content/50">无版本记录。</p>
        ) : (
          <ul className="mt-2 space-y-2">
            {versions.map((version) => (
              <li key={version.tv_id} className="rounded-control border border-edge-soft p-3">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-sm font-semibold">v{version.version}</span>
                  {version.status ? (
                    <span
                      className={`badge badge-sm ${version.status === "current" ? "badge-success" : "badge-ghost"}`}
                    >
                      {version.status}
                    </span>
                  ) : null}
                  {version.validated_at ? (
                    <span className="text-[11px] text-base-content/50">
                      验证于 {formatDateTime(version.validated_at)}
                    </span>
                  ) : null}
                  {version.created_at ? (
                    <span className="text-[11px] text-base-content/50">
                      登记于 {formatDateTime(version.created_at)}
                    </span>
                  ) : null}
                </div>
                {version.content_hash ? (
                  <p className="mt-1 break-all text-[11px] text-base-content/40">
                    sha: {version.content_hash}
                  </p>
                ) : null}
                {version.provenance !== null && version.provenance !== undefined ? (
                  <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap rounded-control bg-base-200 p-2 text-[11px] leading-4">
                    {JSON.stringify(version.provenance, null, 2)}
                  </pre>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
