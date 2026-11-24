import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, Outlet, useLocation, useNavigate } from "react-router";
import {
  Bell,
  Bot,
  ChevronRight,
  CirclePlay,
  Database,
  FolderKanban,
  Globe,
  LayoutDashboard,
  Menu,
  Moon,
  PanelBottom,
  PanelLeftClose,
  PanelLeftOpen,
  PanelRight,
  Puzzle,
  Radar,
  RefreshCw,
  Search,
  ServerCog,
  Settings,
  ShieldCheck,
  Sun,
  Workflow,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ActivityPanel, type ActivityTab } from "./ActivityPanel";
import { PersistentShellSplit } from "./PersistentShellSplit";
import { ActivityPlansLive } from "./ActivityPlansLive";
import { ActivityRunsLive } from "./ActivityRunsLive";
import { ActivityRunDetails } from "./ActivityRunDetails";
import { ActivityRuntimeLive } from "./ActivityRuntimeLive";
import { ErrorBoundary } from "./ErrorBoundary";
import { GlobalSearchPalette } from "./GlobalSearchPalette";
import { InspectorPanel, type InspectorSection } from "./InspectorPanel";
import { TabArea, type TabAreaTab } from "./TabArea";
import { applyTheme, getStoredTheme, type ThemeId } from "../theme";
import { setAppLanguage, SUPPORTED_LANGUAGES, type AppLanguage } from "../i18n";
import { useApi } from "../lib/hooks";
import type { PluginList } from "../lib/api";
import {
  WORKSPACES,
  WORKSPACES_BY_ORDER,
  type Workspace,
  type WorkspaceGroup,
  type WorkspaceNavItem,
  type WorkspaceIconKey,
} from "../workspaces";

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
  shieldCheck: ShieldCheck,
  settings: Settings,
  globe: Globe,
  refreshCw: RefreshCw,
};

const NAV_GROUPS: readonly WorkspaceGroup[] = ["workspace", "capabilities", "operations", "system"];

export const IMMERSIVE_PATH = /\/(?:tools|projects|knowledge)\/[^/]+\/(?:edit|studio)$/;

function isActivePath(itemPath: string, pathname: string): boolean {
  return itemPath === "/"
    ? pathname === "/"
    : pathname === itemPath || pathname.startsWith(`${itemPath}/`);
}

function normalizePath(pathname: string): string {
  return pathname.replace(/\/+$/, "") || "/";
}

function workspaceIdOf(pathname: string): string | null {
  const normalized = normalizePath(pathname);
  for (const workspace of WORKSPACES) {
    if (workspace.tabs.some((item) => isActivePath(item.to, normalized))) return workspace.id;
  }
  return null;
}

function activeWorkspaceTab(
  workspace: Workspace | null,
  pathname: string,
): WorkspaceNavItem | null {
  if (!workspace) return null;
  return (
    [...workspace.tabs]
      .sort((a, b) => b.to.length - a.to.length)
      .find((item) => isActivePath(item.to, pathname)) ?? null
  );
}

const SESSION_OBJECT_PATTERNS: ReadonlyArray<{
  pattern: RegExp;
  kind: "tool" | "run" | "plugin" | "task" | "knowledge";
  labelOf: (match: RegExpMatchArray) => string;
}> = [
  { pattern: /^\/tools\/([^/]+)$/, kind: "tool", labelOf: (m) => decodeURIComponent(m[1] ?? "") },
  { pattern: /^\/runs\/([^/]+)$/, kind: "run", labelOf: (m) => decodeURIComponent(m[1] ?? "") },
  {
    pattern: /^\/plugins\/([^/]+)$/,
    kind: "plugin",
    labelOf: (m) => decodeURIComponent(m[1] ?? ""),
  },
  {
    pattern: /^\/research\/tasks\/([^/]+)$/,
    kind: "task",
    labelOf: (m) => decodeURIComponent(m[1] ?? ""),
  },
];

function sessionObjectOf(
  path: string,
): { kind: "tool" | "run" | "plugin" | "task" | "knowledge"; label: string } | null {
  for (const entry of SESSION_OBJECT_PATTERNS) {
    const match = path.match(entry.pattern);
    if (match) return { kind: entry.kind, label: entry.labelOf(match) };
  }
  return null;
}

interface SessionTab {
  id: string;
  label: string;
}

function initialSidebarCollapsed(): boolean {
  try {
    const stored = localStorage.getItem("rip_sidebar_collapsed");
    if (stored !== null) return stored === "true";
  } catch {
    // Use viewport defaults when storage is unavailable.
  }
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia("(max-width: 1599px)").matches
    : false;
}

export function Layout() {
  const { t, i18n } = useTranslation();
  const location = useLocation();
  const navigate = useNavigate();
  const serviceOverview = useApi<unknown>("/api/overview");
  const serviceRuntime = useApi<{ available?: boolean }>("/api/runtime?limit=5");
  const [drawer, setDrawer] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(initialSidebarCollapsed);
  const [searchOpen, setSearchOpen] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [activityOpen, setActivityOpen] = useState(false);
  const [activityTab, setActivityTab] = useState("queue");
  const [wideShell, setWideShell] = useState(
    () =>
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(min-width: 1280px)").matches,
  );
  const [sessionTabs, setSessionTabs] = useState<SessionTab[]>([]);
  const [theme, setTheme] = useState<ThemeId>(() => getStoredTheme());
  const closeSearch = useCallback(() => setSearchOpen(false), []);

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return undefined;
    const query = window.matchMedia("(min-width: 1280px)");
    const update = () => setWideShell(query.matches);
    update();
    query.addEventListener?.("change", update);
    return () => query.removeEventListener?.("change", update);
  }, []);

  const normalizedPath = normalizePath(location.pathname);
  const selectedRunId = normalizedPath.match(/^\/runs\/([^/]+)$/)?.[1] ?? null;
  const selectedToolId = normalizedPath.match(/^\/tools\/([^/]+)$/)?.[1] ?? null;
  const selectedPluginId = normalizedPath.match(/^\/plugins\/([^/]+)$/)?.[1] ?? null;
  const selectedTaskId = normalizedPath.match(/^\/research\/tasks\/([^/]+)$/)?.[1] ?? null;
  const selectedKnowledgeId =
    normalizedPath === "/knowledge"
      ? new URLSearchParams(location.search).get("knowledge_id")
      : null;
  const selectedTool = useApi<{
    tool?: {
      tool_id: string;
      name: string;
      kind?: string | null;
      lifecycle?: string | null;
      governance?: string | null;
      health_state?: string | null;
      permission_level?: string | null;
      timeout_s?: number | null;
    };
  }>(selectedToolId ? `/api/tools?tool_id=${encodeURIComponent(selectedToolId)}` : null);
  const selectedPlugins = useApi<PluginList>(selectedPluginId ? "/api/plugins" : null);
  const selectedKnowledge = useApi<{
    item?: {
      knowledge_id: string;
      title: string;
      lifecycle?: string | null;
      governance?: string | null;
      component?: string | null;
      severity?: string | null;
      source?: string | null;
      source_url?: string | null;
      updated_at?: string | null;
    };
  }>(
    selectedKnowledgeId
      ? `/api/knowledge?knowledge_id=${encodeURIComponent(selectedKnowledgeId)}`
      : null,
  );
  const selectedTask = useApi<{
    item?: {
      task_id: string;
      title: string;
      state?: string | null;
      target?: string | null;
      task_type?: string | null;
      objective?: string | null;
    };
  }>(selectedTaskId ? `/api/tasks?task_id=${encodeURIComponent(selectedTaskId)}` : null);
  const selectedTaskRuns = useApi<{
    items?: { run_id: string; tool_id: string; task_id?: string | null; status?: string }[];
  }>(selectedTaskId ? "/api/runs?limit=200" : null);
  const selectedToolRuns = useApi<{
    items?: { run_id: string; tool_id: string; status?: string }[];
  }>(selectedToolId ? "/api/runs?limit=200" : null);
  const selectedRun = useApi<{
    run?: {
      run_id: string;
      tool_id: string;
      status?: string;
      duration_ms?: number | null;
      error_category?: string | null;
      task_id?: string | null;
    };
  }>(selectedRunId ? `/api/runs?run_id=${encodeURIComponent(selectedRunId)}` : null);
  const immersive = IMMERSIVE_PATH.test(normalizedPath);
  const activeWorkspaceId = workspaceIdOf(location.pathname);
  const activeWorkspace =
    WORKSPACES.find((workspace) => workspace.id === activeWorkspaceId) ?? null;
  const currentWorkspaceTab = activeWorkspaceTab(activeWorkspace, normalizedPath);
  const title =
    currentWorkspaceTab && activeWorkspace
      ? t(`nav.tabs.${activeWorkspace.id}.${currentWorkspaceTab.order}`)
      : activeWorkspace
        ? t(`nav.workspaces.${activeWorkspace.id}`)
        : t("nav.tabs.dashboard.1");

  useEffect(() => {
    const object =
      sessionObjectOf(normalizedPath) ??
      (selectedKnowledgeId ? { kind: "knowledge" as const, label: selectedKnowledgeId } : null);
    if (immersive || !object) return;
    setSessionTabs((previous) => {
      if (previous.length > 0 && previous[previous.length - 1]!.id === normalizedPath) {
        return previous[previous.length - 1]!.label === object.label
          ? previous
          : [...previous.slice(0, -1), { id: normalizedPath, label: object.label }];
      }
      return [
        ...previous.filter((tab) => tab.id !== normalizedPath),
        { id: normalizedPath, label: object.label },
      ];
    });
  }, [normalizedPath, immersive]);

  useEffect(() => {
    try {
      localStorage.setItem("rip_sidebar_collapsed", String(sidebarCollapsed));
    } catch {
      // Keep the current in-memory preference.
    }
  }, [sidebarCollapsed]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "k") return;
      event.preventDefault();
      setSearchOpen((open) => !open);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

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

  useEffect(() => {
    setDrawer(false);
  }, [location.pathname]);

  useEffect(() => {
    const onThemeChange = (event: Event) => {
      const next = (event as CustomEvent<ThemeId>).detail;
      if (next === "rip-dark" || next === "rip-light") setTheme(next);
    };
    window.addEventListener("rip-theme-change", onThemeChange);
    return () => window.removeEventListener("rip-theme-change", onThemeChange);
  }, []);

  const currentWorkspaceLabel = activeWorkspace
    ? t(`nav.workspaces.${activeWorkspace.id}`)
    : t("nav.workspaces.dashboard");
  const currentWorkspaceHint = activeWorkspace
    ? t(`nav.hints.${activeWorkspace.id}`)
    : t("nav.hints.dashboard");

  const activeTabId = sessionTabs.some((tab) => tab.id === normalizedPath) ? normalizedPath : null;

  const tabs = useMemo<TabAreaTab[]>(
    () => sessionTabs.map((tab) => ({ id: tab.id, label: tab.label, title: tab.id })),
    [sessionTabs],
  );

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
    const object =
      sessionObjectOf(normalizedPath) ??
      (selectedKnowledgeId ? { kind: "knowledge" as const, label: selectedKnowledgeId } : null);
    const rows: Array<[string, string]> = [
      [t("inspector.page"), title],
      [t("inspector.workspace"), currentWorkspaceLabel],
    ];
    if (currentWorkspaceTab && activeWorkspace) {
      rows.push([t("inspector.number"), `${activeWorkspace.order}.${currentWorkspaceTab.order}`]);
    }
    if (object) {
      rows.push([
        t("inspector.object"),
        `${t(`inspector.objectKinds.${object.kind}`)} ${object.label}`,
      ]);
      rows.push([t("inspector.path"), normalizedPath]);
    } else {
      rows.push([t("inspector.path"), normalizedPath]);
    }
    const run = selectedRun.data?.run;
    const tool = selectedTool.data?.tool;
    const plugin = selectedPlugins.data?.items?.find((item) => item.plugin_id === selectedPluginId);
    const task = selectedTask.data?.item;
    const knowledge = selectedKnowledge.data?.item;
    if (tool) {
      rows.push([t("inspector.name"), tool.name]);
      if (tool.kind) rows.push([t("inspector.kind"), tool.kind]);
      if (tool.lifecycle) rows.push([t("inspector.lifecycle"), tool.lifecycle]);
      if (tool.governance) rows.push([t("inspector.governance"), tool.governance]);
      if (tool.health_state) rows.push([t("inspector.health"), tool.health_state]);
      if (tool.permission_level) rows.push([t("inspector.permission"), tool.permission_level]);
      if (tool.timeout_s !== null && tool.timeout_s !== undefined)
        rows.push([t("inspector.timeout"), `${tool.timeout_s} s`]);
    }
    if (plugin) {
      rows.push([t("inspector.name"), plugin.name]);
      rows.push([t("inspector.version"), plugin.version]);
      rows.push([t("inspector.kind"), plugin.kind]);
      rows.push([t("inspector.lifecycle"), plugin.state]);
      rows.push([t("inspector.trust"), plugin.trust_level]);
      rows.push([t("inspector.license"), plugin.license]);
    }
    if (knowledge) {
      rows.push([t("inspector.name"), knowledge.title]);
      if (knowledge.lifecycle) rows.push([t("inspector.lifecycle"), knowledge.lifecycle]);
      if (knowledge.governance) rows.push([t("inspector.governance"), knowledge.governance]);
      if (knowledge.component) rows.push([t("inspector.kind"), knowledge.component]);
      if (knowledge.severity) rows.push([t("inspector.severity"), knowledge.severity]);
      if (knowledge.source) rows.push([t("inspector.source"), knowledge.source]);
      if (knowledge.source_url) rows.push([t("inspector.sourceUrl"), knowledge.source_url]);
    }
    if (task) {
      rows.push([t("inspector.name"), task.title]);
      rows.push([t("inspector.state"), task.state ?? t("inspector.unknown")]);
      if (task.task_type) rows.push([t("inspector.kind"), task.task_type]);
      if (task.target) rows.push([t("inspector.target"), task.target]);
      if (task.objective) rows.push([t("inspector.objective"), task.objective]);
    }
    if (run) {
      rows.push([t("inspector.runStatus"), run.status ?? t("inspector.unknown")]);
      rows.push([t("inspector.tool"), run.tool_id]);
      if (run.duration_ms !== null && run.duration_ms !== undefined)
        rows.push([t("inspector.duration"), `${run.duration_ms} ms`]);
      if (run.error_category) rows.push([t("inspector.errorCategory"), run.error_category]);
      if (run.task_id) rows.push([t("inspector.task"), run.task_id]);
    }
    if (currentWorkspaceTab)
      rows.push([
        t("inspector.description"),
        t(`nav.tabHints.${activeWorkspace?.id}.${currentWorkspaceTab.order}`),
      ]);
    return [
      {
        id: "object",
        title: t("inspector.currentObject"),
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
        title: t("inspector.related"),
        body: (
          <div className="inspector-links">
            {knowledge?.source_url ? (
              <a href={knowledge.source_url} target="_blank" rel="noreferrer">
                {t("inspector.sourceUrl")}
              </a>
            ) : null}
            {run?.tool_id ? (
              <Link to={`/tools/${encodeURIComponent(run.tool_id)}`}>
                {t("inspector.relatedTool")}: {run.tool_id}
              </Link>
            ) : null}
            {run?.task_id ? (
              <Link to={`/research/tasks/${encodeURIComponent(run.task_id)}`}>
                {t("inspector.relatedTask")}: {run.task_id}
              </Link>
            ) : null}
            {task
              ? (selectedTaskRuns.data?.items ?? [])
                  .filter((item) => item.task_id === task.task_id)
                  .slice(0, 4)
                  .map((item) => (
                    <Link key={item.run_id} to={`/runs/${encodeURIComponent(item.run_id)}`}>
                      {t("inspector.relatedRun")}: {item.tool_id} · {item.status}
                    </Link>
                  ))
              : null}
            {tool
              ? (selectedToolRuns.data?.items ?? [])
                  .filter((item) => item.tool_id === tool.tool_id)
                  .slice(0, 4)
                  .map((item) => (
                    <Link key={item.run_id} to={"/runs/" + encodeURIComponent(item.run_id)}>
                      {t("inspector.relatedRun")}: {item.status ?? t("inspector.unknown")}
                    </Link>
                  ))
              : null}
            {plugin ? <Link to="/tools">{t("inspector.toolCenter")}</Link> : null}
            {!run?.tool_id && !run?.task_id && !task && !tool && !plugin && !knowledge ? (
              <p className="inspector-note">{t("inspector.noRelations")}</p>
            ) : null}
          </div>
        ),
      },
      {
        id: "run",
        title: t("inspector.runConfig"),
        body: (
          <div className="inspector-links">
            <Link to="/runs">{t("inspector.runs")}</Link>
            <Link to="/runtime">{t("inspector.runtime")}</Link>
            {tool?.timeout_s ? (
              <p className="inspector-note">
                {t("inspector.timeout")}: {tool.timeout_s} s
              </p>
            ) : null}
            {run?.error_category ? (
              <p className="inspector-note">
                {t("inspector.errorCategory")}: {run.error_category}
              </p>
            ) : null}
          </div>
        ),
      },
    ];
  }, [
    activeWorkspace,
    currentWorkspaceLabel,
    currentWorkspaceTab,
    normalizedPath,
    selectedRun.data,
    selectedTool.data,
    selectedPlugins.data,
    selectedPluginId,
    selectedTask.data,
    selectedTaskRuns.data,
    selectedKnowledge.data,
    selectedKnowledgeId,
    selectedToolRuns.data,
    t,
    title,
  ]);

  const activityTabs = useMemo<ActivityTab[]>(
    () => [
      { id: "queue", label: t("activity.queue"), body: <ActivityPlansLive /> },
      { id: "runs", label: t("activity.runs"), body: <ActivityRunsLive /> },
      {
        id: "logs",
        label: t("activity.logs"),
        body: <ActivityRunDetails mode="logs" />,
      },
      {
        id: "output",
        label: t("activity.output"),
        body: <ActivityRunDetails mode="output" />,
      },
      { id: "status", label: t("activity.status"), body: <ActivityRuntimeLive /> },
    ],
    [t],
  );

  const toggleTheme = () => {
    const next: ThemeId = theme === "rip-dark" ? "rip-light" : "rip-dark";
    applyTheme(next);
    setTheme(next);
  };
  const language = (i18n.resolvedLanguage ?? i18n.language) as AppLanguage;
  const serviceStatus =
    serviceOverview.loading || serviceRuntime.loading
      ? "checking"
      : serviceOverview.error?.status === 401 || serviceRuntime.error?.status === 401
        ? "authRequired"
        : serviceOverview.error !== null || serviceRuntime.error !== null
          ? "unavailable"
          : serviceRuntime.data?.available === false
            ? "degraded"
            : "healthy";
  const layoutMode =
    normalizedPath === "/"
      ? "dashboard"
      : normalizedPath.startsWith("/websec/")
        ? "workbench"
        : normalizedPath === "/workflows"
          ? "canvas"
          : normalizedPath === "/agents"
            ? "editor"
            : "content";

  return (
    <div className={`shell${immersive ? " is-immersive" : ""}`}>
      {drawer ? (
        <button
          type="button"
          aria-label={t("shell.closeMenu")}
          className="rail-backdrop"
          onClick={() => setDrawer(false)}
        />
      ) : null}
      <aside
        className={`rail${drawer ? " is-open" : ""}${sidebarCollapsed ? " is-collapsed" : " is-expanded"}`}
        aria-label={t("shell.globalNavigation")}
      >
        <div className="rail-brand">
          <Link to="/" aria-label={t("app.title")} title={t("app.title")}>
            <span className="rail-brand-mark" aria-hidden="true">
              R
            </span>
          </Link>
          <div className="rail-brand-copy">
            <span className="rail-brand-name">RIP</span>
            <small className="rail-brand-subtitle">Research Intelligence</small>
          </div>
          <button
            type="button"
            className="sidebar-collapse-btn"
            aria-label={sidebarCollapsed ? t("shell.expandSidebar") : t("shell.collapseSidebar")}
            title={sidebarCollapsed ? t("shell.expandSidebar") : t("shell.collapseSidebar")}
            onClick={() => setSidebarCollapsed((collapsed) => !collapsed)}
          >
            {sidebarCollapsed ? (
              <PanelLeftOpen className="h-4 w-4" aria-hidden="true" />
            ) : (
              <PanelLeftClose className="h-4 w-4" aria-hidden="true" />
            )}
          </button>
        </div>
        <nav aria-label={t("shell.globalNavigation")} className="rail-nav">
          {NAV_GROUPS.map((group) => {
            const workspaces = WORKSPACES_BY_ORDER.filter((workspace) => workspace.group === group);
            if (workspaces.length === 0) return null;
            return (
              <section className="rail-group" key={group} aria-label={t(`nav.groups.${group}`)}>
                <h2 className="rail-group-label">{t(`nav.groups.${group}`)}</h2>
                {workspaces.map((workspace) => {
                  const Icon = ICONS[workspace.icon];
                  const active = activeWorkspaceId === workspace.id;
                  const label = t(`nav.workspaces.${workspace.id}`);
                  const hint = t(`nav.hints.${workspace.id}`);
                  return (
                    <Link
                      key={workspace.id}
                      to={workspace.entry}
                      title={`${workspace.order} · ${label} · ${hint}`}
                      aria-label={label}
                      aria-current={active ? "page" : undefined}
                      className={`rail-icon${active ? " is-active" : ""}`}
                      onClick={() => setDrawer(false)}
                    >
                      <Icon className="h-[18px] w-[18px] shrink-0" aria-hidden="true" />
                      <span className="rail-label">{label}</span>
                    </Link>
                  );
                })}
              </section>
            );
          })}
        </nav>
        <div className="rail-foot">
          <div className="rail-profile" role="group" aria-label={t("shell.localWorkspace")}>
            <span className="rail-profile-mark" aria-hidden="true">
              L
            </span>
            <span className="rail-profile-copy">
              <strong>{t("shell.localWorkspace")}</strong>
              <small>{t("shell.localWorkspaceSubtitle")}</small>
            </span>
          </div>
        </div>
      </aside>

      <div className="shell-body">
        <header className="topbar">
          <button
            type="button"
            className="menu-btn"
            aria-label={t("shell.openMenu")}
            aria-expanded={drawer}
            onClick={() => setDrawer(true)}
          >
            <Menu className="h-4 w-4" aria-hidden="true" />
          </button>
          <div className="topbar-heading">
            <nav className="breadcrumbs" aria-label={t("shell.breadcrumb")}>
              <Link to="/">{t("nav.groups.workspace")}</Link>
              <ChevronRight className="h-3.5 w-3.5" aria-hidden="true" />
              <span aria-current="page">{title}</span>
            </nav>
          </div>
          <div className="topbar-actions">
            <button
              type="button"
              className="topbar-item search-trigger"
              onClick={() => setSearchOpen(true)}
              aria-label={`${t("shell.search")} ⌘K`}
              title={t("shell.searchShortcut")}
            >
              <Search className="h-4 w-4" aria-hidden="true" />
              <span className="topbar-label">{t("shell.search")}</span>
              <kbd className="search-kbd">⌘K</kbd>
            </button>
            <Link
              to="/runs"
              className="topbar-item"
              aria-label={t("shell.tasks")}
              title={t("shell.tasks")}
            >
              <CirclePlay className="h-4 w-4" aria-hidden="true" />
              <span className="topbar-label">{t("shell.tasks")}</span>
            </Link>
            <Link
              to="/runtime"
              className="topbar-item"
              aria-label={t("shell.runner")}
              title={t(`shell.serviceStatus.${serviceStatus}`)}
            >
              <ServerCog className="h-4 w-4" aria-hidden="true" />
              <span className="topbar-label">{t("shell.runner")}</span>
            </Link>
            <div className={`service-health is-${serviceStatus}`} role="status" aria-live="polite">
              <span className="service-health-dot" aria-hidden="true" />
              <span className="service-health-label">
                {t(`shell.serviceStatus.${serviceStatus}`)}
              </span>
            </div>
            <button
              type="button"
              className="topbar-icon-button"
              disabled
              aria-label={t("shell.notifications")}
              title={t("shell.notificationsUnavailable")}
            >
              <Bell className="h-4 w-4" aria-hidden="true" />
            </button>
            <label className="language-control">
              <Globe className="h-4 w-4" aria-hidden="true" />
              <span className="sr-only">{t("shell.language")}</span>
              <select
                className="language-switch"
                aria-label={t("shell.language")}
                value={language}
                onChange={(event) => void setAppLanguage(event.target.value as AppLanguage)}
              >
                {SUPPORTED_LANGUAGES.map((item) => (
                  <option key={item.code} value={item.code}>
                    {item.label}
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              className="topbar-icon-button"
              onClick={toggleTheme}
              aria-label={t("shell.switchTheme")}
              title={t("shell.switchTheme")}
            >
              {theme === "rip-dark" ? (
                <Moon className="h-4 w-4" aria-hidden="true" />
              ) : (
                <Sun className="h-4 w-4" aria-hidden="true" />
              )}
            </button>
            <button
              type="button"
              className="topbar-icon-button"
              aria-label={t("shell.inspector")}
              aria-expanded={inspectorOpen}
              title={inspectorOpen ? t("shell.closeInspector") : t("shell.openInspector")}
              onClick={() => setInspectorOpen((open) => !open)}
            >
              <PanelRight className="h-4 w-4" aria-hidden="true" />
            </button>
          </div>
        </header>

        {activeWorkspace && !immersive ? (
          <nav
            className={`workspace-nav${activeWorkspace.tabs.length === 1 ? " is-single" : ""}`}
            aria-label={t("shell.workspaceNavigation")}
          >
            <div className="workspace-nav-context">
              <span>{currentWorkspaceLabel}</span>
              <small>{currentWorkspaceHint}</small>
            </div>
            <div className="workspace-nav-items">
              {activeWorkspace.tabs.map((tab) => {
                const active = currentWorkspaceTab?.to === tab.to;
                const Icon = ICONS[tab.icon];
                return (
                  <Link
                    key={tab.to}
                    to={tab.to}
                    title={t(`nav.tabHints.${activeWorkspace.id}.${tab.order}`)}
                    aria-current={active ? "page" : undefined}
                    className={`workspace-nav-item${active ? " is-active" : ""}`}
                  >
                    <Icon className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>{t(`nav.tabs.${activeWorkspace.id}.${tab.order}`)}</span>
                  </Link>
                );
              })}
            </div>
          </nav>
        ) : null}

        {!immersive ? (
          <TabArea
            tabs={tabs}
            activeId={activeTabId}
            onSelect={(id) => navigate(id)}
            onClose={closeTab}
            closeLabel={(label) => `${t("common.close")} ${label}`}
            label={t("shell.sessionTabs")}
          />
        ) : null}

        {activityOpen && !immersive ? (
          <PersistentShellSplit
            orientation="vertical"
            storageKey="rip_shell_activity_height_v1"
            firstSize={76}
            firstMin={45}
            secondMin={20}
            separatorLabel={t("shell.resizeActivity")}
            first={
              <div className="shell-workarea">
                {wideShell && inspectorOpen ? (
                  <PersistentShellSplit
                    orientation="horizontal"
                    storageKey="rip_shell_inspector_width_v1"
                    firstSize={72}
                    firstMin={55}
                    secondMin={22}
                    separatorLabel={t("shell.resizeInspector")}
                    first={
                      <main className={"shell-center layout-" + layoutMode}>
                        <ErrorBoundary key={location.pathname} scope="route">
                          <Outlet />
                        </ErrorBoundary>
                      </main>
                    }
                    second={
                      <InspectorPanel
                        open
                        sections={inspectorSections}
                        label={t("shell.inspector")}
                      />
                    }
                    className="inspector-split"
                  />
                ) : (
                  <>
                    <main className={"shell-center layout-" + layoutMode}>
                      <ErrorBoundary key={location.pathname} scope="route">
                        <Outlet />
                      </ErrorBoundary>
                    </main>
                    {inspectorOpen && !wideShell ? (
                      <button
                        type="button"
                        className="inspector-backdrop"
                        aria-label={t("shell.closeInspector")}
                        onClick={() => setInspectorOpen(false)}
                      />
                    ) : null}
                    <InspectorPanel
                      open={inspectorOpen}
                      sections={inspectorSections}
                      label={t("shell.inspector")}
                    />
                  </>
                )}
              </div>
            }
            second={
              <div className="activity-dock-region">
                <div
                  className="activity-dock-bar"
                  role="region"
                  aria-label={t("shell.activityCollapsed")}
                >
                  <div className="activity-dock-context">
                    <PanelBottom className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>{t("shell.activityCollapsed")}</span>
                    <span className="activity-dock-hint">{t("shell.activityHint")}</span>
                  </div>
                  <button
                    type="button"
                    className="activity-dock-toggle"
                    aria-label={t("shell.closeActivity")}
                    aria-expanded="true"
                    title={t("shell.closeActivity")}
                    onClick={() => setActivityOpen(false)}
                  >
                    <span>{t("shell.closeActivity")}</span>
                    <ChevronRight className="h-3.5 w-3.5 is-rotated" aria-hidden="true" />
                  </button>
                </div>
                <ActivityPanel
                  open
                  tabs={activityTabs}
                  activeId={activityTab}
                  onSelect={setActivityTab}
                  label={t("shell.activity")}
                />
              </div>
            }
          />
        ) : (
          <>
            <div className="shell-workarea">
              {wideShell && inspectorOpen && !immersive ? (
                <PersistentShellSplit
                  orientation="horizontal"
                  storageKey="rip_shell_inspector_width_v1"
                  firstSize={72}
                  firstMin={55}
                  secondMin={22}
                  separatorLabel={t("shell.resizeInspector")}
                  first={
                    <main className={"shell-center layout-" + layoutMode}>
                      <ErrorBoundary key={location.pathname} scope="route">
                        <Outlet />
                      </ErrorBoundary>
                    </main>
                  }
                  second={
                    <InspectorPanel
                      open
                      sections={inspectorSections}
                      label={t("shell.inspector")}
                    />
                  }
                  className="inspector-split"
                />
              ) : (
                <>
                  <main className={"shell-center layout-" + layoutMode}>
                    <ErrorBoundary key={location.pathname} scope="route">
                      <Outlet />
                    </ErrorBoundary>
                  </main>
                  {inspectorOpen && !wideShell && !immersive ? (
                    <button
                      type="button"
                      className="inspector-backdrop"
                      aria-label={t("shell.closeInspector")}
                      onClick={() => setInspectorOpen(false)}
                    />
                  ) : null}
                  <InspectorPanel
                    open={inspectorOpen && !immersive}
                    sections={inspectorSections}
                    label={t("shell.inspector")}
                  />
                </>
              )}
            </div>
            {!immersive ? (
              <div
                className="activity-dock-bar"
                role="region"
                aria-label={t("shell.activityCollapsed")}
              >
                <div className="activity-dock-context">
                  <PanelBottom className="h-3.5 w-3.5" aria-hidden="true" />
                  <span>{t("shell.activityCollapsed")}</span>
                  <span className="activity-dock-hint">{t("shell.activityHint")}</span>
                </div>
                <button
                  type="button"
                  className="activity-dock-toggle"
                  aria-label={t("shell.openActivity")}
                  aria-expanded="false"
                  title={t("shell.openActivity")}
                  onClick={() => setActivityOpen(true)}
                >
                  <span>{t("shell.openActivity")}</span>
                  <ChevronRight className="h-3.5 w-3.5" aria-hidden="true" />
                </button>
              </div>
            ) : null}
          </>
        )}
      </div>
      {searchOpen ? <GlobalSearchPalette onClose={closeSearch} /> : null}
    </div>
  );
}
