import { useEffect, useMemo, useState } from "react";
import { useApi } from "../lib/hooks";
import { apiGet } from "../lib/api";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel, StatusBadge } from "../components/ui";
import { useUiText } from "../i18n";

interface PassiveRule {
  rule_id: string;
  rule_version: number;
  severity: string;
  class: string;
  description: string;
}
interface RulesData {
  rules: PassiveRule[];
  classes: string[];
  severities: string[];
  count: number;
  note?: string;
}
interface HitRow {
  hit_id: string;
  exchange_id: string;
  listener_id: string;
  rule_id: string;
  rule_version: number;
  severity: string;
  match_detail_json?: string;
  created_at: string;
}
interface HitsData {
  rows: HitRow[];
  next_cursor: string;
  has_more: boolean;
  total: number;
  limit: number;
  fingerprint: string;
}

const tones: Record<string, string> = {
  info: "badge-ghost",
  low: "badge-info",
  medium: "badge-warning",
  high: "badge-error",
};

export function PassiveAuditPage() {
  const ui = useUiText();
  const [severity, setSeverity] = useState("");
  const [ruleId, setRuleId] = useState("");
  const [exchangeId, setExchangeId] = useState("");
  const query = new URLSearchParams();
  if (severity) query.set("severity", severity);
  if (ruleId) query.set("rule_id", ruleId);
  if (exchangeId.trim()) query.set("exchange_id", exchangeId.trim());
  query.set("limit", "100");
  const suffix = query.toString();
  const rules = useApi<RulesData>("/api/wsw/passive/rules");
  const hits = useApi<HitsData>(`/api/wsw/passive/hits?${suffix}`);
  const [extraRows, setExtraRows] = useState<HitRow[]>([]);
  const [cursor, setCursor] = useState("");
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    setExtraRows([]);
    setCursor("");
    setLoadError(null);
  }, [suffix]);
  useEffect(() => {
    setCursor(hits.data?.next_cursor ?? "");
  }, [hits.data]);

  const rows = useMemo(() => [...(hits.data?.rows ?? []), ...extraRows], [hits.data, extraRows]);
  async function loadMore() {
    if (!cursor) return;
    try {
      const page = await apiGet<HitsData>(
        `/api/wsw/passive/hits?${suffix}&cursor=${encodeURIComponent(cursor)}`,
      );
      setExtraRows((old) => [...old, ...page.rows]);
      setCursor(page.next_cursor);
    } catch (error) {
      setLoadError(String(error));
    }
  }

  return (
    <div className="wsw-workspace-page" data-testid="wsw-passive-page">
      <PageHeader
        title={ui("Passive Audit")}
        actions={<span className="badge badge-outline">{ui("只读")}</span>}
      />
      <p className="text-sm text-muted">
        {ui(
          "被动规则只读取自 Rust 注册表投影；命中按捕获流量生成。此页面不会主动发送请求或提升 Finding。",
        )}
      </p>
      <section className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <article className="wsw-stat-card">
          <span>{ui("规则数")}</span>
          <strong>{rules.data?.count ?? "—"}</strong>
        </article>
        <article className="wsw-stat-card">
          <span>{ui("命中总数")}</span>
          <strong>{hits.data?.total ?? "—"}</strong>
        </article>
        <article className="wsw-stat-card">
          <span>{ui("当前筛选")}</span>
          <strong>{rows.length}</strong>
        </article>
        <article className="wsw-stat-card">
          <span>{ui("规则严重性")}</span>
          <strong>{rules.data?.severities.length ?? "—"}</strong>
        </article>
      </section>
      <section className="rounded-surface border border-edge bg-base-100 p-4">
        <h2 className="mb-3 text-sm font-semibold">{ui("规则注册表")}</h2>
        {rules.loading ? (
          <SkeletonPanel lines={3} />
        ) : rules.error ? (
          <ErrorPanel error={rules.error} onRetry={rules.retry} />
        ) : (
          <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
            {(rules.data?.rules ?? []).map((rule) => (
              <button
                key={rule.rule_id}
                type="button"
                className={`wsw-rule-card text-left ${ruleId === rule.rule_id ? "is-selected" : ""}`}
                onClick={() => setRuleId((old) => (old === rule.rule_id ? "" : rule.rule_id))}
              >
                <span className="flex items-center justify-between gap-2">
                  <strong>{rule.rule_id}</strong>
                  <StatusBadge status={rule.severity} tones={tones} />
                </span>
                <span className="text-xs text-muted">
                  {rule.class} · v{rule.rule_version}
                </span>
                <span className="text-xs">{rule.description}</span>
              </button>
            ))}
          </div>
        )}
        {rules.data?.note ? <p className="mt-3 text-xs text-muted">{rules.data.note}</p> : null}
      </section>
      <section className="rounded-surface border border-edge bg-base-100 p-4">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-sm font-semibold">{ui("审计命中")}</h2>
          <span className="text-xs text-muted">
            {hits.data?.total ?? 0} {ui("条记录")}
          </span>
        </div>
        <div className="mb-4 grid gap-2 sm:grid-cols-2 xl:grid-cols-4">
          <label className="wsw-field">
            <span>{ui("严重性")}</span>
            <select
              className="select select-bordered select-sm"
              aria-label={ui("严重性")}
              value={severity}
              onChange={(e) => setSeverity(e.target.value)}
            >
              <option value="">{ui("全部")}</option>
              {(rules.data?.severities ?? ["info", "low", "medium", "high"]).map((v) => (
                <option key={v}>{v}</option>
              ))}
            </select>
          </label>
          <label className="wsw-field">
            <span>{ui("规则")}</span>
            <select
              className="select select-bordered select-sm"
              aria-label={ui("规则")}
              value={ruleId}
              onChange={(e) => setRuleId(e.target.value)}
            >
              <option value="">{ui("全部规则")}</option>
              {(rules.data?.rules ?? []).map((rule) => (
                <option key={rule.rule_id} value={rule.rule_id}>
                  {rule.rule_id}
                </option>
              ))}
            </select>
          </label>
          <label className="wsw-field sm:col-span-2">
            <span>{ui("Exchange ID")}</span>
            <input
              className="input input-bordered input-sm"
              aria-label="Exchange ID"
              value={exchangeId}
              onChange={(e) => setExchangeId(e.target.value)}
              placeholder="wsw-…"
            />
          </label>
        </div>
        {hits.loading ? (
          <SkeletonPanel lines={4} />
        ) : hits.error ? (
          <ErrorPanel error={hits.error} onRetry={hits.retry} />
        ) : rows.length === 0 ? (
          <EmptyPanel
            text={ui(
              "当前筛选下没有审计命中。等待代理捕获符合条件的响应后，这里会显示被动规则命中。",
            )}
          />
        ) : (
          <div className="overflow-x-auto">
            <table className="table table-sm w-full">
              <thead>
                <tr>
                  <th>{ui("严重性")}</th>
                  <th>{ui("规则")}</th>
                  <th>Exchange</th>
                  <th>{ui("时间")}</th>
                  <th>Hit ID</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr key={row.hit_id}>
                    <td>
                      <StatusBadge status={row.severity} tones={tones} />
                    </td>
                    <td className="font-mono text-xs">{row.rule_id}</td>
                    <td className="max-w-56 truncate font-mono text-xs">{row.exchange_id}</td>
                    <td className="whitespace-nowrap text-xs">{row.created_at}</td>
                    <td className="font-mono text-xs">{row.hit_id}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {loadError ? (
          <p role="alert" className="mt-2 text-xs text-error">
            {loadError}
          </p>
        ) : null}
        {cursor ? (
          <button type="button" className="btn btn-sm mt-3" onClick={() => void loadMore()}>
            {ui("加载更多")}
          </button>
        ) : null}
      </section>
    </div>
  );
}
