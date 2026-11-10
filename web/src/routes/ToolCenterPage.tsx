import { useState } from "react";
import { Link } from "react-router";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { formatDateTime } from "../lib/format";

/** 工具中心（P5-7）：目录列表 + 生命周期/治理过滤（服务端过滤参数
 *  lifecycle/governance），行点击进 /tools/:toolId 详情页。
 *  三态统一：loading= SkeletonPanel、error= ErrorPanel（可重试）、
 *  过滤无命中= EmptyPanel（真实空态，不造假数据）。 */

interface ToolRow {
  tool_id: string;
  name: string;
  kind?: string | null;
  lifecycle?: string | null;
  governance?: string | null;
  install_state?: string | null;
  health_state?: string | null;
  permission_level?: string | null;
  timeout_s?: number | null;
  source_system?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
}

interface ToolList {
  available?: boolean;
  source?: string;
  total?: number;
  returned?: number;
  items?: ToolRow[];
}

const LIFECYCLE_OPTIONS = ["REGISTERED", "VALIDATED", "ENABLED", "DISABLED", "DEPRECATED"] as const;
const GOVERNANCE_OPTIONS = ["ACTIVE", "QUARANTINED", "REJECTED", "ARCHIVED"] as const;

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

export function ToolCenterPage() {
  const [lifecycle, setLifecycle] = useState("");
  const [governance, setGovernance] = useState("");

  const params = new URLSearchParams({ limit: "200" });
  if (lifecycle) params.set("lifecycle", lifecycle);
  if (governance) params.set("governance", governance);
  const list = useApi<ToolList>(`/api/tools?${params.toString()}`);

  const items = list.data?.items ?? [];
  const filtered = lifecycle !== "" || governance !== "";

  return (
    <div>
      <PageHeader
        title="工具中心"
        actions={
          <div className="flex items-center gap-2">
            <select
              aria-label="按生命周期过滤"
              className="select select-sm rounded-control border border-edge bg-base-100"
              value={lifecycle}
              onChange={(event) => setLifecycle(event.target.value)}
            >
              <option value="">全部生命周期</option>
              {LIFECYCLE_OPTIONS.map((option) => (
                <option key={option} value={option}>
                  {option}
                </option>
              ))}
            </select>
            <select
              aria-label="按治理状态过滤"
              className="select select-sm rounded-control border border-edge bg-base-100"
              value={governance}
              onChange={(event) => setGovernance(event.target.value)}
            >
              <option value="">全部治理状态</option>
              {GOVERNANCE_OPTIONS.map((option) => (
                <option key={option} value={option}>
                  {option}
                </option>
              ))}
            </select>
          </div>
        }
      />
      {list.loading ? (
        <SkeletonPanel lines={6} />
      ) : list.error !== null ? (
        <ErrorPanel error={list.error} onRetry={list.retry} />
      ) : items.length === 0 ? (
        <EmptyPanel
          text={
            filtered ? "当前过滤条件下没有工具，调整生命周期或治理状态后重试。" : "暂无已登记工具。"
          }
        />
      ) : (
        <>
          <p className="mb-2 text-xs text-base-content/50">
            {filtered ? "过滤后：" : ""}共 {list.data?.total ?? 0} 个工具（显示 {items.length}
            ，数据源：
            {list.data?.source === "main" ? "主库注册面" : list.data?.source}）
          </p>
          <ul className="space-y-2">
            {items.map((tool) => (
              <li key={tool.tool_id}>
                <Link
                  to={`/tools/${encodeURIComponent(tool.tool_id)}`}
                  className="block rounded-surface border border-edge bg-base-100 p-3 transition-colors hover:border-base-content/30"
                >
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
                    {tool.health_state ? (
                      <span className="text-[11px] text-base-content/50">
                        health: {tool.health_state}
                      </span>
                    ) : null}
                    {tool.permission_level ? (
                      <span className="text-[11px] text-base-content/50">
                        {tool.permission_level}
                      </span>
                    ) : null}
                  </div>
                  <p className="mt-1.5 text-sm font-medium">{tool.name}</p>
                  <p className="mt-1 text-[11px] text-base-content/40">
                    {tool.tool_id}
                    {tool.kind ? ` · ${tool.kind}` : ""}
                    {tool.updated_at ? ` · ${formatDateTime(tool.updated_at)}` : ""}
                  </p>
                </Link>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
