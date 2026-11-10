import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { formatDateTime } from "../lib/format";

/** 运行时节点页（P5-8）：/api/runtime 节点与心跳字段（kind/health/version/
 *  endpoint/last_heartbeat_at + 服务端推导的 heartbeat_age_seconds）。
 *  已知口径（如实声明）：实时心跳依赖 Gateway 常驻编排，PHASE2-ACCEPTANCE
 *  §7 载明其未落地（心跳当前为派发/恢复按需探测 + 600s 间隔守卫），本页
 *  展示的是登记节点与最近心跳快照，实时性属待验证。runtime_node 为零行时
 *  显真实空态与说明。 */

interface RuntimeNode {
  node_id: string;
  kind?: string;
  endpoint?: string | null;
  health?: string;
  version?: string | null;
  last_heartbeat_at?: string | null;
  heartbeat_age_seconds?: number | null;
  created_at?: string | null;
  updated_at?: string | null;
}

interface RuntimeData {
  available?: boolean;
  source?: string;
  total?: number;
  returned?: number;
  reason?: string;
  message?: string;
  items?: RuntimeNode[];
}

const KIND_BADGE: Record<string, string> = {
  python_worker: "badge-info",
  podman_host: "badge-primary",
  podman_vm: "badge-accent",
};

const HEALTH_BADGE: Record<string, string> = {
  ok: "badge-success",
  degraded: "badge-warning",
  broken: "badge-error",
  unknown: "badge-ghost",
};

function heartbeatText(node: RuntimeNode): string {
  if (!node.last_heartbeat_at) return "从未收到心跳";
  const at = formatDateTime(node.last_heartbeat_at);
  const age = node.heartbeat_age_seconds;
  if (age === null || age === undefined) return `${at}（时龄未知）`;
  if (age < 90) return `${at}（${Math.round(age)} 秒前）`;
  if (age < 5400) return `${at}（${Math.round(age / 60)} 分钟前）`;
  return `${at}（${Math.round(age / 3600)} 小时前）`;
}

export function RuntimePage() {
  const list = useApi<RuntimeData>("/api/runtime?limit=200");
  const items = list.data?.items ?? [];

  return (
    <div>
      <PageHeader
        title="运行时"
        actions={<span className="badge badge-ghost">实时心跳待 Gateway 常驻编排（待验证）</span>}
      />
      <p className="mb-3 text-xs text-base-content/50">
        实时心跳依赖 Gateway 常驻编排（PHASE2-ACCEPTANCE §7 载明未落地，心跳为按需探测 + 600s
        间隔守卫）， 本页展示登记节点与最近心跳快照。
      </p>
      {list.loading ? (
        <SkeletonPanel lines={4} />
      ) : list.error !== null ? (
        <ErrorPanel error={list.error} onRetry={list.retry} />
      ) : list.data?.available === false ? (
        <EmptyPanel
          text={list.data.message ?? `数据面不可读（${list.data.reason ?? "unknown"}）。`}
        />
      ) : items.length === 0 ? (
        <EmptyPanel text="暂无运行时节点：runtime_node 为空（Gateway 心跳尚未接入或节点未登记）。" />
      ) : (
        <>
          <p className="mb-2 text-xs text-base-content/50">
            共 {list.data?.total ?? 0} 个节点（数据源：
            {list.data?.source === "aux" ? "RP_AUX_DB 开发库副本" : list.data?.source}）
          </p>
          <ul className="space-y-2">
            {items.map((node) => (
              <li key={node.node_id} className="rounded-surface border border-edge bg-base-100 p-3">
                <div className="flex flex-wrap items-center gap-1">
                  {node.kind ? (
                    <span className={`badge badge-sm ${KIND_BADGE[node.kind] ?? "badge-ghost"}`}>
                      {node.kind}
                    </span>
                  ) : null}
                  {node.health ? (
                    <span
                      className={`badge badge-sm ${HEALTH_BADGE[node.health] ?? "badge-ghost"}`}
                    >
                      {node.health}
                    </span>
                  ) : null}
                  {node.version ? (
                    <span className="text-[11px] text-base-content/50">{node.version}</span>
                  ) : null}
                </div>
                <p className="mt-1.5 break-all text-sm font-medium">{node.node_id}</p>
                <p className="mt-1 text-[11px] text-base-content/40">
                  {node.endpoint ? `endpoint: ${node.endpoint}` : "endpoint: 未上报"}
                </p>
                <p className="mt-1 text-[11px] text-base-content/40">心跳: {heartbeatText(node)}</p>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
