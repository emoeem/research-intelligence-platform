import { translateUi } from "../../i18n";
/** Research Knowledge Context（P5-1 首版，Phase 5 §4.5.E）：本页是 RIP 相比
 *  普通扫描器最重要的界面之一。本版提供知识检索入口（/api/knowledge?q=
 *  既有服务端关键词过滤，五字段 LIKE），并如实标注：
 *  - 现阶段是知识条目检索（M 面知识库），不是 Phase 5 Knowledge Matcher 的
 *    「命中/排除原因」解释面（P5-3 交付：统一检索 + Context Broker +
 *    适用性判断 + 排除原因）；
 *  - RTIS POC 引用面随 P5-5 RTIS POC Adapter 批次接入（不经 3.0 库复制）。
 *  无假数据；三态统一。 */

import { useState } from "react";
import { useApi } from "../../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../../components/ui";
import { FilterSearch } from "../../components/data";
import { formatDateTime } from "../../lib/format";

interface KnowledgeRow {
  knowledge_id: string;
  record_kind?: string | null;
  lifecycle?: string | null;
  governance?: string | null;
  title?: string | null;
  content_kind?: string | null;
  category?: string | null;
  cve?: string | null;
  component?: string | null;
  versions?: string | null;
  severity?: string | null;
  language?: string | null;
  evidence_level?: string | null;
  verified?: number | boolean | null;
  hit_count?: number | null;
  tags?: string[] | null;
  source?: string | null;
  source_url?: string | null;
  updated_at?: string | null;
}

interface KnowledgeList {
  available?: boolean;
  source?: string;
  total?: number;
  returned?: number;
  limit?: number;
  filters?: Record<string, string | null>;
  message?: string;
  items?: KnowledgeRow[];
}

export function ResearchKnowledgePage() {
  const [query, setQuery] = useState("");
  const [submitted, setSubmitted] = useState("");
  // submitted 为空时 path=null 不发请求（进入页面先空态，避免全量扫库）。
  const list = useApi<KnowledgeList>(
    submitted ? `/api/knowledge?q=${encodeURIComponent(submitted)}&limit=50` : null,
  );

  const items = list.data?.items ?? [];
  const total = list.data?.total;

  return (
    <div>
      <PageHeader
        title={translateUi("知识上下文")}
        actions={
          <span className="badge badge-ghost">
            {translateUi("数据源：/api/knowledge（服务端过滤）")}
          </span>
        }
      />

      <div className="mb-2 flex flex-wrap items-center gap-3">
        <FilterSearch
          label={translateUi("检索")}
          value={query}
          placeholder={translateUi("组件 / CVE / 类别 / 标题关键词")}
          onChange={setQuery}
        />
        <button
          type="button"
          className="btn btn-sm btn-primary"
          onClick={() => {
            setSubmitted(query.trim());
          }}
        >
          {translateUi("检索知识")}
        </button>
      </div>

      <div className="mb-4 rounded-surface border border-edge bg-base-200/40 p-3 text-xs text-muted">
        <p>
          {translateUi(
            "当前能力边界：本页是知识条目检索（M 面知识库 LIKE 过滤）。Phase 5 知识驱动验证的匹配解释面（命中理由、适用条件、排除原因、Token 预算、缓存命中）由 Knowledge Matcher（P5-3）经统一检索与 Context Broker 提供，届时本页升级为命中/排除双栏视图。",
          )}
        </p>
      </div>

      {!submitted ? (
        <EmptyPanel
          text={translateUi(
            "输入关键词后检索（服务端过滤 title/ID/component/cve/category 五字段）。",
          )}
        />
      ) : list.loading ? (
        <SkeletonPanel lines={5} />
      ) : list.error !== null ? (
        <ErrorPanel error={list.error} onRetry={list.retry} />
      ) : items.length === 0 ? (
        <EmptyPanel text={list.data?.message ?? translateUi("没有匹配的知识条目。")} />
      ) : (
        <>
          <p className="mb-2 text-xs text-muted">
            {translateUi("查询")}「{submitted}」：{translateUi("命中")} {total ?? items.length}{" "}
            {translateUi("条（显示")} {items.length}）
          </p>
          <ul className="space-y-2">
            {items.map((item) => (
              <li
                key={item.knowledge_id}
                className="rounded-surface border border-edge bg-base-100 p-3"
              >
                <div className="flex flex-wrap items-center gap-1.5">
                  {item.lifecycle ? (
                    <span className="badge badge-sm badge-info">{item.lifecycle}</span>
                  ) : null}
                  {item.severity ? (
                    <span
                      className={`badge badge-sm ${
                        item.severity === "critical" || item.severity === "high"
                          ? "badge-error"
                          : item.severity === "medium"
                            ? "badge-warning"
                            : "badge-ghost"
                      }`}
                    >
                      {item.severity}
                    </span>
                  ) : null}
                  {item.verified ? (
                    <span className="badge badge-sm badge-success">verified</span>
                  ) : null}
                  {item.component ? (
                    <span className="badge badge-sm badge-outline">{item.component}</span>
                  ) : null}
                  {item.versions ? (
                    <span className="badge badge-sm badge-outline">{item.versions}</span>
                  ) : null}
                  {typeof item.hit_count === "number" && item.hit_count > 0 ? (
                    <span className="text-[11px] text-muted">
                      {translateUi("历史复用")} {item.hit_count}
                    </span>
                  ) : null}
                </div>
                <p className="mt-1.5 text-sm font-medium leading-5">
                  {item.title ?? item.knowledge_id}
                </p>
                {item.cve ? <p className="mt-1 text-xs text-muted">{item.cve}</p> : null}
                <p className="mt-1 text-[11px] text-muted">
                  {item.knowledge_id}
                  {item.source ? ` · ${item.source}` : ""}
                  {item.updated_at ? ` · ${formatDateTime(item.updated_at)}` : ""}
                </p>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
