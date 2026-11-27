import { translateUi } from "../i18n";
import { useCallback, useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import {
  type KnowledgeSourceEntry,
  type KnowledgeSourceList,
  type KnowledgeSourcePlugin,
  reviewKnowledgeSourceEntries,
} from "../lib/api";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { formatDateTime } from "../lib/format";

/** 知识库页（P5-6）：列表 + 关键词检索（/api/knowledge?q=，服务端 LIKE，
 *  q 为 keyword 别名）+ 详情侧栏（来源/元数据/正文摘要）。
 *  检索 300ms 防抖（useApi 按路径重取，SWR 旧数据留屏）；三态统一：
 *  loading= SkeletonPanel、error= ErrorPanel（可重试）、无命中= EmptyPanel
 *  显服务端真实空态文案。详情按 ?knowledge_id= 全字段拉取。
 *  SK-2（P6-10 第 6 条）：新增「知识来源」视图，展示插件知识条目
 *  （knowledge_source_entry）的来源主页、分类、许可证状态、快照 Commit 与
 *  审核状态；平台知识面（M 库 knowledge 表）行为不变。 */

interface KnowledgeRow {
  knowledge_id: string;
  record_kind?: string;
  lifecycle?: string;
  governance?: string;
  title: string;
  component?: string | null;
  cve?: string | null;
  severity?: string | null;
  language?: string | null;
  evidence_level?: string | null;
  verified?: number | null;
  hit_count?: number | null;
  updated_at?: string | null;
}

interface KnowledgeList {
  available?: boolean;
  total?: number;
  returned?: number;
  message?: string;
  items?: KnowledgeRow[];
}

interface KnowledgeItem extends KnowledgeRow {
  description?: string | null;
  usage?: string | null;
  content?: string | null;
  source?: string | null;
  source_url?: string | null;
  verified_note?: string | null;
  tags?: unknown;
  versions?: string | null;
}

interface KnowledgeDetail {
  available?: boolean;
  source?: string;
  item?: KnowledgeItem;
}

const LIFECYCLE_BADGE: Record<string, string> = {
  VERIFIED: "badge-success",
  CANDIDATE: "badge-warning",
  DISCOVERED: "badge-ghost",
  INDEXED: "badge-info",
  STALE: "badge-error",
  FETCHED: "badge-ghost",
  PARSED: "badge-ghost",
};

function verifiedLabel(verified: number | null | undefined): string {
  if (verified === 1) return translateUi("已验证");
  if (verified === 0) return translateUi("未验证");
  return translateUi("未标注");
}

function truncate(text: string | null | undefined, max: number): string | null {
  if (!text) return null;
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

function stringList(value: string | string[] | undefined): string[] {
  if (Array.isArray(value)) return value;
  return value ? [value] : [];
}

function MetaRow({ label, value }: { label: string; value: string | null | undefined }) {
  if (value === null || value === undefined || value === "") return null;
  return (
    <div className="flex gap-2 text-xs">
      <span className="w-20 shrink-0 text-muted">{label}</span>
      <span className="min-w-0 flex-1 break-words">{value}</span>
    </div>
  );
}

function DetailSidebar({
  detail,
  selectedId,
}: {
  detail: ReturnType<typeof useApi<KnowledgeDetail>>;
  selectedId: string | null;
}) {
  if (!selectedId) {
    return <EmptyPanel text={translateUi("从左侧选择条目查看详情（来源、元数据与正文摘要）。")} />;
  }
  if (detail.loading) {
    return (
      <div aria-busy="true" className="space-y-3 p-1">
        <div className="skeleton h-5 w-3/4" />
        <div className="skeleton h-16 w-full" />
        <div className="skeleton h-24 w-full" />
      </div>
    );
  }
  if (detail.error !== null) {
    return <ErrorPanel error={detail.error} onRetry={detail.retry} />;
  }
  const item = detail.data?.item;
  if (!item) return <EmptyPanel text={translateUi("条目详情不存在。")} />;
  const tags = Array.isArray(item.tags)
    ? (item.tags as unknown[]).filter((t) => typeof t === "string")
    : [];
  return (
    <div className="space-y-3">
      <div>
        <div className="flex flex-wrap items-center gap-1">
          {item.lifecycle ? (
            <span className={`badge badge-sm ${LIFECYCLE_BADGE[item.lifecycle] ?? "badge-ghost"}`}>
              {item.lifecycle}
            </span>
          ) : null}
          <span className="badge badge-sm badge-outline">{verifiedLabel(item.verified)}</span>
          {item.record_kind === "negative" ? (
            <span className="badge badge-sm badge-error">{translateUi("反知识")}</span>
          ) : null}
        </div>
        <h2 className="mt-2 text-sm font-semibold leading-5">{item.title}</h2>
        <p className="mt-1 break-all text-[11px] text-muted">{item.knowledge_id}</p>
      </div>

      {item.description ? (
        <section>
          <h3 className="text-xs font-semibold text-muted">{translateUi("描述")}</h3>
          <p className="mt-1 whitespace-pre-wrap text-xs leading-5">{item.description}</p>
        </section>
      ) : null}

      <section>
        <h3 className="text-xs font-semibold text-muted">{translateUi("元数据")}</h3>
        <div className="mt-1 space-y-1">
          <MetaRow label={translateUi("组件")} value={item.component} />
          <MetaRow label="CVE" value={item.cve} />
          <MetaRow label={translateUi("等级")} value={item.severity} />
          <MetaRow label={translateUi("语言")} value={item.language} />
          <MetaRow label={translateUi("证据级")} value={item.evidence_level} />
          <MetaRow
            label={translateUi("命中")}
            value={
              item.hit_count === null || item.hit_count === undefined
                ? null
                : String(item.hit_count)
            }
          />
          <MetaRow
            label={translateUi("更新")}
            value={item.updated_at ? formatDateTime(item.updated_at) : null}
          />
          <MetaRow label={translateUi("验证注记")} value={truncate(item.verified_note, 200)} />
        </div>
        {tags.length > 0 ? (
          <div className="mt-2 flex flex-wrap gap-1">
            {tags.map((tag) => (
              <span key={tag as string} className="badge badge-ghost badge-sm">
                {tag as string}
              </span>
            ))}
          </div>
        ) : null}
      </section>

      <section>
        <h3 className="text-xs font-semibold text-muted">{translateUi("来源")}</h3>
        <div className="mt-1 space-y-1">
          <MetaRow label={translateUi("来源")} value={item.source} />
          {item.source_url ? (
            <div className="flex gap-2 text-xs">
              <span className="w-20 shrink-0 text-muted">{translateUi("链接")}</span>
              <a
                href={item.source_url}
                target="_blank"
                rel="noreferrer"
                className="min-w-0 flex-1 break-all text-primary underline underline-offset-2"
              >
                {item.source_url}
              </a>
            </div>
          ) : null}
        </div>
      </section>

      <section>
        <h3 className="text-xs font-semibold text-muted">{translateUi("正文摘要")}</h3>
        {item.content ? (
          <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap rounded-control bg-base-200 p-2 text-[11px] leading-4">
            {truncate(item.content, 1200)}
          </pre>
        ) : item.usage ? (
          <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap rounded-control bg-base-200 p-2 text-[11px] leading-4">
            {truncate(item.usage, 1200)}
          </pre>
        ) : (
          <p className="mt-1 text-xs text-muted">
            {translateUi("该条目无正文内容（仅元数据记录）。")}
          </p>
        )}
      </section>
    </div>
  );
}

const REVIEW_BADGE: Record<string, string> = {
  APPROVED: "badge-success",
  DISCOVERED: "badge-ghost",
  UNREVIEWED: "badge-warning",
  REJECTED: "badge-error",
  DEPRECATED: "badge-ghost",
};

const RIGHTS_LABEL: Record<string, string> = {
  catalog_only: "仅目录元数据",
  approved_only: "审核后正文",
  metadata_only: "仅元数据与哈希",
};

const ENTRY_KIND_LABEL: Record<string, string> = {
  catalog_entry: "目录条目",
  document_markdown: "文档正文",
  skill_candidate: "Skill 候选",
  nuclei_template: "Nuclei 模板",
};

function SourceEntriesView() {
  const [pluginId, setPluginId] = useState("");
  const [review, setReview] = useState("");
  const [rawQuery, setRawQuery] = useState("");
  const [keyword, setKeyword] = useState("");
  const [selectedEntries, setSelectedEntries] = useState<
    Array<{ plugin_id: string; plugin_version: string; item_key: string }>
  >([]);
  const [reviewReason, setReviewReason] = useState("");
  const [reviewSubmitting, setReviewSubmitting] = useState(false);
  const [reviewError, setReviewError] = useState<string | null>(null);

  useEffect(() => {
    const timer = setTimeout(() => setKeyword(rawQuery.trim()), 300);
    return () => clearTimeout(timer);
  }, [rawQuery]);

  useEffect(() => {
    setSelectedEntries([]);
    setReviewError(null);
  }, [pluginId, review, keyword]);

  const qs = new URLSearchParams({ limit: "100" });
  if (pluginId) qs.set("plugin_id", pluginId);
  if (review) qs.set("review_status", review);
  if (keyword) qs.set("q", keyword);
  const list = useApi<KnowledgeSourceList>(`/api/knowledge_sources?${qs.toString()}`);

  const plugins: KnowledgeSourcePlugin[] = list.data?.plugins ?? [];
  const items: KnowledgeSourceEntry[] = list.data?.items ?? [];
  const total = list.data?.total ?? 0;
  const reviewHistory = list.data?.review_history ?? [];
  const selectedPluginId = selectedEntries[0]?.plugin_id ?? "";
  const selectedVersion = selectedEntries[0]?.plugin_version ?? "";
  const selectedCount = selectedEntries.length;

  const toggleSelected = (item: KnowledgeSourceEntry) => {
    setSelectedEntries((current) => {
      const exists = current.some(
        (entry) => entry.plugin_id === item.plugin_id && entry.item_key === item.item_key,
      );
      if (exists) {
        return current.filter(
          (entry) => entry.plugin_id !== item.plugin_id || entry.item_key !== item.item_key,
        );
      }
      if (
        current.length > 0 &&
        (current[0]?.plugin_id !== item.plugin_id ||
          current[0]?.plugin_version !== item.plugin_version)
      )
        return current;
      return [
        ...current,
        { plugin_id: item.plugin_id, plugin_version: item.plugin_version, item_key: item.item_key },
      ];
    });
  };

  const submitReview = async (status: "APPROVED" | "REJECTED") => {
    if (!selectedPluginId || !selectedVersion || selectedCount === 0 || reviewSubmitting) return;
    if (!reviewReason.trim()) {
      setReviewError(translateUi("审核理由为必填项"));
      return;
    }
    setReviewSubmitting(true);
    setReviewError(null);
    try {
      await reviewKnowledgeSourceEntries(
        selectedPluginId,
        selectedEntries.map((entry) => entry.item_key),
        status,
        selectedVersion,
        reviewReason.trim(),
      );
      setSelectedEntries([]);
      setReviewReason("");
      await list.retry();
    } catch (error) {
      setReviewError(error instanceof Error ? error.message : translateUi("审核操作失败"));
    } finally {
      setReviewSubmitting(false);
    }
  };

  return (
    <section aria-label={translateUi("知识来源条目")}>
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <select
          className="select select-sm rounded-control border border-edge bg-base-100"
          aria-label={translateUi("按来源插件过滤")}
          value={pluginId}
          onChange={(e) => setPluginId(e.target.value)}
        >
          <option value="">
            {translateUi("全部来源（")} {plugins.length} {translateUi("个插件）")}
          </option>
          {plugins.map((p) => (
            <option key={p.plugin_id} value={p.plugin_id}>
              {p.name}（{p.license}）
            </option>
          ))}
        </select>
        <select
          className="select select-sm rounded-control border border-edge bg-base-100"
          aria-label={translateUi("按审核状态过滤")}
          value={review}
          onChange={(e) => setReview(e.target.value)}
        >
          <option value="">{translateUi("全部审核状态")}</option>
          {Object.keys(REVIEW_BADGE).map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
        <input
          type="text"
          className="input input-sm w-52 rounded-control border border-edge bg-base-100"
          placeholder={translateUi("检索标题 / Template ID / URL / 描述…")}
          aria-label={translateUi("检索知识来源条目")}
          value={rawQuery}
          onChange={(e) => setRawQuery(e.target.value)}
        />
        <span className="text-xs text-muted" data-testid="source-total">
          {translateUi("共")} {total} {translateUi("条（显示")} {items.length}）
        </span>
      </div>
      <div
        className="mb-3 flex flex-wrap items-center gap-2 rounded-surface border border-edge bg-base-100 p-2"
        data-testid="knowledge-source-review-actions"
      >
        <span className="text-xs text-muted">
          {translateUi("已选择")} {selectedCount} {translateUi("条")}
          {selectedPluginId ? ` · ${selectedPluginId}@${selectedVersion}` : ""}
        </span>
        <input
          className="input input-xs min-w-52 flex-1 rounded-control border border-edge bg-base-100"
          aria-label={translateUi("审核理由")}
          placeholder={translateUi("请输入审核理由（必填）")}
          value={reviewReason}
          onChange={(event) => setReviewReason(event.target.value)}
          maxLength={1000}
        />
        <button
          type="button"
          className="btn btn-xs btn-primary"
          disabled={
            selectedCount === 0 || !selectedVersion || !reviewReason.trim() || reviewSubmitting
          }
          onClick={() => void submitReview("APPROVED")}
        >
          {reviewSubmitting ? translateUi("提交中…") : translateUi("批准所选")}
        </button>
        <button
          type="button"
          className="btn btn-xs btn-outline btn-error"
          disabled={
            selectedCount === 0 || !selectedVersion || !reviewReason.trim() || reviewSubmitting
          }
          onClick={() => void submitReview("REJECTED")}
        >
          {translateUi("驳回所选")}
        </button>
        <button
          type="button"
          className="btn btn-xs btn-ghost"
          disabled={selectedCount === 0 || reviewSubmitting}
          onClick={() => setSelectedEntries([])}
        >
          {translateUi("清空选择")}
        </button>
        <span className="text-[11px] text-muted">
          {translateUi("审核操作会写入审计历史；批准后才允许进入搜索与模型上下文。")}
          {translateUi("每次批量审核仅支持同一插件，最多 100 条。")}
        </span>
        {reviewError ? (
          <p className="w-full text-xs text-error" role="alert">
            {reviewError}
          </p>
        ) : null}
      </div>
      {list.loading ? (
        <SkeletonPanel lines={6} />
      ) : list.error !== null ? (
        <ErrorPanel error={list.error} onRetry={list.retry} />
      ) : !list.data?.available ? (
        <EmptyPanel
          text={
            list.data?.message ?? translateUi("知识来源数据面不可用（需 011 迁移的插件库副本）")
          }
        />
      ) : items.length === 0 ? (
        <EmptyPanel text={translateUi("没有匹配的知识来源条目，调整过滤条件后重试。")} />
      ) : (
        <ul className="space-y-2" data-testid="source-entry-list">
          {items.map((item) => (
            <li key={`${item.plugin_id}@${item.entry_kind}:${item.item_key}`}>
              <div className="rounded-surface border border-edge bg-base-100 p-3">
                <div className="flex flex-wrap items-center gap-2">
                  <input
                    type="checkbox"
                    className="checkbox checkbox-xs"
                    aria-label={`${translateUi("选择条目")} ${item.item_key}`}
                    checked={selectedEntries.some(
                      (entry) =>
                        entry.plugin_id === item.plugin_id && entry.item_key === item.item_key,
                    )}
                    onChange={() => toggleSelected(item)}
                  />
                  <div className="flex flex-wrap items-center gap-1">
                    <span
                      className={`badge badge-sm ${REVIEW_BADGE[item.review_status] ?? "badge-ghost"}`}
                    >
                      {item.review_status}
                    </span>
                    <span className="badge badge-sm badge-outline">
                      {ENTRY_KIND_LABEL[item.entry_kind] ?? item.entry_kind}
                    </span>
                    <span className="badge badge-sm badge-info" data-testid="source-license">
                      {item.license_status}
                    </span>
                    <span className="badge badge-sm badge-ghost">
                      {RIGHTS_LABEL[item.rights_policy] ?? item.rights_policy}
                    </span>
                    {item.source_commit ? (
                      <span
                        className="badge badge-sm badge-ghost font-mono"
                        title={`快照 Commit: ${item.source_commit}`}
                      >
                        {item.source_commit.slice(0, 7)}
                      </span>
                    ) : null}
                    <span className="text-[11px] text-muted">{item.plugin_id}</span>
                  </div>
                </div>
                <p className="mt-1.5 text-sm font-medium leading-5" data-testid="source-title">
                  {item.title}
                </p>
                <p
                  className="mt-1 break-all font-mono text-[10px] text-muted"
                  data-testid="source-item-key"
                >
                  {item.item_key}
                </p>
                {item.cve_record ? (
                  <div className="mt-2 space-y-1.5 text-[11px]" data-testid="cve-record-metadata">
                    <div className="flex flex-wrap items-center gap-1.5">
                      {item.cve_record.known_exploited ? (
                        <span className="badge badge-sm badge-error">
                          CISA KEV · {translateUi("已知在野利用")}
                        </span>
                      ) : null}
                      {item.cve_record.cve_id ? (
                        <span className="font-mono">{item.cve_record.cve_id}</span>
                      ) : null}
                      {item.cve_record.vendor_project ? (
                        <span className="text-muted">
                          {translateUi("厂商")}: {item.cve_record.vendor_project}
                        </span>
                      ) : null}
                      {item.cve_record.product ? (
                        <span className="text-muted">
                          {translateUi("产品")}: {item.cve_record.product}
                        </span>
                      ) : null}
                    </div>
                    <div className="flex flex-wrap gap-x-3 gap-y-1 text-muted">
                      {item.cve_record.date_added ? (
                        <span>
                          {translateUi("CISA收录日期")}: {item.cve_record.date_added}
                        </span>
                      ) : null}
                      {item.cve_record.due_date ? (
                        <span>
                          {translateUi("修复期限")}: {item.cve_record.due_date}
                        </span>
                      ) : null}
                      {item.cve_record.known_ransomware_campaign_use ? (
                        <span>
                          {translateUi("勒索软件使用")}:{" "}
                          {item.cve_record.known_ransomware_campaign_use}
                        </span>
                      ) : null}
                      {item.cve_record.forensic_triage ? (
                        <span>
                          {translateUi("取证分诊")}: {item.cve_record.forensic_triage}
                        </span>
                      ) : null}
                      {item.cve_record.cwes?.length ? (
                        <span>CWE: {item.cve_record.cwes.join(", ")}</span>
                      ) : null}
                    </div>
                    <p className="text-muted">
                      {translateUi("CVSS/EPSS 未由 KEV 提供")} · {translateUi("PoC 未评估")} ·{" "}
                      {translateUi("当前目标未测试")}
                    </p>
                    {item.cve_record.required_action ? (
                      <p className="text-muted">
                        <span className="font-medium">{translateUi("所需操作")}:</span>{" "}
                        {item.cve_record.required_action}
                      </p>
                    ) : null}
                  </div>
                ) : null}
                {item.pocindex_record ? (
                  <div
                    className="mt-2 space-y-1.5 text-[11px]"
                    data-testid="pocindex-record-metadata"
                  >
                    <div className="flex flex-wrap items-center gap-1.5">
                      <span className="badge badge-sm badge-info">
                        {translateUi("PoC Index 风险元数据")}
                      </span>
                      <span className="text-muted">
                        {translateUi("PoC Index KEV 索引")}:{" "}
                        {item.pocindex_record.pocindex_kev_present
                          ? translateUi("已收录")
                          : translateUi("未收录")}
                      </span>
                      {typeof item.pocindex_record.epss_score === "number" ? (
                        <span className="font-medium">
                          {translateUi("EPSS 分数")}: {item.pocindex_record.epss_score.toFixed(5)}
                        </span>
                      ) : null}
                      {typeof item.pocindex_record.epss_percentile === "number" ? (
                        <span className="text-muted">
                          {translateUi("EPSS 百分位")}:{" "}
                          {(item.pocindex_record.epss_percentile * 100).toFixed(1)}%
                        </span>
                      ) : null}
                    </div>
                    {item.pocindex_record.cvss_assessments.length ? (
                      <div className="flex flex-wrap gap-x-3 gap-y-1 text-muted">
                        <span className="font-medium">{translateUi("CVSS 评估")}:</span>
                        {item.pocindex_record.cvss_assessments
                          .slice(0, 4)
                          .map((assessment, index) => (
                            <span key={`${assessment.version}-${assessment.source}-${index}`}>
                              v{assessment.version} {assessment.score.toFixed(1)}{" "}
                              {assessment.severity} · {assessment.source} ·{" "}
                              {assessment.assessment_type}
                            </span>
                          ))}
                      </div>
                    ) : null}
                    {item.pocindex_record.nuclei_metadata ? (
                      <div className="flex flex-wrap gap-x-3 gap-y-1 text-muted">
                        <span className="font-medium">{translateUi("Nuclei 元数据")}:</span>
                        <span>{item.pocindex_record.nuclei_metadata.severity ?? "unknown"}</span>
                        {typeof item.pocindex_record.nuclei_metadata.cvss === "number" ? (
                          <span>CVSS {item.pocindex_record.nuclei_metadata.cvss.toFixed(1)}</span>
                        ) : null}
                        {item.pocindex_record.nuclei_metadata.cwe ? (
                          <span>{item.pocindex_record.nuclei_metadata.cwe}</span>
                        ) : null}
                      </div>
                    ) : null}
                    {item.pocindex_record.advisories.length ? (
                      <div className="flex flex-wrap gap-x-2 gap-y-1 text-muted">
                        <span className="font-medium">
                          {translateUi("公告链接")} ({item.pocindex_record.advisories.length}):
                        </span>
                        {item.pocindex_record.advisories.slice(0, 3).map((advisory, index) => (
                          <a
                            key={`${advisory.url}-${index}`}
                            href={advisory.url}
                            target="_blank"
                            rel="noreferrer"
                            className="text-primary underline-offset-2 hover:underline"
                          >
                            {advisory.tags[0] ?? new URL(advisory.url).hostname}
                          </a>
                        ))}
                      </div>
                    ) : null}
                    {item.pocindex_record.snapshot ? (
                      <p className="text-muted">
                        {translateUi("快照时间")}:{" "}
                        {item.pocindex_record.snapshot.retrieved_at ?? "—"}
                        {item.pocindex_record.snapshot.repository_commit
                          ? ` · ${item.pocindex_record.snapshot.repository_commit.slice(0, 7)}`
                          : ""}
                      </p>
                    ) : null}
                  </div>
                ) : null}
                {item.nuclei_template ? (
                  <div
                    className="mt-2 flex flex-wrap items-center gap-1.5 text-[11px]"
                    data-testid="nuclei-template-metadata"
                  >
                    {item.nuclei_template.severity ? (
                      <span className="badge badge-sm badge-warning">
                        {item.nuclei_template.severity}
                      </span>
                    ) : null}
                    {item.nuclei_template.parse_status ? (
                      <span className="badge badge-sm badge-ghost">
                        {item.nuclei_template.parse_status}
                      </span>
                    ) : null}
                    {item.nuclei_template.protocols?.length ? (
                      <span className="text-muted">
                        {translateUi("协议")}: {item.nuclei_template.protocols.join(", ")}
                      </span>
                    ) : null}
                    {stringList(item.nuclei_template.classification?.["cve-id"]).length ? (
                      <span className="text-muted">
                        CVE:{" "}
                        {stringList(item.nuclei_template.classification?.["cve-id"]).join(", ")}
                      </span>
                    ) : null}
                    {stringList(item.nuclei_template.classification?.["cwe-id"]).length ? (
                      <span className="text-muted">
                        CWE:{" "}
                        {stringList(item.nuclei_template.classification?.["cwe-id"]).join(", ")}
                      </span>
                    ) : null}
                    {item.nuclei_template.classification?.["cvss-score"] !== undefined ? (
                      <span className="text-muted">
                        Nuclei CVSS: {String(item.nuclei_template.classification["cvss-score"])}
                      </span>
                    ) : null}
                    {item.nuclei_template.classification?.["epss-score"] !== undefined ? (
                      <span className="text-muted">
                        Nuclei EPSS: {String(item.nuclei_template.classification["epss-score"])}
                      </span>
                    ) : null}
                    {item.nuclei_template.classification?.["epss-percentile"] !== undefined ? (
                      <span className="text-muted">
                        Nuclei EPSS percentile:{" "}
                        {String(item.nuclei_template.classification["epss-percentile"])}
                      </span>
                    ) : null}
                    {item.nuclei_template.tags?.length ? (
                      <span className="text-muted">
                        {translateUi("标签")}: {item.nuclei_template.tags.slice(0, 10).join(", ")}
                      </span>
                    ) : null}
                    {item.nuclei_template.author?.length ? (
                      <span className="text-muted">
                        {translateUi("作者")}: {item.nuclei_template.author.join(", ")}
                      </span>
                    ) : null}
                    {item.nuclei_template.request_block_count !== undefined ? (
                      <span className="text-muted">
                        {translateUi("请求块")}: {item.nuclei_template.request_block_count}
                      </span>
                    ) : null}
                    {item.nuclei_template.matcher_count !== undefined ? (
                      <span className="text-muted">
                        {translateUi("匹配器")}: {item.nuclei_template.matcher_count}
                      </span>
                    ) : null}
                    {item.nuclei_template.extractor_count !== undefined ? (
                      <span className="text-muted">
                        {translateUi("提取器")}: {item.nuclei_template.extractor_count}
                      </span>
                    ) : null}
                    {item.nuclei_template.has_flow ? (
                      <span className="badge badge-sm badge-ghost">Flow</span>
                    ) : null}
                  </div>
                ) : null}
                {item.category_path.length > 0 ? (
                  <p className="mt-1 text-[11px] text-muted" data-testid="source-category">
                    {translateUi("分类：")}
                    {item.category_path.join(" / ")}
                  </p>
                ) : null}
                <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-muted">
                  {item.canonical_url ? (
                    <a
                      href={item.canonical_url}
                      target="_blank"
                      rel="noreferrer"
                      className="max-w-md truncate text-primary underline underline-offset-2"
                    >
                      {item.canonical_url}
                    </a>
                  ) : null}
                  <span>
                    {translateUi("更新")}{" "}
                    {item.last_seen_at ? formatDateTime(item.last_seen_at) : "-"}
                  </span>
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
      {reviewHistory.length > 0 ? (
        <section
          className="mt-4 rounded-surface border border-edge bg-base-100 p-3"
          aria-label={translateUi("最近审核历史")}
          data-testid="knowledge-source-review-history"
        >
          <h3 className="text-xs font-semibold">{translateUi("最近审核历史")}</h3>
          <ul className="mt-2 space-y-2">
            {reviewHistory.map((event) => (
              <li key={event.event_id} className="flex flex-wrap items-center gap-2 text-[11px]">
                <span className="font-mono text-muted">{formatDateTime(event.ts)}</span>
                <span className="badge badge-sm badge-outline">{event.status ?? "-"}</span>
                <span>
                  {event.updated} {translateUi("条")}
                </span>
                <span className="text-muted">{event.actor}</span>
                <span
                  className="min-w-0 flex-1 truncate text-muted"
                  title={event.item_keys.join(", ")}
                >
                  {event.item_keys.slice(0, 3).join(", ")}
                  {event.item_keys.length > 3 ? ` +${event.item_keys.length - 3}` : ""}
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </section>
  );
}

interface CuratedPocHit {
  resource_id: string;
  name: string;
  resource_type: string;
  evidence_level?: string | null;
  knowledge_lifecycle?: string | null;
  poc_category?: string | null;
  poc_severity?: string | null;
  poc_cve_id?: string | null;
  source_path?: string | null;
  source_commit?: string | null;
  related_poc_ids?: string[];
  source_annotations?: string[];
  poc_source_integrity?: string | null;
  poc_verification_result?: string | null;
  poc_dedup_status?: string | null;
  poc_independent_knowledge_status?: string | null;
  poc_missing_evidence?: string[];
  poc_evidence_refs?: string[];
  poc_source_urls?: string[];
  poc_review_gate?: string | null;
  related_entities?: string[];
}

interface CuratedPocSummary {
  available: boolean;
  total_curated: number;
  by_evidence_level: Record<string, number>;
  by_lifecycle: Record<string, number>;
  by_source_integrity: Record<string, number>;
  by_verification_result: Record<string, number>;
  missing_evidence_entries: number;
  missing_evidence_items: number;
  entries_with_source_path: number;
  entries_without_source_path: number;
  entries_with_source_commit: number;
  entries_without_source_commit: number;
  entries_with_source_urls: number;
  entries_without_source_urls: number;
  relation_count: number;
  external_entity_count: number;
  review_queue_count: number;
  metadata_incomplete_entries: number;
}

interface CuratedPocSearchResult {
  available: boolean;
  query: string;
  total: number;
  returned: number;
  hits: CuratedPocHit[];
  filters: Record<string, string>;
}

function CuratedPocSearchView() {
  const [rawQuery, setRawQuery] = useState("");
  const [keyword, setKeyword] = useState("");
  const [evidence, setEvidence] = useState("");
  const [lifecycle, setLifecycle] = useState("");
  const [severity, setSeverity] = useState("");
  const [category, setCategory] = useState("");
  const [cve, setCve] = useState("");
  const [sourceIntegrity, setSourceIntegrity] = useState("");
  const [verificationResult, setVerificationResult] = useState("");
  const [dedupStatus, setDedupStatus] = useState("");
  const [missingEvidence, setMissingEvidence] = useState("true");
  const summary = useApi<CuratedPocSummary>("/api/curated-pocs/summary");

  useEffect(() => {
    const timer = setTimeout(() => setKeyword(rawQuery.trim()), 300);
    return () => clearTimeout(timer);
  }, [rawQuery]);

  const params = new URLSearchParams({ limit: "100" });
  if (keyword) params.set("q", keyword);
  if (evidence) params.set("evidence_level", evidence);
  if (lifecycle) params.set("knowledge_lifecycle", lifecycle);
  if (severity) params.set("poc_severity", severity);
  if (category.trim()) params.set("poc_category", category.trim());
  if (cve.trim()) params.set("poc_cve_id", cve.trim());
  if (sourceIntegrity) params.set("poc_source_integrity", sourceIntegrity);
  if (verificationResult) params.set("poc_verification_result", verificationResult);
  if (dedupStatus) params.set("poc_dedup_status", dedupStatus);
  if (missingEvidence) params.set("poc_missing_evidence", missingEvidence);
  const hasCriteria = Boolean(
    keyword ||
    evidence ||
    lifecycle ||
    severity ||
    category.trim() ||
    cve.trim() ||
    sourceIntegrity ||
    verificationResult ||
    dedupStatus ||
    missingEvidence,
  );
  const result = useApi<CuratedPocSearchResult>(
    hasCriteria ? `/api/curated-pocs/search?${params.toString()}` : null,
  );
  const hits = result.data?.hits ?? [];

  return (
    <section aria-label={translateUi("自有 POC 证据检索")} data-testid="curated-poc-search">
      <div className="mb-3 rounded-surface border border-edge bg-base-100 p-3">
        <div
          className="mb-3 grid grid-cols-2 gap-2 md:grid-cols-3 xl:grid-cols-6"
          data-testid="poc-governance-summary"
        >
          <div className="rounded-control bg-base-200 p-2">
            <div className="text-[11px] text-muted">{translateUi("已纳入 POC")}</div>
            <div className="text-lg font-semibold tabular-nums">
              {summary.data?.total_curated ?? "—"}
            </div>
          </div>
          <div className="rounded-control bg-base-200 p-2">
            <div className="text-[11px] text-muted">{translateUi("待补证队列")}</div>
            <div className="text-lg font-semibold tabular-nums">
              {summary.data?.review_queue_count ?? "—"}
            </div>
          </div>
          <div className="rounded-control bg-base-200 p-2">
            <div className="text-[11px] text-muted">{translateUi("缺失证据项")}</div>
            <div className="text-lg font-semibold tabular-nums">
              {summary.data?.missing_evidence_items ?? "—"}
            </div>
          </div>
          <div className="rounded-control bg-base-200 p-2">
            <div className="text-[11px] text-muted">{translateUi("关联关系")}</div>
            <div className="text-lg font-semibold tabular-nums">
              {summary.data?.relation_count ?? "—"}
            </div>
          </div>
          <div className="rounded-control bg-base-200 p-2">
            <div className="text-[11px] text-muted">{translateUi("治理元数据待补")}</div>
            <div className="text-lg font-semibold tabular-nums">
              {summary.data?.metadata_incomplete_entries ?? "—"}
            </div>
          </div>
          <div className="rounded-control bg-base-200 p-2">
            <div className="text-[11px] text-muted">{translateUi("缺少来源 Commit")}</div>
            <div className="text-lg font-semibold tabular-nums">
              {summary.data?.entries_without_source_commit ?? "—"}
            </div>
          </div>
        </div>
        <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
          <input
            type="search"
            className="input input-sm w-full rounded-control border border-edge bg-base-100"
            placeholder={translateUi("搜索 POC 标题、技术或关键词")}
            aria-label={translateUi("搜索自有 POC")}
            value={rawQuery}
            onChange={(event) => setRawQuery(event.target.value)}
          />
          <select
            aria-label={translateUi("证据等级筛选")}
            className="select select-sm rounded-control border border-edge bg-base-100"
            value={evidence}
            onChange={(event) => setEvidence(event.target.value)}
          >
            <option value="">{translateUi("全部证据等级")}</option>
            {(["K0", "K1", "K2", "K3", "K4"] as const).map((level) => (
              <option key={level} value={level}>
                {level}
              </option>
            ))}
          </select>
          <select
            aria-label={translateUi("POC 生命周期筛选")}
            className="select select-sm rounded-control border border-edge bg-base-100"
            value={lifecycle}
            onChange={(event) => setLifecycle(event.target.value)}
          >
            <option value="">{translateUi("全部生命周期")}</option>
            <option value="Candidate">Candidate</option>
            <option value="Researched">Researched</option>
            <option value="Lab-validated">Lab-validated</option>
          </select>
          <select
            aria-label={translateUi("POC 严重程度筛选")}
            className="select select-sm rounded-control border border-edge bg-base-100"
            value={severity}
            onChange={(event) => setSeverity(event.target.value)}
          >
            <option value="">{translateUi("全部严重程度")}</option>
            {(["critical", "high", "medium", "low", "info"] as const).map((level) => (
              <option key={level} value={level}>
                {level.toUpperCase()}
              </option>
            ))}
          </select>
          <input
            className="input input-sm rounded-control border border-edge bg-base-100"
            placeholder={translateUi("漏洞分类，例如 auth-bypass")}
            aria-label={translateUi("POC 分类筛选")}
            value={category}
            onChange={(event) => setCategory(event.target.value)}
          />
          <select
            aria-label={translateUi("来源完整性筛选")}
            className="select select-sm rounded-control border border-edge bg-base-100"
            value={sourceIntegrity}
            onChange={(event) => setSourceIntegrity(event.target.value)}
          >
            <option value="">{translateUi("全部来源完整性")}</option>
            <option value="complete">complete</option>
            <option value="partial">partial</option>
            <option value="missing">missing</option>
          </select>
          <select
            aria-label={translateUi("证据核验结论筛选")}
            className="select select-sm rounded-control border border-edge bg-base-100"
            value={verificationResult}
            onChange={(event) => setVerificationResult(event.target.value)}
          >
            <option value="">{translateUi("全部核验结论")}</option>
            <option value="candidate_not_verified">candidate_not_verified</option>
            <option value="historical_evidence_supported">historical_evidence_supported</option>
          </select>
          <select
            aria-label={translateUi("去重状态筛选")}
            className="select select-sm rounded-control border border-edge bg-base-100"
            value={dedupStatus}
            onChange={(event) => setDedupStatus(event.target.value)}
          >
            <option value="">{translateUi("全部去重状态")}</option>
            <option value="independent">independent</option>
            <option value="related-family">related-family</option>
          </select>
          <select
            aria-label={translateUi("缺失证据筛选")}
            className="select select-sm rounded-control border border-edge bg-base-100"
            value={missingEvidence}
            onChange={(event) => setMissingEvidence(event.target.value)}
          >
            <option value="">{translateUi("全部证据状态")}</option>
            <option value="true">{translateUi("仅待补证")}</option>
            <option value="false">{translateUi("证据项已齐")}</option>
          </select>
          <input
            className="input input-sm rounded-control border border-edge bg-base-100"
            placeholder="CVE-YYYY-NNNN"
            aria-label={translateUi("CVE 编号筛选")}
            value={cve}
            onChange={(event) => setCve(event.target.value)}
          />
        </div>
        <div className="mt-2 flex flex-wrap items-center justify-between gap-2 text-xs text-muted">
          <span>
            {translateUi(
              "仅展示 Overlay 明确纳入的 POC 元数据；不会读取 POC 正文。默认显示待补证队列。",
            )}
          </span>
          <button
            type="button"
            className="btn btn-ghost btn-xs"
            onClick={() => {
              setRawQuery("");
              setKeyword("");
              setEvidence("");
              setLifecycle("");
              setSeverity("");
              setCategory("");
              setCve("");
              setSourceIntegrity("");
              setVerificationResult("");
              setDedupStatus("");
              setMissingEvidence("true");
            }}
          >
            {translateUi("清除筛选")}
          </button>
        </div>
      </div>

      {!hasCriteria ? (
        <EmptyPanel
          text={translateUi("输入关键词或选择至少一个筛选条件以检索已审核纳入的自有 POC。")}
        />
      ) : result.loading ? (
        <SkeletonPanel lines={5} />
      ) : result.error !== null ? (
        <ErrorPanel error={result.error} onRetry={result.retry} />
      ) : hits.length === 0 ? (
        <EmptyPanel text={translateUi("没有匹配的自有 POC；可调整关键词或筛选条件。")} />
      ) : (
        <>
          <p className="mb-2 text-xs text-muted" data-testid="poc-search-total">
            {translateUi("匹配")} {result.data?.total ?? 0} {translateUi("条自有 POC")}
          </p>
          <ul className="space-y-2" data-testid="curated-poc-results">
            {hits.map((hit) => (
              <li
                key={hit.resource_id}
                className="rounded-surface border border-edge bg-base-100 p-3"
              >
                <div className="flex flex-wrap items-center gap-1">
                  {hit.evidence_level ? (
                    <span
                      className={`badge badge-sm ${hit.evidence_level === "K0" ? "badge-warning" : "badge-info"}`}
                    >
                      {hit.evidence_level}
                    </span>
                  ) : null}
                  {hit.knowledge_lifecycle ? (
                    <span className="badge badge-sm badge-outline">{hit.knowledge_lifecycle}</span>
                  ) : null}
                  {hit.poc_severity ? (
                    <span
                      className={`badge badge-sm ${hit.poc_severity === "critical" ? "badge-error" : hit.poc_severity === "high" ? "badge-warning" : "badge-ghost"}`}
                    >
                      {hit.poc_severity.toUpperCase()}
                    </span>
                  ) : null}
                  {hit.poc_category ? (
                    <span className="badge badge-sm badge-ghost">{hit.poc_category}</span>
                  ) : null}
                  {hit.poc_source_integrity ? (
                    <span className="badge badge-sm badge-outline">
                      {translateUi("来源")}:{hit.poc_source_integrity}
                    </span>
                  ) : null}
                  {hit.poc_dedup_status ? (
                    <span className="badge badge-sm badge-outline">{hit.poc_dedup_status}</span>
                  ) : null}
                  <span className="ml-auto font-mono text-[11px] text-muted">
                    {hit.resource_id}
                  </span>
                </div>
                <h3 className="mt-2 text-sm font-semibold leading-5">{hit.name}</h3>
                <div className="mt-2 grid gap-1 text-[11px] text-muted md:grid-cols-2">
                  {hit.poc_cve_id ? <p>CVE: {hit.poc_cve_id}</p> : null}
                  {hit.source_commit ? (
                    <p>
                      Commit: <span className="font-mono">{hit.source_commit.slice(0, 12)}</span>
                    </p>
                  ) : null}
                  <p className="break-all md:col-span-2">
                    {translateUi("来源路径")}：
                    <span className="font-mono">{hit.source_path || translateUi("未记录")}</span>
                  </p>
                  <p className="break-all md:col-span-2">
                    {translateUi("来源 Commit")}：
                    <span className="font-mono">{hit.source_commit || translateUi("未记录")}</span>
                  </p>
                  {hit.poc_verification_result ? (
                    <p className="md:col-span-2">
                      {translateUi("核验结论")}：{hit.poc_verification_result}
                    </p>
                  ) : null}
                  {hit.poc_independent_knowledge_status ? (
                    <p className="md:col-span-2">
                      {translateUi("独立知识状态")}：{hit.poc_independent_knowledge_status}
                    </p>
                  ) : null}
                  {hit.poc_missing_evidence?.length ? (
                    <p className="break-all md:col-span-2">
                      {translateUi("缺失证据")}：{hit.poc_missing_evidence.join(" · ")}
                    </p>
                  ) : null}
                  {hit.poc_evidence_refs?.length ? (
                    <p className="break-all md:col-span-2">
                      {translateUi("证据引用")}：{hit.poc_evidence_refs.join(" · ")}
                    </p>
                  ) : null}
                  {hit.poc_source_urls?.length ? (
                    <p className="break-all md:col-span-2">
                      {translateUi("上游来源")}：
                      {hit.poc_source_urls.map((url) => (
                        <a
                          key={url}
                          href={url}
                          target="_blank"
                          rel="noreferrer"
                          className="mr-2 underline underline-offset-2"
                        >
                          {url}
                        </a>
                      ))}
                    </p>
                  ) : null}
                  {hit.poc_review_gate ? (
                    <p className="break-words md:col-span-2">
                      {translateUi("下一步审核要求")}：{hit.poc_review_gate}
                    </p>
                  ) : null}
                  {hit.related_poc_ids?.length ? (
                    <p className="break-all md:col-span-2">
                      {translateUi("关联 POC")}：{hit.related_poc_ids.join(" · ")}
                    </p>
                  ) : null}
                  {hit.related_entities?.length ? (
                    <p className="break-all md:col-span-2">
                      {translateUi("关联案例组")}：{hit.related_entities.join(" · ")}
                    </p>
                  ) : null}
                </div>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

interface PocGovernanceItem {
  poc_id: number;
  title: string;
  evidence_level?: string | null;
  lifecycle?: string | null;
  verification_result?: string | null;
  review_gate?: string | null;
  missing_evidence: string[];
  evidence_refs: string[];
  related_relations: Array<{
    direction: "incoming" | "outgoing";
    target: string;
    target_title: string;
    type?: string;
    confidence?: number;
    note?: string;
  }>;
  source_path?: string | null;
  source_commit?: string | null;
  assessment: {
    evidence_completeness: string;
    artifact_validity: string;
    remediation_status: string;
    license_status: string;
    assessed_at?: string;
    reviewer?: string;
    review_status?: string;
    review_reason?: string;
  };
}

interface PocGovernanceList {
  available: boolean;
  summary: { total: number; assessed: number; not_assessed: number; evidence_gaps: number };
  items: PocGovernanceItem[];
}

interface PocRetestAttempt {
  item_id?: string;
  provider?: string;
  provider_version?: string;
  result_status?: string;
  error_category?: string | null;
  run_id?: string | null;
  trace_id?: string | null;
  created_at?: string;
  transitions?: Array<{ from_status?: string; to_status?: string; created_at?: string }>;
  evidence?: Array<{
    evidence_id?: string;
    evidence_type?: string;
    content_hash?: string;
    created_at?: string;
  }>;
}

interface PocGovernanceDetail {
  available: boolean;
  item: PocGovernanceItem;
  retest_history: { rtis_poc_id: number; attempts: PocRetestAttempt[] };
}

function PocGovernanceView() {
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const list = useApi<PocGovernanceList>("/api/curated-pocs/governance");
  const detail = useApi<PocGovernanceDetail>(
    selectedId === null ? null : `/api/curated-pocs/governance?poc_id=${selectedId}`,
  );
  const items = list.data?.items ?? [];
  const filtered = items.filter((item) =>
    `${item.poc_id} ${item.title} ${item.assessment.artifact_validity} ${item.assessment.remediation_status}`
      .toLowerCase()
      .includes(query.trim().toLowerCase()),
  );
  const selected = detail.data?.item;
  const attempts = detail.data?.retest_history.attempts ?? [];

  return (
    <section aria-label={translateUi("POC 治理")} data-testid="poc-governance-view">
      <div
        className="mb-3 grid grid-cols-2 gap-2 md:grid-cols-4"
        data-testid="poc-governance-m3-summary"
      >
        {[
          [translateUi("已纳入 POC"), list.data?.summary.total],
          [translateUi("已评估"), list.data?.summary.assessed],
          [translateUi("尚未评估"), list.data?.summary.not_assessed],
          [translateUi("缺失证据项"), list.data?.summary.evidence_gaps],
        ].map(([label, value]) => (
          <div key={String(label)} className="rounded-surface border border-edge bg-base-100 p-3">
            <p className="text-xs text-muted">{label}</p>
            <p className="mt-1 text-xl font-semibold tabular-nums">{value ?? "—"}</p>
          </div>
        ))}
      </div>
      <div className="mb-3">
        <input
          className="input input-sm w-full max-w-md rounded-control border border-edge bg-base-100"
          aria-label={translateUi("检索 POC 治理记录")}
          placeholder={translateUi("按 POC ID、标题或评估状态检索")}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
      </div>
      {list.loading ? (
        <SkeletonPanel lines={5} />
      ) : list.error !== null ? (
        <ErrorPanel error={list.error} onRetry={list.retry} />
      ) : (
        <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_420px]">
          <ul className="space-y-2" data-testid="poc-governance-list">
            {filtered.map((item) => (
              <li key={item.poc_id}>
                <button
                  type="button"
                  onClick={() => setSelectedId(item.poc_id)}
                  aria-pressed={selectedId === item.poc_id}
                  className={`w-full rounded-surface border p-3 text-left ${selectedId === item.poc_id ? "border-primary bg-primary/5" : "border-edge bg-base-100 hover:border-base-content/30"}`}
                >
                  <div className="flex flex-wrap items-center gap-1">
                    <span className="badge badge-sm badge-outline">POC {item.poc_id}</span>
                    <span className="badge badge-sm badge-warning">
                      {item.assessment.artifact_validity}
                    </span>
                    <span className="badge badge-sm badge-outline">
                      {item.assessment.evidence_completeness}
                    </span>
                    <span className="badge badge-sm badge-ghost">
                      {item.assessment.remediation_status}
                    </span>
                  </div>
                  <p className="mt-2 text-sm font-medium">{item.title}</p>
                  <p className="mt-1 text-[11px] text-muted">
                    {item.evidence_level ?? "—"} · {item.lifecycle ?? "—"} ·{" "}
                    {translateUi("缺失证据")}: {item.missing_evidence.length}
                  </p>
                </button>
              </li>
            ))}
            {filtered.length === 0 ? (
              <li>
                <EmptyPanel text={translateUi("没有匹配的 POC 治理记录。")} />
              </li>
            ) : null}
          </ul>
          <aside
            className="min-w-0 rounded-surface border border-edge bg-base-100 p-3"
            data-testid="poc-governance-detail"
          >
            {selectedId === null ? (
              <EmptyPanel text={translateUi("选择一条 POC 查看评估与复测历史。")} />
            ) : detail.loading ? (
              <SkeletonPanel lines={5} />
            ) : detail.error !== null ? (
              <ErrorPanel error={detail.error} onRetry={detail.retry} />
            ) : selected ? (
              <div className="space-y-3">
                <div>
                  <p className="text-[11px] text-muted">POC {selected.poc_id}</p>
                  <h3 className="mt-1 text-sm font-semibold">{selected.title}</h3>
                </div>
                <section>
                  <h4 className="text-xs font-semibold text-muted">{translateUi("评估状态")}</h4>
                  <dl className="mt-2 grid grid-cols-2 gap-2 text-xs">
                    <div>
                      <dt className="text-muted">{translateUi("证据完整性")}</dt>
                      <dd>{selected.assessment.evidence_completeness}</dd>
                    </div>
                    <div>
                      <dt className="text-muted">{translateUi("工件有效性")}</dt>
                      <dd>{selected.assessment.artifact_validity}</dd>
                    </div>
                    <div>
                      <dt className="text-muted">{translateUi("修复状态")}</dt>
                      <dd>{selected.assessment.remediation_status}</dd>
                    </div>
                    <div>
                      <dt className="text-muted">{translateUi("许可证")}</dt>
                      <dd>{selected.assessment.license_status}</dd>
                    </div>
                    <div>
                      <dt className="text-muted">{translateUi("审核状态")}</dt>
                      <dd>{selected.assessment.review_status ?? "candidate"}</dd>
                    </div>
                    <div>
                      <dt className="text-muted">{translateUi("评估时间")}</dt>
                      <dd>{selected.assessment.assessed_at ?? "—"}</dd>
                    </div>
                  </dl>
                </section>
                <section>
                  <h4 className="text-xs font-semibold text-muted">{translateUi("证据与来源")}</h4>
                  <p className="mt-1 text-xs">
                    {translateUi("缺失证据")}: {selected.missing_evidence.join(" · ") || "—"}
                  </p>
                  <p className="mt-1 break-all text-[11px] text-muted">
                    {selected.source_path ?? "—"}
                  </p>
                  <p className="mt-1 break-all font-mono text-[10px] text-muted">
                    {selected.source_commit ?? "—"}
                  </p>
                  <p className="mt-2 text-xs">
                    {translateUi("来源证据引用")}: {selected.evidence_refs.join(" · ") || "—"}
                  </p>
                  {selected.review_gate ? (
                    <p className="mt-2 text-xs">
                      {translateUi("下一步审核要求")}: {selected.review_gate}
                    </p>
                  ) : null}
                </section>
                <section>
                  <h4 className="text-xs font-semibold text-muted">
                    {translateUi("关联关系")} ({selected.related_relations.length})
                  </h4>
                  {selected.related_relations.length === 0 ? (
                    <p className="mt-1 text-xs text-muted">{translateUi("暂无关联关系")}</p>
                  ) : (
                    <ul className="mt-2 space-y-2">
                      {selected.related_relations.map((relation, index) => (
                        <li
                          key={`${relation.direction}-${relation.target}-${index}`}
                          className="rounded-control bg-base-200 p-2 text-xs"
                        >
                          <div className="flex flex-wrap items-center gap-1">
                            <span className="badge badge-xs badge-outline">
                              {relation.direction === "outgoing"
                                ? translateUi("指向")
                                : translateUi("被指向")}
                            </span>
                            <span className="badge badge-xs badge-info">
                              {relation.type ?? "related"}
                            </span>
                            <span className="text-muted">
                              {translateUi("置信度")}: {relation.confidence ?? "—"}
                            </span>
                          </div>
                          <p className="mt-1 break-all font-medium">{relation.target_title}</p>
                          <p className="mt-1 break-all text-[10px] text-muted">{relation.target}</p>
                          {relation.note ? (
                            <p className="mt-1 text-muted">{relation.note}</p>
                          ) : null}
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
                <section>
                  <h4 className="text-xs font-semibold text-muted">
                    {translateUi("复测历史")} ({attempts.length})
                  </h4>
                  {attempts.length === 0 ? (
                    <p className="mt-1 text-xs text-muted">
                      {translateUi("暂无复测记录；未执行不代表复测失败。")}
                    </p>
                  ) : (
                    <ul className="mt-2 space-y-2">
                      {attempts.map((attempt, index) => (
                        <li
                          key={attempt.item_id ?? index}
                          className="rounded-control bg-base-200 p-2 text-xs"
                        >
                          <div className="flex flex-wrap justify-between gap-1">
                            <span className="font-medium">
                              {attempt.result_status ?? "UNKNOWN"}
                            </span>
                            <span className="text-muted">{attempt.created_at ?? "—"}</span>
                          </div>
                          <p className="mt-1 text-muted">
                            {attempt.provider ?? "—"} {attempt.provider_version ?? ""} ·{" "}
                            {attempt.error_category ?? "—"}
                          </p>
                          {attempt.transitions?.length ? (
                            <p className="mt-1 break-words text-[11px]">
                              {attempt.transitions
                                .map(
                                  (transition) =>
                                    `${transition.from_status ?? "—"} → ${transition.to_status ?? "—"}`,
                                )
                                .join(" · ")}
                            </p>
                          ) : null}
                          {attempt.evidence?.length ? (
                            <ul className="mt-1 space-y-1">
                              {attempt.evidence.map((evidence) => (
                                <li
                                  key={evidence.evidence_id ?? evidence.content_hash}
                                  className="break-all font-mono text-[10px] text-muted"
                                >
                                  {evidence.evidence_type ?? "evidence"} · SHA-256{" "}
                                  {evidence.content_hash ?? "—"}
                                </li>
                              ))}
                            </ul>
                          ) : null}
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
              </div>
            ) : (
              <EmptyPanel text={translateUi("POC 治理详情不可用。")} />
            )}
          </aside>
        </div>
      )}
    </section>
  );
}

export function KnowledgePage() {
  const [view, setView] = useState<"platform" | "sources" | "pocs" | "governance">("platform");
  return (
    <div>
      <PageHeader
        title={translateUi("知识库")}
        actions={
          <div className="flex items-center gap-2">
            <div className="flex items-center gap-1" data-testid="knowledge-view-switch">
              <button
                type="button"
                className={`btn btn-xs ${view === "platform" ? "btn-primary" : "btn-ghost"}`}
                onClick={() => setView("platform")}
              >
                {translateUi("平台知识")}
              </button>
              <button
                type="button"
                className={`btn btn-xs ${view === "sources" ? "btn-primary" : "btn-ghost"}`}
                onClick={() => setView("sources")}
              >
                {translateUi("知识来源")}
              </button>
              <button
                type="button"
                className={`btn btn-xs ${view === "pocs" ? "btn-primary" : "btn-ghost"}`}
                onClick={() => setView("pocs")}
                data-testid="knowledge-poc-tab"
              >
                {translateUi("自有 POC")}
              </button>
              <button
                type="button"
                className={`btn btn-xs ${view === "governance" ? "btn-primary" : "btn-ghost"}`}
                onClick={() => setView("governance")}
                data-testid="knowledge-poc-governance-tab"
              >
                {translateUi("POC 治理")}
              </button>
            </div>
          </div>
        }
      />
      {view === "sources" ? (
        <SourceEntriesView />
      ) : view === "pocs" ? (
        <CuratedPocSearchView />
      ) : view === "governance" ? (
        <PocGovernanceView />
      ) : (
        <PlatformKnowledgeView />
      )}
    </div>
  );
}

function PlatformKnowledgeView() {
  const location = useLocation();
  const navigate = useNavigate();
  const [rawQuery, setRawQuery] = useState("");
  const [keyword, setKeyword] = useState("");
  // 深链契约：?knowledge_id= 初始化选中态（⌘K 面板与外部链接的落点），
  // 选中变化同步回 URL（replace，不产生历史噪音），对齐 ProjectsPage 的
  // ?task_id= 模式。
  const [selectedId, setSelectedId] = useState<string | null>(() =>
    new URLSearchParams(location.search).get("knowledge_id"),
  );

  const selectKnowledge = useCallback(
    (id: string | null) => {
      setSelectedId(id);
      const params = new URLSearchParams(location.search);
      if (id) params.set("knowledge_id", id);
      else params.delete("knowledge_id");
      navigate(
        { pathname: location.pathname, search: params.toString() ? `?${params.toString()}` : "" },
        { replace: true },
      );
    },
    [location.search, location.pathname, navigate],
  );

  // 检索防抖 300ms：输入停稳后才改路径触发 useApi 重取。
  useEffect(() => {
    const timer = setTimeout(() => setKeyword(rawQuery.trim()), 300);
    return () => clearTimeout(timer);
  }, [rawQuery]);

  const listPath = `/api/knowledge?limit=50${keyword ? `&q=${encodeURIComponent(keyword)}` : ""}`;
  const list = useApi<KnowledgeList>(listPath);
  const detail = useApi<KnowledgeDetail>(
    selectedId ? `/api/knowledge?knowledge_id=${encodeURIComponent(selectedId)}` : null,
  );

  const items = list.data?.items ?? [];
  const total = list.data?.total ?? 0;

  return (
    <div>
      <div className="mb-3 flex items-center justify-end">
        <input
          type="text"
          className="input input-sm w-56 rounded-control border border-edge bg-base-100"
          placeholder={translateUi("检索标题 / 组件 / CVE…")}
          aria-label={translateUi("检索知识条目")}
          value={rawQuery}
          onChange={(event) => setRawQuery(event.target.value)}
        />
      </div>
      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_380px]">
        <section aria-label={translateUi("知识条目列表")}>
          {list.loading ? (
            <SkeletonPanel lines={6} />
          ) : list.error !== null ? (
            <ErrorPanel error={list.error} onRetry={list.retry} />
          ) : items.length === 0 ? (
            <EmptyPanel
              text={
                list.data?.message ??
                (keyword
                  ? `${translateUi("没有匹配")}「${keyword}」${translateUi("的知识条目。")}`
                  : translateUi("暂无知识条目。"))
              }
            />
          ) : (
            <>
              <p className="mb-2 text-xs text-muted">
                {keyword ? `${translateUi("检索")}「${keyword}」：` : ""}
                {translateUi("共")} {total} {translateUi("条（显示")} {items.length}）
              </p>
              <ul className="space-y-2">
                {items.map((item) => {
                  const active = item.knowledge_id === selectedId;
                  return (
                    <li key={item.knowledge_id}>
                      <button
                        type="button"
                        onClick={() => selectKnowledge(item.knowledge_id)}
                        aria-current={active ? "true" : undefined}
                        className={`w-full rounded-surface border p-3 text-left transition-colors ${
                          active
                            ? "border-primary bg-primary/5"
                            : "border-edge bg-base-100 hover:border-base-content/30"
                        }`}
                      >
                        <div className="flex flex-wrap items-center gap-1">
                          {item.lifecycle ? (
                            <span
                              className={`badge badge-sm ${LIFECYCLE_BADGE[item.lifecycle] ?? "badge-ghost"}`}
                            >
                              {item.lifecycle}
                            </span>
                          ) : null}
                          <span className="badge badge-sm badge-outline">
                            {verifiedLabel(item.verified)}
                          </span>
                          {item.component ? (
                            <span className="text-[11px] text-muted">{item.component}</span>
                          ) : null}
                          {item.severity ? (
                            <span className="text-[11px] text-muted">{item.severity}</span>
                          ) : null}
                        </div>
                        <p className="mt-1.5 text-sm font-medium leading-5">{item.title}</p>
                        <p className="mt-1 text-[11px] text-muted">
                          {item.updated_at ? formatDateTime(item.updated_at) : "-"}
                          {item.hit_count ? ` · ${translateUi("命中")} ${item.hit_count}` : ""}
                        </p>
                      </button>
                    </li>
                  );
                })}
              </ul>
            </>
          )}
        </section>
        <aside
          aria-label={translateUi("条目详情")}
          className="rounded-surface border border-edge bg-base-100 p-4"
        >
          <DetailSidebar detail={detail} selectedId={selectedId} />
        </aside>
      </div>
    </div>
  );
}
