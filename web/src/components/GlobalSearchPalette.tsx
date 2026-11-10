/** ⌘K 全局搜索面板：匹配/分组/排序在 lib/global-search.ts（P5-2 纯模块），
 *  本组件只负责取数与交互（模式沿自参照实现 Layout.tsx 的搜索面板段）。
 *
 *  索引源全部是真实端点（P5-3 落地）：/api/tools、/api/knowledge、/api/tasks，
 *  另加一个「页面」组（workspaces.ts 的真实路由，静态数据）。打开面板时一次性
 *  拉取、关闭即卸载（下次打开重新拉取拿到新数据），limit=200 用满服务端
 *  MAX_LIMIT 即索引上限，超出部分属已知取舍（与参照实现同口径）。
 *  工具/知识/任务详情页尚未落地（P5-5/P5-6），命中落所属列表页并带定位参数，
 *  由对应批次消费。 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Search } from "lucide-react";
import { useApi } from "../lib/hooks";
import {
  searchGlobalSearch,
  type GlobalSearchGroupSource,
  type GlobalSearchHit,
} from "../lib/global-search";
import { WORKSPACES } from "../workspaces";
import { trapFocusWithin } from "./focus-trap";

interface ToolRow {
  tool_id: string;
  name: string;
  lifecycle?: string | null;
}

interface KnowledgeRow {
  knowledge_id: string;
  title: string;
  component?: string | null;
}

interface TaskRow {
  task_id: string;
  title: string;
  state?: string | null;
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
  const panelRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const restoreRef = useRef<HTMLElement | null>(null);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  const tools = useApi<{ items?: ToolRow[] }>("/api/tools?limit=200");
  const knowledge = useApi<{ items?: KnowledgeRow[] }>("/api/knowledge?limit=200");
  const tasks = useApi<{ items?: TaskRow[] }>("/api/tasks?limit=200");

  const sources = useMemo<GlobalSearchGroupSource[]>(() => {
    const groups: GlobalSearchGroupSource[] = [PAGE_GROUP];
    const toolItems = (tools.data?.items ?? []).map((tool) => ({
      text: `${tool.name} ${tool.tool_id}`,
      title: tool.name,
      to: `/tools/${encodeURIComponent(tool.tool_id)}`,
      ...(tool.lifecycle ? { meta: tool.lifecycle } : {}),
    }));
    if (toolItems.length > 0) groups.push({ group: "tools", label: "工具", items: toolItems });
    const knowledgeItems = (knowledge.data?.items ?? []).map((item) => ({
      text: `${item.title} ${item.component ?? ""}`,
      title: item.title,
      to: `/knowledge?knowledge_id=${encodeURIComponent(item.knowledge_id)}`,
      ...(item.component ? { meta: item.component } : {}),
    }));
    if (knowledgeItems.length > 0)
      groups.push({ group: "knowledge", label: "知识", items: knowledgeItems });
    const taskItems = (tasks.data?.items ?? []).map((item) => ({
      text: `${item.title} ${item.task_id}`,
      title: item.title,
      to: `/projects?task_id=${encodeURIComponent(item.task_id)}`,
      ...(item.state ? { meta: item.state } : {}),
    }));
    if (taskItems.length > 0) groups.push({ group: "tasks", label: "任务", items: taskItems });
    return groups;
  }, [tools.data, knowledge.data, tasks.data]);

  const result = useMemo(() => searchGlobalSearch(sources, query), [sources, query]);
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

  const isLoading = tools.loading || knowledge.loading || tasks.loading;
  const loadFailed =
    !isLoading &&
    [tools, knowledge, tasks].every((source) => source.error !== null && source.data === null);

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
        aria-label="全局搜索"
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
            aria-label="搜索关键词"
            placeholder="搜索页面、工具、知识条目、任务…"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
          <button type="button" className="search-esc" onClick={onClose} aria-label="关闭搜索">
            Esc
          </button>
        </div>
        <div className="search-body" ref={listRef}>
          {query.trim() === "" ? (
            <div className="search-popular">
              <p className="search-popular-label">热门入口</p>
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
            <div className="search-hint">正在载入可搜索的数据…</div>
          ) : loadFailed ? (
            <div className="search-hint">数据载入失败，请关闭后重试。</div>
          ) : result.groups.length === 0 ? (
            <div className="search-hint">没有找到「{query.trim()}」相关内容</div>
          ) : (
            result.groups.map((group) => (
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
            ))
          )}
        </div>
        <footer className="search-footnote">↑↓ 选择 · Enter 打开 · Esc 关闭</footer>
      </div>
    </div>
  );
}
