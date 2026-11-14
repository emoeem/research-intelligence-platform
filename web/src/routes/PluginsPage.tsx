import { useState } from "react";
import { Link } from "react-router";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { TrustConfirmDialog } from "../components/TrustConfirmDialog";
import { formatDateTime } from "../lib/format";
import {
  pluginInstall,
  pluginManifestInspect,
  resolveTrustTier,
  type ManifestInspectResult,
  type PluginConfirmFields,
  type PluginList,
  type PluginRow,
  type TrustPolicy,
} from "../lib/api";

/** 插件中心（P6-8，计划 §10.4 E）：目录分类（已安装 / 可更新 / 已禁用 /
 *  其他含已卸载）+ 生命周期状态徽章 + 信任级 + Manifest 入口。行点击进
 *  /plugins/:pluginId 详情（三级按需加载，默认 L2）。数据来自
 *  /api/plugins（apps/server 直查 RIP_PLUGIN_DB 只读）。
 *  P10-2：安装面板（Manifest 预检 → 信任级四档分级确认对话框 → 安装），
 *  确认强度由 plugin-registry::trust_policy 单一来源下发（official 常规
 *  确认到 remote 输入插件 id 二次确认），服务端按同一策略表强制。 */

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
      来源 {total} 条{approved > 0 ? ` · APPROVED ${approved}` : ""}
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
          <span className="ml-auto text-muted">{formatDateTime(plugin.updated_at)}</span>
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

const TRUST_OPTIONS = ["official", "community", "local", "remote"] as const;

/** P10-2：安装面板（Manifest 预检 → 分级确认对话框 → 安装）。
 *  预检结果展示 id/version/license 与来源推导信任档；确认强度按所选
 *  trust 走策略表（策略不可得 fail-closed 最强档）。 */
function InstallPanel({ policy }: { policy: TrustPolicy | null }) {
  const [manifestPath, setManifestPath] = useState("");
  const [trust, setTrust] = useState<(typeof TRUST_OPTIONS)[number]>("local");
  const [preview, setPreview] = useState<ManifestInspectResult | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [inspectBusy, setInspectBusy] = useState(false);
  const [installBusy, setInstallBusy] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);
  const [installOk, setInstallOk] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);

  const tier = resolveTrustTier(policy, trust);

  async function inspect() {
    setPreviewError(null);
    setPreview(null);
    setInstallOk(null);
    setInspectBusy(true);
    try {
      const res = await pluginManifestInspect(manifestPath.trim());
      setPreview(res.result);
    } catch (error) {
      setPreviewError(error instanceof Error ? error.message : String(error));
    } finally {
      setInspectBusy(false);
    }
  }

  async function doInstall(fields: PluginConfirmFields) {
    if (!preview) return;
    setInstallError(null);
    setInstallBusy(true);
    try {
      const res = await pluginInstall(manifestPath.trim(), trust, fields);
      setInstallOk(`${res.result.command ?? "install"} 已受理：${preview.id}@${preview.version}`);
      setDialogOpen(false);
      setPreview(null);
    } catch (error) {
      setInstallError(error instanceof Error ? error.message : String(error));
    } finally {
      setInstallBusy(false);
    }
  }

  return (
    <section
      className="mb-6 rounded-control border border-edge bg-base-100 p-4"
      data-testid="install-panel"
    >
      <h2 className="mb-2 text-sm font-semibold">安装插件（Manifest 预检 + 分级确认）</h2>
      <div className="flex flex-wrap items-center gap-2">
        <input
          type="text"
          className="input input-sm input-bordered w-72 font-mono"
          placeholder="schemas/plugin-manifest/examples/knowledge.rtis-redteam.json"
          value={manifestPath}
          onChange={(event) => setManifestPath(event.target.value)}
          data-testid="install-manifest-path"
        />
        <select
          className="select select-sm select-bordered"
          value={trust}
          onChange={(event) => setTrust(event.target.value as (typeof TRUST_OPTIONS)[number])}
          data-testid="install-trust-select"
          aria-label="信任级"
        >
          {TRUST_OPTIONS.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="btn btn-sm btn-ghost"
          onClick={inspect}
          disabled={inspectBusy || manifestPath.trim().length === 0}
          data-testid="install-inspect"
        >
          预检 Manifest
        </button>
        <button
          type="button"
          className="btn btn-sm btn-primary"
          onClick={() => setDialogOpen(true)}
          disabled={!preview || installBusy}
          data-testid="install-open-confirm"
        >
          安装
        </button>
      </div>
      <p className="mt-2 text-xs text-muted" data-testid="install-tier-hint">
        信任级 {trust}：{tier.confirmation_label}
        {tier.auto_update_prompt ? "；D10 允许自动更新提示" : "；D10 需手动检查更新"}
      </p>
      {preview ? (
        <div
          className="mt-3 rounded-control border border-edge p-3 text-xs"
          data-testid="install-preview"
        >
          <div className="flex flex-wrap gap-2">
            <span className="font-mono">{preview.id}</span>
            <span className="badge badge-sm badge-ghost">{preview.version}</span>
            <span className="badge badge-sm badge-ghost">{preview.kind}</span>
            <span className="badge badge-sm badge-info">{preview.license}</span>
            <span className="badge badge-sm badge-ghost">
              来源默认信任级 {preview.source_default_trust}
            </span>
          </div>
          <p className="mt-1 opacity-70">
            建议确认强度：{preview.confirmation_label}
            {trust !== preview.source_default_trust
              ? `（当前选择 ${trust}，与来源默认不一致，以选择为准）`
              : ""}
          </p>
        </div>
      ) : null}
      {previewError ? (
        <p className="mt-2 text-xs text-error" role="alert" data-testid="install-preview-error">
          {previewError}
        </p>
      ) : null}
      {installError ? (
        <p className="mt-2 text-xs text-error" role="alert" data-testid="install-error">
          {installError}
        </p>
      ) : null}
      {installOk ? (
        <p className="mt-2 text-xs text-success" role="status" data-testid="install-ok">
          {installOk}
        </p>
      ) : null}
      <TrustConfirmDialog
        open={dialogOpen}
        onClose={() => setDialogOpen(false)}
        tier={tier}
        pluginId={preview?.id ?? ""}
        confirmText="确认安装"
        onConfirm={doInstall}
        busy={installBusy}
        contextNote={
          preview
            ? `来源 ${String((preview.source as { type?: string } | undefined)?.type ?? "?")}: ${JSON.stringify(preview.source ?? {})}`
            : undefined
        }
      />
    </section>
  );
}

export function PluginsPage() {
  const list = useApi<PluginList>("/api/plugins");
  const policy = useApi<TrustPolicy>("/api/plugins/trust-policy");
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
      <InstallPanel policy={policy.data ?? null} />
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
