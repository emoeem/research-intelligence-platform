/** ⌘K 全局搜索面板：匹配/分组/排序在 lib/global-search.ts（P5-2 纯模块），
 *  本组件只负责取数与交互（模式沿自参照实现 Layout.tsx 的搜索面板段）。
 *
 *  页面/工具/任务入口仍使用各自列表数据；知识、资源、Nuclei 与 POC 命中
 *  统一走 /api/search（PostgreSQL + SQLite），输入防抖后服务端检索，不再被
 *  /api/knowledge 的 200 条客户端截断限制。 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Search } from "lucide-react";
import { useApi } from "../lib/hooks";
import {
  searchGlobalSearch,
  type GlobalSearchGroupSource,
  type GlobalSearchHit,
  type GlobalSearchGroupResult,
} from "../lib/global-search";
import { WORKSPACES } from "../workspaces";
import { trapFocusWithin } from "./focus-trap";
import { useUiText } from "../i18n";

interface ToolRow {
  tool_id: string;
  name: string;
  lifecycle?: string | null;
}

interface TaskRow {
  task_id: string;
  title: string;
  state?: string | null;
}

interface UnifiedSearchHit {
  store: string;
  id: string;
  title: string;
  detail?: Record<string, unknown>;
}

interface UnifiedSearchResponse {
  hits?: UnifiedSearchHit[];
  returned?: number;
  channels?: { pg_intel?: number; sqlite_knowledge?: number; sqlite_error?: string | null };
}

/** 「页面」组：workspaces.ts 的真实路由 + 总览，静态数据（不是假索引）。 */
const PAGE_GROUP: GlobalSearchGroupSource = {
  group: "pages",
  label: "页面",
  items: [
    { text: "总览 Overview", to: "/", title: "总览" },
    ...WORKSPACES.flatMap((workspace) =>
      workspace.tabs.map((tab) => ({
        text: `${tab.label} ${workspace.label}`,
        title: tab.label,
        to: tab.to,
        meta: `${workspace.order}.${tab.order} ${workspace.label}`,
      })),
    ),
  ],
};

/** 空态热门入口（静态功能入口文案，不是数据查询词）。 */
const POPULAR_TERMS: ReadonlyArray<{ label: string; to: string; hint: string }> = [
  { label: "总览", to: "/", hint: "平台 KPI 与数据面状态" },
  { label: "工具中心", to: "/tools", hint: "工具目录与详情" },
  { label: "知识库", to: "/knowledge", hint: "知识条目检索" },
  { label: "运行记录", to: "/runs", hint: "Run 列表与 Artifact" },
];

export function GlobalSearchPalette({ onClose }: { onClose: () => void }) {
  const navigate = useNavigate();
  const ui = useUiText();
  const panelRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const restoreRef = useRef<HTMLElement | null>(null);
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  const tools = useApi<{ items?: ToolRow[] }>("/api/tools?limit=200");
  const tasks = useApi<{ items?: TaskRow[] }>("/api/tasks?limit=200");
  useEffect(() => {
    const timer = window.setTimeout(() => setDebouncedQuery(query.trim()), 250);
    return () => window.clearTimeout(timer);
  }, [query]);
  const searchPath = debouncedQuery
    ? `/api/search?q=${encodeURIComponent(debouncedQuery)}&limit=20`
    : null;
  const unified = useApi<UnifiedSearchResponse>(searchPath);

  const sources = useMemo<GlobalSearchGroupSource[]>(() => {
    const translatedPages: GlobalSearchGroupSource = {
      ...PAGE_GROUP,
      label: ui("页面"),
      items: PAGE_GROUP.items.map((item) => ({
        ...item,
        title: item.title === "总览" ? ui("总览") : item.title,
        text: item.text,
      })),
    };
    const groups: GlobalSearchGroupSource[] = [translatedPages];
    const toolItems = (tools.data?.items ?? []).map((tool) => ({
      text: `${tool.name} ${tool.tool_id}`,
      title: tool.name,
      to: `/tools/${encodeURIComponent(tool.tool_id)}`,
      ...(tool.lifecycle ? { meta: tool.lifecycle } : {}),
    }));
    if (toolItems.length > 0) groups.push({ group: "tools", label: ui("工具"), items: toolItems });
    const taskItems = (tasks.data?.items ?? []).map((item) => ({
      text: `${item.title} ${item.task_id}`,
      title: item.title,
      to: `/projects?task_id=${encodeURIComponent(item.task_id)}`,
      ...(item.state ? { meta: item.state } : {}),
    }));
    if (taskItems.length > 0) groups.push({ group: "tasks", label: ui("任务"), items: taskItems });
    return groups;
  }, [tools.data, tasks.data, ui]);

  const localResult = useMemo(() => searchGlobalSearch(sources, query), [sources, query]);
  const remoteGroups = useMemo<GlobalSearchGroupResult[]>(() => {
    const groups = new Map<string, GlobalSearchGroupResult>();
    for (const hit of unified.data?.hits ?? []) {
      const detail = hit.detail ?? {};
      const resourceType = String(detail.resource_type ?? "");
      const group =
        hit.store === "pg-intel"
          ? "knowledge-cards"
          : resourceType === "rtis_poc"
            ? "poc-arsenal"
            : resourceType === "nuclei_template"
              ? "nuclei-arsenal"
              : "knowledge-sources";
      const label =
        group === "poc-arsenal"
          ? ui("自有 POC")
          : group === "nuclei-arsenal"
            ? ui("Nuclei 模板")
            : group === "knowledge-cards"
              ? ui("知识")
              : ui("知识来源");
      const to =
        hit.store === "pg-intel"
          ? `/knowledge?knowledge_id=${encodeURIComponent(hit.id)}`
          : resourceType === "rtis_poc"
            ? `/knowledge?view=pocs&q=${encodeURIComponent(hit.title)}`
            : `/knowledge?view=sources&q=${encodeURIComponent(hit.title)}`;
      const evidence = detail.evidence_level ? `证据 ${String(detail.evidence_level)}` : "";
      const quality = detail.quality_grade ? `质量 ${String(detail.quality_grade)}` : "";
      const meta = [
        hit.store === "pg-intel" ? "PostgreSQL" : "SQLite",
        resourceType,
        evidence || quality,
      ]
        .filter(Boolean)
        .join(" · ");
      const current = groups.get(group) ?? { group, label, hits: [] };
      if (current.hits.length < 5) {
        current.hits.push({ group, title: hit.title, to, meta });
      }
      groups.set(group, current);
    }
    return [...groups.values()];
  }, [unified.data, ui]);
  const result = useMemo(() => {
    const groups = [...localResult.groups, ...remoteGroups];
    return { query: localResult.query, groups, hits: groups.flatMap((group) => group.hits) };
  }, [localResult, remoteGroups, query]);
  // 键盘上下选择走扁平 hits；组渲染需要「命中在扁平序列里的位置」。
  const hitIndexes = useMemo(
    () => new Map(result.hits.map((hit, index) => [hit, index])),
    [result],
  );

  useEffect(() => {
    restoreRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    document.body.style.overflow = "hidden";
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      document.body.style.overflow = "";
      restoreRef.current?.focus?.();
    };
  }, [onClose]);

  // 关键词变了，选中项回到第一条；高亮项跟随滚动。
  useEffect(() => {
    setActiveIndex(0);
  }, [query]);
  useEffect(() => {
    // 滚动跟随是纯增强：scrollIntoView 缺失（jsdom/旧环境）时静默跳过。
    listRef.current?.querySelector('[data-active="true"]')?.scrollIntoView?.({ block: "nearest" });
  }, [activeIndex, result]);

  const openHit = (hit: GlobalSearchHit) => {
    onClose();
    navigate(hit.to);
  };

  const waitingForDebounce = query.trim() !== debouncedQuery;
  const isLoading = tools.loading || tasks.loading || waitingForDebounce || unified.loading;
  const loadFailed =
    !isLoading &&
    localResult.hits.length === 0 &&
    unified.error !== null &&
    tools.error !== null &&
    tasks.error !== null;

  return (
    <div
      className="search-overlay"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={panelRef}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-label={ui("全局搜索")}
        className="search-panel"
        onKeyDown={(event) => {
          trapFocusWithin(panelRef.current, event.nativeEvent);
          if (event.key === "Escape") {
            event.preventDefault();
            onClose();
            return;
          }
          if (event.key === "ArrowDown") {
            event.preventDefault();
            setActiveIndex((index) => Math.min(index + 1, Math.max(result.hits.length - 1, 0)));
            return;
          }
          if (event.key === "ArrowUp") {
            event.preventDefault();
            setActiveIndex((index) => Math.max(index - 1, 0));
            return;
          }
          if (event.key === "Enter") {
            const hit = result.hits[activeIndex];
            if (hit) {
              event.preventDefault();
              openHit(hit);
            }
          }
        }}
      >
        <div className="search-inputrow">
          <Search className="h-4 w-4 shrink-0" aria-hidden="true" />
          <input
            autoFocus
            type="text"
            className="search-input"
            aria-label={ui("搜索关键词")}
            placeholder={ui("搜索页面、工具、知识条目、任务…")}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
          <button
            type="button"
            className="search-esc"
            onClick={onClose}
            aria-label={ui("关闭搜索")}
          >
            Esc
          </button>
        </div>
        <div className="search-body" ref={listRef}>
          {query.trim() === "" ? (
            <div className="search-popular">
              <p className="search-popular-label">{ui("热门入口")}</p>
              <div className="search-chips">
                {POPULAR_TERMS.map((term) => (
                  <button
                    key={term.label}
                    type="button"
                    className="search-chip"
                    title={term.hint}
                    onClick={() => {
                      onClose();
                      navigate(term.to);
                    }}
                  >
                    {term.label}
                  </button>
                ))}
              </div>
            </div>
          ) : isLoading ? (
            <div className="search-hint">{ui("正在载入可搜索的数据…")}</div>
          ) : loadFailed ? (
            <div className="search-hint">{ui("数据载入失败，请关闭后重试。")}</div>
          ) : result.groups.length === 0 ? (
            <div className="search-hint">
              {unified.error
                ? unified.error.message
                : `${ui("没有找到相关内容")}「${query.trim()}」`}
            </div>
          ) : (
            <>
              {unified.error ? (
                <div className="search-hint">
                  {ui("知识搜索暂不可用")}: {unified.error.message}
                </div>
              ) : null}
              {result.groups.map((group) => (
                <section key={group.group}>
                  <h3 className="search-group-label">{group.label}</h3>
                  {group.hits.map((hit) => {
                    const index = hitIndexes.get(hit) ?? 0;
                    const active = index === activeIndex;
                    return (
                      <button
                        key={`${group.group}-${index}`}
                        type="button"
                        className={`search-hit${active ? " is-active" : ""}`}
                        data-active={active || undefined}
                        onMouseEnter={() => setActiveIndex(index)}
                        onClick={() => openHit(hit)}
                      >
                        <span className="search-hit-title">{hit.title}</span>
                        {hit.meta ? <span className="search-hit-meta">{hit.meta}</span> : null}
                      </button>
                    );
                  })}
                </section>
              ))}
            </>
          )}
        </div>
        <footer className="search-footnote">{ui("↑↓ 选择 · Enter 打开 · Esc 关闭")}</footer>
      </div>
    </div>
  );
}
