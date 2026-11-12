/** 壳层 Layout（P7-1 五区版）：顶部栏（工作区切换 / 全局搜索 / 运行任务 /
 *  Runner 状态 / 通知）+ 左侧主导航图标轨 + 中央多标签工作区（固定 tab /
 *  会话 tab，固定 tab 来自 workspaces.ts 数据）+ 右侧可折叠 Inspector +
 *  底部可折叠 Activity 面板。
 *
 *  布局与数据模型分离：TabArea / InspectorPanel / ActivityPanel 是与数据
 *  无关的通用组件，RIP 的 tab 组装、Inspector 分区内容、Activity 页签内容
 *  全部在本文件完成；工作区数据仍只来自 src/workspaces.ts（同一份数据源）。
 *  模式沿自参照实现 admin/src/components/Layout.tsx 与计划 §10.2 主布局。 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, Outlet, useLocation, useNavigate } from "react-router";
import {
  Bell,
  Bot,
  CirclePlay,
  Database,
  FolderKanban,
  LayoutDashboard,
  Menu,
  Moon,
  PanelBottom,
  PanelRight,
  Puzzle,
  Radar,
  Search,
  ServerCog,
  Settings,
  Sun,
  Workflow,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import { ActivityPanel, type ActivityTab } from "./ActivityPanel";
import { ErrorBoundary } from "./ErrorBoundary";
import { GlobalSearchPalette } from "./GlobalSearchPalette";
import { InspectorPanel, type InspectorSection } from "./InspectorPanel";
import { TabArea, type TabAreaTab } from "./TabArea";
import { applyTheme, getStoredTheme, THEME_OPTIONS, type ThemeId } from "../theme";
import { WORKSPACES, WORKSPACES_BY_ORDER, tabNumberOf, type WorkspaceIconKey } from "../workspaces";

/** workspaces.ts 的图标键 → lucide 组件。Record 全键覆盖：加新键漏映射会直接 tsc 报错。 */
const ICONS: Record<WorkspaceIconKey, LucideIcon> = {
  layoutDashboard: LayoutDashboard,
  folderKanban: FolderKanban,
  radar: Radar,
  database: Database,
  wrench: Wrench,
  puzzle: Puzzle,
  bot: Bot,
  workflow: Workflow,
  circlePlay: CirclePlay,
  serverCog: ServerCog,
  settings: Settings,
};

/**
 * 沉浸态正则（预留）：命中时整宽渲染，图标轨/顶栏/标签区/Inspector/Activity
 * 隐藏（顶栏返回退出）。RIP 尚无沉浸页面（工具详情编辑、项目工作台等在后续
 * 批次落地时把路径模式加进这里即可，Layout 结构不用动）；当前正则刻意不命中
 * 任何既有路由。
 */
export const IMMERSIVE_PATH = /\/(?:tools|projects|knowledge)\/[^/]+\/(?:edit|studio)$/;

function isActivePath(itemPath: string, pathname: string): boolean {
  return itemPath === "/"
    ? pathname === "/"
    : pathname === itemPath || pathname.startsWith(`${itemPath}/`);
}

function workspaceIdOf(pathname: string): string | null {
  const normalized = pathname.replace(/\/+$/, "") || "/";
  for (const workspace of WORKSPACES) {
    if (workspace.tabs.some((item) => isActivePath(item.to, normalized))) return workspace.id;
  }
  return null;
}

function normalizePath(pathname: string): string {
  return pathname.replace(/\/+$/, "") || "/";
}

/** 会话 tab 的对象类别（Inspector「当前对象」用）；按详情路由前缀识别。 */
const SESSION_OBJECT_PATTERNS: ReadonlyArray<{
  pattern: RegExp;
  kind: string;
  labelOf: (match: RegExpMatchArray) => string;
}> = [
  { pattern: /^\/tools\/([^/]+)$/, kind: "工具", labelOf: (m) => decodeURIComponent(m[1] ?? "") },
  { pattern: /^\/runs\/([^/]+)$/, kind: "Run", labelOf: (m) => decodeURIComponent(m[1] ?? "") },
  { pattern: /^\/plugins\/([^/]+)$/, kind: "插件", labelOf: (m) => decodeURIComponent(m[1] ?? "") },
];

function sessionObjectOf(path: string): { kind: string; label: string } | null {
  for (const entry of SESSION_OBJECT_PATTERNS) {
    const match = path.match(entry.pattern);
    if (match) return { kind: entry.kind, label: entry.labelOf(match) };
  }
  return null;
}

/** 中央区的会话 tab（对象详情 / 深链接打开的文档），按最近访问排序（末尾最新）。 */
interface SessionTab {
  id: string;
  label: string;
}

export function Layout() {
  const location = useLocation();
  const navigate = useNavigate();
  const [drawer, setDrawer] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [notifOpen, setNotifOpen] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(true);
  const [activityOpen, setActivityOpen] = useState(true);
  const [activityTab, setActivityTab] = useState("queue");
  const [sessionTabs, setSessionTabs] = useState<SessionTab[]>([]);
  const [theme, setTheme] = useState<ThemeId>(() => getStoredTheme());
  const notifRef = useRef<HTMLDivElement>(null);
  const closeSearch = useCallback(() => setSearchOpen(false), []);

  const normalizedPath = normalizePath(location.pathname);
  const immersive = IMMERSIVE_PATH.test(normalizedPath);

  // 固定 tab id 全集（十一工作区的全部着陆 tab，来自 workspaces.ts 数据）。
  const fixedIds = useMemo(
    () => new Set(WORKSPACES_BY_ORDER.flatMap((workspace) => workspace.tabs.map((tab) => tab.to))),
    [],
  );

  // 深链接 / 详情路由到达时登记会话 tab（固定 tab 不重复登记）。
  useEffect(() => {
    if (immersive || fixedIds.has(normalizedPath)) return;
    setSessionTabs((prev) => {
      if (prev.length > 0 && prev[prev.length - 1]!.id === normalizedPath) {
        const label = sessionObjectOf(normalizedPath)?.label ?? normalizedPath;
        return prev[prev.length - 1]!.label === label
          ? prev
          : [...prev.slice(0, -1), { id: normalizedPath, label }];
      }
      return [
        ...prev.filter((tab) => tab.id !== normalizedPath),
        { id: normalizedPath, label: sessionObjectOf(normalizedPath)?.label ?? normalizedPath },
      ];
    });
  }, [normalizedPath, immersive, fixedIds]);

  // ⌘K / Ctrl+K 开关全局搜索面板。
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "k") return;
      event.preventDefault();
      setSearchOpen((open) => !open);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  // 通知气泡：Esc 与点击面板外关闭。
  useEffect(() => {
    if (!notifOpen) return undefined;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setNotifOpen(false);
    };
    const onMouseDown = (event: MouseEvent) => {
      if (notifRef.current && !notifRef.current.contains(event.target as Node)) {
        setNotifOpen(false);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    document.addEventListener("mousedown", onMouseDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("mousedown", onMouseDown);
    };
  }, [notifOpen]);

  // 抽屉打开期间锁 body 滚动 + Esc 关闭（参照实现事故教训：滚动锁与 Esc 缺一不可）。
  useEffect(() => {
    if (!drawer) return undefined;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setDrawer(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [drawer]);

  // 路由变化自动收起抽屉与通知气泡。
  useEffect(() => {
    setDrawer(false);
    setNotifOpen(false);
  }, [location.pathname]);

  const activeWorkspaceId = workspaceIdOf(location.pathname);
  const activeWorkspace =
    WORKSPACES.find((workspace) => workspace.id === activeWorkspaceId) ?? null;

  const title = useMemo(() => {
    if (activeWorkspace) {
      const tab = activeWorkspace.tabs.find((item) => isActivePath(item.to, normalizedPath));
      return tab ? tab.label : activeWorkspace.label;
    }
    return "总览";
  }, [activeWorkspace, normalizedPath]);

  const activeTabId = useMemo(() => {
    if (fixedIds.has(normalizedPath) || sessionTabs.some((tab) => tab.id === normalizedPath)) {
      return normalizedPath;
    }
    const tab = activeWorkspace?.tabs.find((item) => isActivePath(item.to, normalizedPath));
    return tab?.to ?? null;
  }, [normalizedPath, fixedIds, sessionTabs, activeWorkspace]);

  /** 中央标签区数据：固定 tab（十一工作区着陆页，pinned）+ 会话 tab（最近访问）。 */
  const tabs = useMemo<TabAreaTab[]>(() => {
    const fixed: TabAreaTab[] = WORKSPACES_BY_ORDER.flatMap((workspace) =>
      workspace.tabs.map((tab) => {
        const Icon = ICONS[tab.icon];
        return {
          id: tab.to,
          label: tab.label,
          icon: <Icon className="h-3.5 w-3.5" aria-hidden="true" />,
          pinned: true,
          number: tabNumberOf(workspace, tab),
          title: `${tabNumberOf(workspace, tab)} · ${tab.label} · ${tab.hint}`,
        };
      }),
    );
    const session: TabAreaTab[] = sessionTabs.map((tab) => ({
      id: tab.id,
      label: tab.label,
      title: tab.id,
    }));
    return [...fixed, ...session];
  }, [sessionTabs]);

  const closeTab = useCallback(
    (id: string) => {
      const remaining = sessionTabs.filter((tab) => tab.id !== id);
      setSessionTabs(remaining);
      if (id === normalizedPath) {
        const fallback =
          remaining.length > 0
            ? remaining[remaining.length - 1]!.id
            : (WORKSPACES.find((workspace) => workspace.id === workspaceIdOf(id))?.entry ?? "/");
        navigate(fallback);
      }
    },
    [sessionTabs, normalizedPath, navigate],
  );

  const inspectorSections = useMemo<InspectorSection[]>(() => {
    const tab = activeWorkspace?.tabs.find((item) => isActivePath(item.to, normalizedPath));
    const object = sessionObjectOf(normalizedPath);
    const rows: Array<[string, string]> = [
      ["页面", tab ? tab.label : title],
      [
        "工作区",
        activeWorkspace ? `${activeWorkspace.order} · ${activeWorkspace.label}` : "1 · Dashboard",
      ],
    ];
    if (activeWorkspace && tab) rows.push(["编号", tabNumberOf(activeWorkspace, tab)]);
    if (object) {
      rows.push(["对象", `${object.kind} ${object.label}`]);
      rows.push(["路径", normalizedPath]);
    } else {
      rows.push(["路径", normalizedPath]);
    }
    if (tab) rows.push(["说明", tab.hint]);
    return [
      {
        id: "object",
        title: "当前对象",
        body: (
          <dl className="inspector-rows">
            {rows.map(([key, value]) => (
              <div key={key} className="inspector-row">
                <dt>{key}</dt>
                <dd>{value}</dd>
              </div>
            ))}
          </dl>
        ),
      },
      {
        id: "related",
        title: "关联知识 / 工具",
        body: (
          <div className="inspector-links">
            <Link to="/knowledge">知识库</Link>
            <Link to="/tools">工具中心</Link>
            <p className="inspector-note">对象级关联（当前条目引用的知识与工具）随详情批次接入。</p>
          </div>
        ),
      },
      {
        id: "run",
        title: "运行配置",
        body: (
          <div className="inspector-links">
            <Link to="/runs">运行记录</Link>
            <Link to="/runtime">运行时</Link>
            <p className="inspector-note">对象级试运行配置随工具详情批次接入。</p>
          </div>
        ),
      },
    ];
  }, [activeWorkspace, normalizedPath, title]);

  const activityTabs = useMemo<ActivityTab[]>(
    () => [
      {
        id: "queue",
        label: "任务队列",
        body: (
          <p className="activity-note">
            任务队列视图在<Link to="/projects">项目区</Link>；活动面板的实时队列随任务批次接入。
          </p>
        ),
      },
      {
        id: "runs",
        label: "Tool Runs",
        body: (
          <p className="activity-note">
            Tool Runs 完整列表在<Link to="/runs">运行记录</Link>；此处后续接入运行中的 Run
            实时状态。
          </p>
        ),
      },
      {
        id: "logs",
        label: "日志",
        body: (
          <p className="activity-note">
            Run 日志流随运行详情批次接入，当前可在<Link to="/runs">运行记录</Link>查看单次 Run 与
            Artifact。
          </p>
        ),
      },
      {
        id: "output",
        label: "输出",
        body: (
          <p className="activity-note">
            输出面板占位：Artifact 预览在<Link to="/runs">运行记录</Link>页。
          </p>
        ),
      },
      {
        id: "status",
        label: "状态",
        body: (
          <p className="activity-note">
            Runner 状态与节点心跳在<Link to="/runtime">运行时</Link>区。
          </p>
        ),
      },
    ],
    [],
  );

  const toggleTheme = () => {
    const next: ThemeId = theme === "rip-dark" ? "rip-light" : "rip-dark";
    applyTheme(next);
    setTheme(next);
  };

  const themeName = THEME_OPTIONS.find((option) => option.id === theme)?.name ?? theme;

  return (
    <div className={`shell${immersive ? " is-immersive" : ""}`}>
      {drawer ? (
        <button
          type="button"
          aria-label="关闭菜单"
          className="rail-backdrop"
          onClick={() => setDrawer(false)}
        />
      ) : null}
      <aside className={`rail${drawer ? " is-open" : ""}`} aria-label="工作区图标轨">
        <div className="rail-brand">
          <Link to="/" aria-label="回到总览" title="总览">
            <span className="rail-brand-mark">
              <Radar className="h-5 w-5" aria-hidden="true" />
            </span>
          </Link>
        </div>
        <nav aria-label="工作区" className="rail-nav">
          {WORKSPACES.map((workspace) => {
            const Icon = ICONS[workspace.icon];
            const active = activeWorkspaceId === workspace.id;
            return (
              <Link
                key={workspace.id}
                to={workspace.entry}
                title={`${workspace.order} · ${workspace.label} · ${workspace.hint}`}
                aria-label={workspace.label}
                aria-current={active ? "page" : undefined}
                className={`rail-icon${active ? " is-active" : ""}`}
              >
                <Icon className="h-5 w-5 shrink-0" aria-hidden="true" />
                <span className="rail-label">{workspace.label}</span>
              </Link>
            );
          })}
        </nav>
        <div className="rail-foot">
          <button
            type="button"
            onClick={toggleTheme}
            title={`切换主题（当前：${themeName}）`}
            aria-label={`切换主题（当前：${themeName}）`}
            className="rail-icon"
          >
            {theme === "rip-dark" ? (
              <Sun className="h-[18px] w-[18px] shrink-0" aria-hidden="true" />
            ) : (
              <Moon className="h-[18px] w-[18px] shrink-0" aria-hidden="true" />
            )}
            <span className="rail-label">切换主题</span>
          </button>
        </div>
      </aside>
      <div className="shell-body">
        {/* 顶部栏五要素：工作区切换 / 全局搜索（命令面板）/ 运行任务 / Runner 状态 / 通知。 */}
        <header className="topbar">
          <button
            type="button"
            className="menu-btn"
            aria-label="打开菜单"
            aria-expanded={drawer}
            onClick={() => setDrawer(true)}
          >
            <Menu className="h-4 w-4" aria-hidden="true" />
          </button>
          <select
            className="ws-switch"
            aria-label="切换工作区"
            value={activeWorkspaceId ?? ""}
            onChange={(event) => {
              const workspace = WORKSPACES_BY_ORDER.find((item) => item.id === event.target.value);
              if (workspace) navigate(workspace.entry);
            }}
          >
            <option value="" disabled hidden>
              工作区
            </option>
            {WORKSPACES_BY_ORDER.map((workspace) => (
              <option key={workspace.id} value={workspace.id}>
                {workspace.order} · {workspace.label}
              </option>
            ))}
          </select>
          <h1 className="topbar-title">{title}</h1>
          <div className="topbar-actions">
            <button
              type="button"
              className="topbar-item search-trigger"
              onClick={() => setSearchOpen(true)}
            >
              <Search className="h-4 w-4" aria-hidden="true" />
              <span className="topbar-label">搜索</span>
              <kbd className="search-kbd">⌘K</kbd>
            </button>
            <Link className="topbar-item" to="/runs" title="运行任务（Run 列表）">
              <CirclePlay className="h-4 w-4" aria-hidden="true" />
              <span className="topbar-label">运行任务</span>
            </Link>
            <Link className="topbar-item" to="/runtime" title="Runner 状态">
              <ServerCog className="h-4 w-4" aria-hidden="true" />
              <span className="topbar-label">Runner</span>
            </Link>
            <div className="notif-wrap" ref={notifRef}>
              <button
                type="button"
                className="topbar-item"
                aria-label="通知"
                aria-expanded={notifOpen}
                onClick={() => setNotifOpen((open) => !open)}
              >
                <Bell className="h-4 w-4" aria-hidden="true" />
                <span className="topbar-label">通知</span>
              </button>
              {notifOpen ? (
                <div className="notif-pop" role="note" aria-label="通知列表">
                  <p className="notif-empty">暂无通知</p>
                  <p className="notif-note">通知中心随后续批次接入。</p>
                </div>
              ) : null}
            </div>
            <button
              type="button"
              className="topbar-item"
              aria-label="Inspector"
              aria-expanded={inspectorOpen}
              title={inspectorOpen ? "折叠 Inspector" : "展开 Inspector"}
              onClick={() => setInspectorOpen((open) => !open)}
            >
              <PanelRight className="h-4 w-4" aria-hidden="true" />
            </button>
            <button
              type="button"
              className="topbar-item"
              aria-label="Activity 面板"
              aria-expanded={activityOpen}
              title={activityOpen ? "折叠 Activity 面板" : "展开 Activity 面板"}
              onClick={() => setActivityOpen((open) => !open)}
            >
              <PanelBottom className="h-4 w-4" aria-hidden="true" />
            </button>
          </div>
        </header>
        {/* 中央多标签工作区：固定 tab（十一工作区着陆页）+ 会话 tab（深链接/
            详情对象，可关闭，按最近访问排序）。全宽贴顶栏，沉浸态不出现。 */}
        {!immersive ? (
          <TabArea
            tabs={tabs}
            activeId={activeTabId}
            onSelect={(id) => navigate(id)}
            onClose={closeTab}
            label="工作区标签页"
          />
        ) : null}
        <div className="shell-workarea">
          <main className="shell-center">
            {/* 按路由 re-key：单页渲染错误不炸整个壳，导航离开即复位（参照实现教训）。 */}
            <ErrorBoundary key={location.pathname} scope="route">
              <Outlet />
            </ErrorBoundary>
          </main>
          <InspectorPanel
            open={inspectorOpen && !immersive}
            sections={inspectorSections}
            label="Inspector"
          />
        </div>
        <ActivityPanel
          open={activityOpen && !immersive}
          tabs={activityTabs}
          activeId={activityTab}
          onSelect={setActivityTab}
          label="Activity 面板"
        />
      </div>
      {searchOpen ? <GlobalSearchPalette onClose={closeSearch} /> : null}
    </div>
  );
}
