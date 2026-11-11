import { Link } from "react-router";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { formatDateTime } from "../lib/format";
import type { PluginList, PluginRow } from "../lib/api";

/** 插件中心（P6-8，计划 §10.4 E）：目录分类（已安装 / 可更新 / 已禁用 /
 *  其他含已卸载）+ 生命周期状态徽章 + 信任级 + Manifest 入口。行点击进
 *  /plugins/:pluginId 详情（三级按需加载，默认 L2）。数据来自
 *  /api/plugins（apps/server 直查 RIP_PLUGIN_DB 只读）。 */

const STATE_BADGE: Record<string, string> = {
  DISCOVERED: "badge-ghost",
  VALIDATED: "badge-info",
  INSTALLED: "badge-info",
  REGISTERED: "badge-primary",
  ENABLED: "badge-success",
  LOADED: "badge-success",
  ACTIVE: "badge-success",
  INVALID: "badge-error",
  DISABLED: "badge-warning",
  UPDATE_PENDING: "badge-warning",
  ERROR: "badge-error",
  UNINSTALLED: "badge-ghost",
};

const KIND_LABEL: Record<string, string> = {
  knowledge: "知识",
  tool: "工具",
  project: "项目",
  agent: "Agent",
  workflow: "工作流",
  integration: "集成",
};

const TRUST_BADGE: Record<string, string> = {
  official: "badge-success",
  community: "badge-info",
  local: "badge-ghost",
  remote: "badge-warning",
};

/** SK-2：来源知识统计徽章（审核计数与条目类型，knowledge_source_entry）。 */
function KnowledgeStatsBadges({ plugin }: { plugin: PluginRow }) {
  const stats = plugin.knowledge_stats;
  if (!stats) return null;
  const total = Object.values(stats.review ?? {}).reduce((a, b) => a + b, 0);
  if (total === 0) return null;
  const approved = stats.review?.APPROVED ?? 0;
  return (
    <span className="badge badge-sm badge-outline" data-testid="knowledge-stats">
      来源 {total} 条
      {approved > 0 ? ` · APPROVED ${approved}` : ""}
    </span>
  );
}

function PluginCard({ plugin }: { plugin: PluginRow }) {
  const stateBadge = STATE_BADGE[plugin.state] ?? "badge-ghost";
  return (
    <Link
      to={`/plugins/${encodeURIComponent(plugin.plugin_id)}`}
      className="block rounded-control border border-edge bg-base-100 p-4 hover:border-primary"
      data-testid={`plugin-card-${plugin.plugin_id}`}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="font-medium">{plugin.name}</span>
        <span className={`badge badge-sm ${stateBadge}`} data-testid="plugin-state">
          {plugin.state}
        </span>
      </div>
      <div className="mt-1 text-xs opacity-70">
        {plugin.plugin_id} @ {plugin.version}
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
        <span className="badge badge-sm badge-ghost">{KIND_LABEL[plugin.kind] ?? plugin.kind}</span>
        <span className={`badge badge-sm ${TRUST_BADGE[plugin.trust_level] ?? "badge-ghost"}`}>
          {plugin.trust_level}
        </span>
        {/* SK-2：许可证状态与快照 Commit（来源主页在详情页）。 */}
        <span className="badge badge-sm badge-info" data-testid="plugin-license">
          {plugin.license}
        </span>
        {plugin.pinned?.value ? (
          <span
            className="badge badge-sm badge-ghost font-mono"
            title={`快照 ${plugin.pinned.kind}: ${plugin.pinned.value}`}
            data-testid="plugin-commit"
          >
            {plugin.pinned.value.slice(0, 7)}
          </span>
        ) : null}
        <KnowledgeStatsBadges plugin={plugin} />
        {plugin.update_pending ? (
          <span className="badge badge-sm badge-warning" data-testid="update-pending">
            可更新
          </span>
        ) : null}
        {plugin.source_uninstalled ? (
          <span className="badge badge-sm badge-ghost">来源已卸载</span>
        ) : null}
        <span className="opacity-70">{plugin.capabilities_count ?? 0} capabilities</span>
        {plugin.updated_at ? (
          <span className="ml-auto opacity-50">{formatDateTime(plugin.updated_at)}</span>
        ) : null}
      </div>
    </Link>
  );
}

function CategorySection({
  title,
  plugins,
  testId,
}: {
  title: string;
  plugins: PluginRow[];
  testId: string;
}) {
  if (plugins.length === 0) return null;
  return (
    <section className="mb-6" data-testid={testId}>
      <h2 className="mb-2 text-sm font-semibold opacity-80">
        {title}（{plugins.length}）
      </h2>
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {plugins.map((plugin) => (
          <PluginCard key={`${plugin.plugin_id}@${plugin.version}`} plugin={plugin} />
        ))}
      </div>
    </section>
  );
}

export function PluginsPage() {
  const list = useApi<PluginList>("/api/plugins");
  if (list.loading) return <SkeletonPanel lines={4} />;
  if (list.error) return <ErrorPanel error={list.error} onRetry={list.retry} />;

  const data = list.data;
  if (!data?.available) {
    return (
      <div>
        <PageHeader title="插件中心" />
        <EmptyPanel text={data?.message ?? "插件数据面不可用"} />
      </div>
    );
  }

  const items = data.items ?? [];
  const active = items.filter((p) => p.is_active && !["DISABLED", "UNINSTALLED"].includes(p.state));
  const updatable = items.filter((p) => p.update_pending);
  const disabled = items.filter((p) => p.state === "DISABLED");
  const others = items.filter(
    (p) => !active.includes(p) && !updatable.includes(p) && !disabled.includes(p),
  );

  return (
    <div>
      <PageHeader title={`插件中心（共 ${data.total} 个插件条目）`} />
      {items.length === 0 ? (
        <EmptyPanel text="尚无插件：经 rp-plugin install 安装后在此展示" />
      ) : (
        <>
          <CategorySection title="已安装" plugins={active} testId="plugins-installed" />
          <CategorySection title="可更新" plugins={updatable} testId="plugins-updatable" />
          <CategorySection title="已禁用" plugins={disabled} testId="plugins-disabled" />
          <CategorySection title="其他（含已卸载）" plugins={others} testId="plugins-others" />
        </>
      )}
    </div>
  );
}
