import { useCallback, useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { formatDateTime } from "../lib/format";

/** 知识库页（P5-6）：列表 + 关键词检索（/api/knowledge?q=，服务端 LIKE，
 *  q 为 keyword 别名）+ 详情侧栏（来源/元数据/正文摘要）。
 *  检索 300ms 防抖（useApi 按路径重取，SWR 旧数据留屏）；三态统一：
 *  loading= SkeletonPanel、error= ErrorPanel（可重试）、无命中= EmptyPanel
 *  显服务端真实空态文案。详情按 ?knowledge_id= 全字段拉取。 */

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
  if (verified === 1) return "已验证";
  if (verified === 0) return "未验证";
  return "未标注";
}

function truncate(text: string | null | undefined, max: number): string | null {
  if (!text) return null;
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

function MetaRow({ label, value }: { label: string; value: string | null | undefined }) {
  if (value === null || value === undefined || value === "") return null;
  return (
    <div className="flex gap-2 text-xs">
      <span className="w-20 shrink-0 text-base-content/50">{label}</span>
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
    return <EmptyPanel text="从左侧选择条目查看详情（来源、元数据与正文摘要）。" />;
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
  if (!item) return <EmptyPanel text="条目详情不存在。" />;
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
            <span className="badge badge-sm badge-error">反知识</span>
          ) : null}
        </div>
        <h2 className="mt-2 text-sm font-semibold leading-5">{item.title}</h2>
        <p className="mt-1 break-all text-[11px] text-base-content/40">{item.knowledge_id}</p>
      </div>

      {item.description ? (
        <section>
          <h3 className="text-xs font-semibold text-base-content/60">描述</h3>
          <p className="mt-1 whitespace-pre-wrap text-xs leading-5">{item.description}</p>
        </section>
      ) : null}

      <section>
        <h3 className="text-xs font-semibold text-base-content/60">元数据</h3>
        <div className="mt-1 space-y-1">
          <MetaRow label="组件" value={item.component} />
          <MetaRow label="CVE" value={item.cve} />
          <MetaRow label="等级" value={item.severity} />
          <MetaRow label="语言" value={item.language} />
          <MetaRow label="证据级" value={item.evidence_level} />
          <MetaRow
            label="命中"
            value={
              item.hit_count === null || item.hit_count === undefined
                ? null
                : String(item.hit_count)
            }
          />
          <MetaRow label="更新" value={item.updated_at ? formatDateTime(item.updated_at) : null} />
          <MetaRow label="验证注记" value={truncate(item.verified_note, 200)} />
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
        <h3 className="text-xs font-semibold text-base-content/60">来源</h3>
        <div className="mt-1 space-y-1">
          <MetaRow label="来源" value={item.source} />
          {item.source_url ? (
            <div className="flex gap-2 text-xs">
              <span className="w-20 shrink-0 text-base-content/50">链接</span>
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
        <h3 className="text-xs font-semibold text-base-content/60">正文摘要</h3>
        {item.content ? (
          <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap rounded-control bg-base-200 p-2 text-[11px] leading-4">
            {truncate(item.content, 1200)}
          </pre>
        ) : item.usage ? (
          <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap rounded-control bg-base-200 p-2 text-[11px] leading-4">
            {truncate(item.usage, 1200)}
          </pre>
        ) : (
          <p className="mt-1 text-xs text-base-content/50">该条目无正文内容（仅元数据记录）。</p>
        )}
      </section>
    </div>
  );
}

export function KnowledgePage() {
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
      <PageHeader
        title="知识库"
        actions={
          <input
            type="text"
            className="input input-sm w-56 rounded-control border border-edge bg-base-100"
            placeholder="检索标题 / 组件 / CVE…"
            aria-label="检索知识条目"
            value={rawQuery}
            onChange={(event) => setRawQuery(event.target.value)}
          />
        }
      />
      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_380px]">
        <section aria-label="知识条目列表">
          {list.loading ? (
            <SkeletonPanel lines={6} />
          ) : list.error !== null ? (
            <ErrorPanel error={list.error} onRetry={list.retry} />
          ) : items.length === 0 ? (
            <EmptyPanel
              text={
                list.data?.message ??
                (keyword ? `没有匹配「${keyword}」的知识条目。` : "暂无知识条目。")
              }
            />
          ) : (
            <>
              <p className="mb-2 text-xs text-base-content/50">
                {keyword ? `检索「${keyword}」：` : ""}共 {total} 条（显示 {items.length}）
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
                            <span className="text-[11px] text-base-content/50">
                              {item.component}
                            </span>
                          ) : null}
                          {item.severity ? (
                            <span className="text-[11px] text-base-content/50">
                              {item.severity}
                            </span>
                          ) : null}
                        </div>
                        <p className="mt-1.5 text-sm font-medium leading-5">{item.title}</p>
                        <p className="mt-1 text-[11px] text-base-content/40">
                          {item.updated_at ? formatDateTime(item.updated_at) : "-"}
                          {item.hit_count ? ` · 命中 ${item.hit_count}` : ""}
                        </p>
                      </button>
                    </li>
                  );
                })}
              </ul>
            </>
          )}
        </section>
        <aside aria-label="条目详情" className="rounded-surface border border-edge bg-base-100 p-4">
          <DetailSidebar detail={detail} selectedId={selectedId} />
        </aside>
      </div>
    </div>
  );
}
