import { useState } from "react";
import { Link, useParams } from "react-router";
import {
  pluginAction,
  pluginCleanupRollback,
  pluginInstall,
  pluginUpgrade,
  resolveTrustTier,
  type PluginConfirmFields,
  type TrustPolicy,
} from "../lib/api";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { TrustConfirmDialog } from "../components/TrustConfirmDialog";
import { formatDateTime } from "../lib/format";
import type { PluginDetail } from "../lib/api";

/** 插件详情（P6-8，三级按需加载 PLUGIN-CONTRACT §9）：默认 L2（context
 *  summary + capability 清单 + compatibility + dependencies），“载入完整
 *  Manifest 与 docs”按钮升级到 L3（三级加载纪律：目录级列表只在 L1，正文
 *  与 Schema 只在 L3 按需）。操作按钮（enable/disable/uninstall/ingest/
 *  upgrade）走 /api/plugins 写端点（rp-plugin 子进程，状态机 + audit）。 */

const STATE_BADGE: Record<string, string> = {
  ENABLED: "badge-success",
  LOADED: "badge-success",
  ACTIVE: "badge-success",
  REGISTERED: "badge-primary",
  DISABLED: "badge-warning",
  INVALID: "badge-error",
  ERROR: "badge-error",
  UNINSTALLED: "badge-ghost",
};

function CompatTable({ compat }: { compat: NonNullable<PluginDetail["compatibility"]> }) {
  return (
    <table className="table table-xs" data-testid="compat-table">
      <thead>
        <tr>
          <th>兼容项</th>
          <th>声明</th>
        </tr>
      </thead>
      <tbody>
        <tr>
          <td>rip_core</td>
          <td className="font-mono">{compat.rip_core ?? "-"}</td>
        </tr>
        <tr>
          <td>manifest_schema</td>
          <td className="font-mono">{compat.manifest_schema?.join(", ") ?? "-"}</td>
        </tr>
        {compat.worker_protocol ? (
          <tr>
            <td>worker_protocol</td>
            <td className="font-mono">{compat.worker_protocol}</td>
          </tr>
        ) : null}
        {compat.runtimes?.length ? (
          <tr>
            <td>runtimes</td>
            <td className="font-mono">{compat.runtimes.join(", ")}</td>
          </tr>
        ) : null}
      </tbody>
    </table>
  );
}

/** 依赖图（列表形态：本插件依赖谁 + 谁依赖本插件由目录侧反查，此处先
 *  呈现声明面 dependencies 与 capability requires 的有向清单）。 */
function DependencyGraph({ detail }: { detail: PluginDetail }) {
  const deps = detail.dependencies ?? [];
  const requires = (detail.capabilities ?? []).flatMap((cap) =>
    (cap.requires ?? []).map((requirement) => ({ from: cap.capability_id, requirement })),
  );
  if (deps.length === 0 && requires.length === 0) {
    return <p className="text-sm opacity-70">无声明依赖</p>;
  }
  return (
    <div className="space-y-2 text-sm" data-testid="dependency-graph">
      {deps.map((dep) => (
        <div key={dep.id} className="flex items-center gap-2">
          <span className="font-mono">{detail.plugin_id}</span>
          <span aria-hidden="true">→</span>
          <span className="font-mono">{dep.id}</span>
          <span className="badge badge-sm badge-ghost">{dep.requirement}</span>
        </div>
      ))}
      {requires.map((edge) => (
        <div key={`${edge.from}/${edge.requirement}`} className="flex items-center gap-2">
          <span className="font-mono">{edge.from}</span>
          <span aria-hidden="true">→</span>
          <span className="font-mono">{edge.requirement}</span>
          <span className="badge badge-sm badge-ghost">capability requires</span>
        </div>
      ))}
    </div>
  );
}

type ConfirmKind = "install" | "upgrade" | null;

export function PluginDetailPage() {
  const { pluginId = "" } = useParams();
  const [level, setLevel] = useState<1 | 2 | 3>(2);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionBusy, setActionBusy] = useState(false);
  const [confirmKind, setConfirmKind] = useState<ConfirmKind>(null);
  const [cleanupBusy, setCleanupBusy] = useState(false);
  const [cleanupNote, setCleanupNote] = useState<string | null>(null);
  const detail = useApi<PluginDetail>(
    `/api/plugins/${encodeURIComponent(pluginId)}?level=${level}`,
  );
  const policy = useApi<TrustPolicy>("/api/plugins/trust-policy");

  async function runAction(action: "enable" | "disable" | "uninstall" | "ingest") {
    setActionError(null);
    setActionBusy(true);
    try {
      await pluginAction(action, pluginId);
      await detail.retry();
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setActionBusy(false);
    }
  }

  // P10-2：install/upgrade 一律走分级确认对话框（确认强度按信任级策略表）。
  async function runConfirmedInstall(fields: PluginConfirmFields) {
    setConfirmKind(null);
    setActionError(null);
    setActionBusy(true);
    try {
      const manifestPath = `schemas/plugin-manifest/examples/${pluginId}.yaml`;
      await pluginInstall(manifestPath, "local", fields);
      await detail.retry();
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setActionBusy(false);
    }
  }

  async function runConfirmedUpgrade(fields: PluginConfirmFields) {
    setConfirmKind(null);
    setActionError(null);
    setActionBusy(true);
    try {
      // 升级 Manifest 默认取同目录 0.2.0 命名（P6-2 升级语义：新旧并存）；
      // 真实路径由维护者在插件包内准备。升级信任级由服务端沿用当前插件。
      const manifestPath = `schemas/plugin-manifest/examples/${pluginId}-0.2.0.yaml`;
      await pluginUpgrade(pluginId, manifestPath, fields);
      await detail.retry();
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setActionBusy(false);
    }
  }

  async function runCleanupRollback() {
    setCleanupNote(null);
    setCleanupBusy(true);
    try {
      const res = await pluginCleanupRollback(pluginId);
      const report = (res.result?.report ?? {}) as {
        cleaned?: string[];
        retained?: string[];
      };
      setCleanupNote(
        `回滚窗口清理完成：清理 ${(report.cleaned ?? []).length} 个旧版本，保留 ${
          (report.retained ?? []).length
        } 个历史版本`,
      );
      await detail.retry();
    } catch (error) {
      setCleanupNote(error instanceof Error ? error.message : String(error));
    } finally {
      setCleanupBusy(false);
    }
  }

  if (detail.loading) return <SkeletonPanel lines={5} />;
  if (detail.error) return <ErrorPanel error={detail.error} onRetry={detail.retry} />;
  const data = detail.data;
  if (!data?.plugin_id) {
    return (
      <div>
        <PageHeader title="插件详情" />
        <EmptyPanel text={`未找到插件：${pluginId}`} />
      </div>
    );
  }

  const stateBadge = STATE_BADGE[data.state] ?? "badge-ghost";
  const canEnable = ["REGISTERED", "DISABLED", "ERROR"].includes(data.state);
  const canDisable = data.state === "ENABLED" || data.state === "LOADED" || data.state === "ACTIVE";
  const canUninstall = ["REGISTERED", "ENABLED", "DISABLED"].includes(data.state);
  const canIngest = (data.capabilities ?? []).some(
    (cap) => cap.capability_type === "knowledge_source",
  );
  // P10-2：确认强度按插件当前信任级（install 为 local 安装语义）。
  const upgradeTier = resolveTrustTier(policy.data ?? null, data.trust_level ?? "local");
  const installTier = resolveTrustTier(policy.data ?? null, "local");

  return (
    <div>
      <PageHeader
        title={
          <>
            <Link to="/plugins" className="mr-2 text-sm opacity-70 hover:opacity-100">
              ← 插件中心
            </Link>
            {data.name}
          </>
        }
        actions={
          <div className="flex flex-wrap items-center gap-2">
            <span className={`badge ${stateBadge}`} data-testid="detail-state">
              {data.state}
            </span>
            {data.update_pending ? <span className="badge badge-warning">可更新</span> : null}
            {canEnable ? (
              <button
                className="btn btn-sm btn-primary"
                disabled={actionBusy}
                onClick={() => runAction("enable")}
                data-testid="action-enable"
              >
                启用
              </button>
            ) : null}
            {canDisable ? (
              <button
                className="btn btn-sm btn-warning"
                disabled={actionBusy}
                onClick={() => runAction("disable")}
                data-testid="action-disable"
              >
                禁用
              </button>
            ) : null}
            {data.state === "UNINSTALLED" || !data.is_active ? (
              <button
                className="btn btn-sm btn-primary"
                disabled={actionBusy}
                onClick={() => setConfirmKind("install")}
                data-testid="action-install"
              >
                安装
              </button>
            ) : null}
            {data.update_pending ? (
              <button
                className="btn btn-sm btn-secondary"
                disabled={actionBusy}
                onClick={() => setConfirmKind("upgrade")}
                data-testid="action-upgrade"
              >
                升级
              </button>
            ) : null}
            {canUninstall ? (
              <button
                className="btn btn-sm btn-error"
                disabled={actionBusy}
                onClick={() => runAction("uninstall")}
                data-testid="action-uninstall"
              >
                卸载
              </button>
            ) : null}
            {canIngest && data.state !== "UNINSTALLED" ? (
              <button
                className="btn btn-sm btn-ghost"
                disabled={actionBusy}
                onClick={() => runAction("ingest")}
                data-testid="action-ingest"
              >
                摄取
              </button>
            ) : null}
            {data.is_active && data.state !== "UNINSTALLED" ? (
              <button
                className="btn btn-sm btn-ghost"
                disabled={cleanupBusy || actionBusy}
                onClick={runCleanupRollback}
                title="P10-2 D6：回滚窗口清理（清理上一版本旧目录，active 版本不动）"
                data-testid="action-cleanup-rollback"
              >
                清理旧版本
              </button>
            ) : null}
          </div>
        }
      />
      {actionError ? (
        <div
          role="alert"
          className="mb-4 rounded-control border border-error bg-error/10 p-3 text-sm"
        >
          {actionError}
        </div>
      ) : null}
      {cleanupNote ? (
        <div
          role="status"
          className="mb-4 rounded-control border border-info bg-info/10 p-3 text-sm"
          data-testid="cleanup-note"
        >
          {cleanupNote}
        </div>
      ) : null}
      <TrustConfirmDialog
        open={confirmKind === "install"}
        onClose={() => setConfirmKind(null)}
        tier={installTier}
        pluginId={pluginId}
        confirmText="确认安装"
        onConfirm={runConfirmedInstall}
        busy={actionBusy}
        contextNote={
          data ? `本地来源安装：schemas/plugin-manifest/examples/${pluginId}.yaml` : undefined
        }
      />
      <TrustConfirmDialog
        open={confirmKind === "upgrade"}
        onClose={() => setConfirmKind(null)}
        tier={upgradeTier}
        pluginId={pluginId}
        confirmText="确认升级"
        onConfirm={runConfirmedUpgrade}
        busy={actionBusy}
        contextNote={`升级确认强度按当前信任级 ${data.trust_level}（策略表单一来源 plugin-registry::trust_policy）`}
      />

      <div className="mb-4 flex items-center gap-2 text-xs" data-testid="level-switch">
        <span className="opacity-70">加载级别：</span>
        {[1, 2, 3].map((candidate) => (
          <button
            key={candidate}
            className={`btn btn-xs ${level === candidate ? "btn-primary" : "btn-ghost"}`}
            onClick={() => setLevel(candidate as 1 | 2 | 3)}
          >
            L{candidate}
          </button>
        ))}
        <span className="text-muted">
          {level === 1
            ? "目录级元数据"
            : level === 2
              ? "匹配级摘要与清单"
              : "调用级完整 Manifest 与 docs"}
        </span>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <section className="rounded-control border border-edge bg-base-100 p-4">
          <h2 className="mb-2 text-sm font-semibold">元数据</h2>
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
            <dt className="opacity-70">版本</dt>
            <dd className="font-mono">{data.version}</dd>
            <dt className="opacity-70">许可证</dt>
            <dd data-testid="detail-license">{data.license}</dd>
            <dt className="opacity-70">信任级</dt>
            <dd>{data.trust_level}</dd>
            <dt className="opacity-70">来源</dt>
            <dd className="font-mono text-xs">
              {data.source_type}:{data.source_ref}
            </dd>
            {data.homepage ? (
              <>
                <dt className="opacity-70">来源主页</dt>
                <dd className="text-xs">
                  <a
                    href={data.homepage}
                    target="_blank"
                    rel="noreferrer"
                    className="break-all text-primary underline underline-offset-2"
                    data-testid="detail-homepage"
                  >
                    {data.homepage}
                  </a>
                </dd>
              </>
            ) : null}
            {data.pinned?.value ? (
              <>
                <dt className="opacity-70">快照 Commit</dt>
                <dd className="font-mono text-xs" data-testid="detail-commit">
                  {data.pinned.kind}: {data.pinned.value}
                </dd>
              </>
            ) : null}
            <dt className="opacity-70">lock sha256</dt>
            <dd className="font-mono text-xs">{data.lock_sha256 ?? "-"}</dd>
            <dt className="opacity-70">更新时间</dt>
            <dd>{data.updated_at ? formatDateTime(data.updated_at) : "-"}</dd>
            {data.source_uninstalled ? (
              <>
                <dt className="opacity-70">来源状态</dt>
                <dd>已卸载（已入主库知识保留带溯源）</dd>
              </>
            ) : null}
          </dl>
          {level >= 2 && data.knowledge_provides?.content_kinds?.length ? (
            <div className="mt-3" data-testid="detail-knowledge-provides">
              <h3 className="text-xs font-semibold text-muted">内容类型与许可策略</h3>
              <div className="mt-1 flex flex-wrap items-center gap-1">
                {data.knowledge_provides.content_kinds.map((kind) => (
                  <span
                    key={kind}
                    className="badge badge-sm badge-outline"
                    title={
                      data.knowledge_provides?.kinds?.[kind]?.license_policy
                        ? `${kind} policy: ${data.knowledge_provides?.kinds?.[kind]?.license_policy}`
                        : kind
                    }
                  >
                    {kind}
                    {data.knowledge_provides?.kinds?.[kind]?.license_policy
                      ? ` · ${data.knowledge_provides?.kinds?.[kind]?.license_policy}`
                      : ""}
                  </span>
                ))}
              </div>
            </div>
          ) : null}
          {level >= 2 && data.knowledge_stats ? (
            <div className="mt-3" data-testid="detail-knowledge-stats">
              <h3 className="text-xs font-semibold text-muted">审核状态</h3>
              <div className="mt-1 flex flex-wrap items-center gap-1">
                {Object.entries(data.knowledge_stats.review ?? {}).map(([status, n]) => (
                  <span key={status} className="badge badge-sm badge-ghost">
                    {status}: {n}
                  </span>
                ))}
                {data.knowledge_stats.last_seen_at ? (
                  <span className="text-[11px] text-muted">
                    最近摄取 {formatDateTime(data.knowledge_stats.last_seen_at)}
                  </span>
                ) : null}
              </div>
            </div>
          ) : null}
          {level >= 2 && data.context?.summary ? (
            <p className="mt-3 text-sm" data-testid="context-summary">
              {data.context.summary}
            </p>
          ) : null}
        </section>

        <section className="rounded-control border border-edge bg-base-100 p-4">
          <h2 className="mb-2 text-sm font-semibold">兼容性</h2>
          {data.compatibility ? (
            <CompatTable compat={data.compatibility} />
          ) : (
            <p className="text-sm opacity-70">未声明</p>
          )}
        </section>

        <section className="rounded-control border border-edge bg-base-100 p-4">
          <h2 className="mb-2 text-sm font-semibold">
            capability 清单（{(data.capabilities ?? []).length}）
          </h2>
          {(data.capabilities ?? []).length === 0 ? (
            <p className="text-sm opacity-70">L2 起展示（capability 行随版本落库）</p>
          ) : (
            <ul className="space-y-2" data-testid="capability-list">
              {(data.capabilities ?? []).map((cap) => (
                <li
                  key={cap.capability_id}
                  className="rounded-control border border-edge p-2 text-sm"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-mono text-xs">{cap.capability_id}</span>
                    <span className="badge badge-sm badge-ghost">{cap.capability_type}</span>
                  </div>
                  <p className="mt-1 opacity-80">{cap.summary}</p>
                  <p className="mt-1 font-mono text-xs text-muted">entry: {cap.entry}</p>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="rounded-control border border-edge bg-base-100 p-4">
          <h2 className="mb-2 text-sm font-semibold">依赖图</h2>
          <DependencyGraph detail={data} />
        </section>
      </div>

      {level >= 3 ? (
        <>
          <section className="mt-4 rounded-control border border-edge bg-base-100 p-4">
            <h2 className="mb-2 text-sm font-semibold">docs（L3）</h2>
            <pre className="max-h-72 overflow-auto whitespace-pre-wrap font-mono text-xs">
              {data.docs?.body ?? "(无 docs 正文)"}
            </pre>
          </section>
          <section className="mt-4 rounded-control border border-edge bg-base-100 p-4">
            <h2 className="mb-2 text-sm font-semibold">Manifest（L3 原始 JSON）</h2>
            <pre className="max-h-96 overflow-auto font-mono text-xs" data-testid="manifest-json">
              {JSON.stringify(data.manifest, null, 2)}
            </pre>
          </section>
        </>
      ) : null}

      <section className="mt-4 rounded-control border border-edge bg-base-100 p-4">
        <h2 className="mb-2 text-sm font-semibold">审计事件（plugin.*，最近 50）</h2>
        {(data.audit ?? []).length === 0 ? (
          <p className="text-sm opacity-70">无审计事件（audit-engine 经 rp-plugin 子进程写入）</p>
        ) : (
          <ul className="space-y-1 font-mono text-xs" data-testid="audit-trail">
            {(data.audit ?? []).map((event) => (
              <li key={event.event_id}>
                {event.ts} [{event.actor}] {event.action} {event.entity_id}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
