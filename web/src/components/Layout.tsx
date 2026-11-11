/** 壳层 Layout：图标轨 + 工作区 tab 条 + 移动抽屉 + 按路由 re-key 的 ErrorBoundary。
 *  模式沿自参照实现 admin/src/components/Layout.tsx（UI 复用盘点 §2.1），按 RIP
 *  信息架构重写：工作区数据来自 src/workspaces.ts（同一份数据源），无 Telegram
 *  会话/横幅/角色过滤（RIP 会话模型在 P5-4 之后的批次按需接入）。 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, Outlet, useLocation } from "react-router";
import {
  CirclePlay,
  Cpu,
  Database,
  FolderKanban,
  Menu,
  Moon,
  Radar,
  Search,
  ServerCog,
  Puzzle,
  Sun,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import { ErrorBoundary } from "./ErrorBoundary";
import { GlobalSearchPalette } from "./GlobalSearchPalette";
import { applyTheme, getStoredTheme, THEME_OPTIONS, type ThemeId } from "../theme";
import { WORKSPACES, tabNumberOf, type WorkspaceIconKey } from "../workspaces";

/** workspaces.ts 的图标键 → lucide 组件。Record 全键覆盖：加新键漏映射会直接 tsc 报错。 */
const ICONS: Record<WorkspaceIconKey, LucideIcon> = {
  radar: Radar,
  cpu: Cpu,
  folderKanban: FolderKanban,
  database: Database,
  wrench: Wrench,
  circlePlay: CirclePlay,
  serverCog: ServerCog,
  puzzle: Puzzle,
};

/**
 * 沉浸态正则（预留）：命中时整宽渲染，图标轨/页头/tab 条隐藏（顶栏返回退出）。
 * RIP 尚无沉浸页面（工具详情编辑、项目工作台等在后续批次落地时把路径模式
 * 加进这里即可，Layout 结构不用动）；当前正则刻意不命中任何既有路由。
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

export function Layout() {
  const location = useLocation();
  const [drawer, setDrawer] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [theme, setTheme] = useState<ThemeId>(() => getStoredTheme());
  const closeSearch = useCallback(() => setSearchOpen(false), []);

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

  // 路由变化自动收起抽屉。
  useEffect(() => {
    setDrawer(false);
  }, [location.pathname]);

  const activeWorkspaceId = workspaceIdOf(location.pathname);
  const activeWorkspace =
    WORKSPACES.find((workspace) => workspace.id === activeWorkspaceId) ?? null;
  const immersive = IMMERSIVE_PATH.test(location.pathname.replace(/\/+$/, ""));

  const title = useMemo(() => {
    if (activeWorkspace) {
      const normalized = location.pathname.replace(/\/+$/, "") || "/";
      const tab = activeWorkspace.tabs.find((item) => isActivePath(item.to, normalized));
      return tab ? tab.label : activeWorkspace.label;
    }
    return "总览";
  }, [activeWorkspace, location.pathname]);

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
      <main className="shell-main">
        <header className="shell-head">
          <button
            type="button"
            className="menu-btn"
            aria-label="打开菜单"
            aria-expanded={drawer}
            onClick={() => setDrawer(true)}
          >
            <Menu className="h-4 w-4" aria-hidden="true" />
          </button>
          <h1 className="shell-head-title">{title}</h1>
          <div className="shell-head-actions">
            <button type="button" className="search-trigger" onClick={() => setSearchOpen(true)}>
              <Search className="h-4 w-4" aria-hidden="true" />
              <span>搜索</span>
              <kbd className="search-kbd">⌘K</kbd>
            </button>
          </div>
        </header>
        {/* 工作区子导航：进入工作区后主区域顶部的横向 tab 条（总览与沉浸态不出现）。
            tab 带稳定编号（workspaces.ts 的 order 坐标），与功能地图互相指认。 */}
        {activeWorkspace && !immersive ? (
          <nav aria-label={`${activeWorkspace.label}工作区`} className="ws-tabs">
            {activeWorkspace.tabs.map((item) => {
              const Icon = ICONS[item.icon];
              const active = isActivePath(item.to, location.pathname.replace(/\/+$/, "") || "/");
              return (
                <Link
                  key={item.to}
                  to={item.to}
                  aria-current={active ? "page" : undefined}
                  className={`ws-tab${active ? " is-active" : ""}`}
                  title={item.hint}
                >
                  <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                  <span className="ws-tab-no">{tabNumberOf(activeWorkspace, item)}</span>
                  {item.label}
                </Link>
              );
            })}
          </nav>
        ) : null}
        {/* 按路由 re-key：单页渲染错误不炸整个壳，导航离开即复位（参照实现教训）。 */}
        <ErrorBoundary key={location.pathname} scope="route">
          <Outlet />
        </ErrorBoundary>
      </main>
      {searchOpen ? <GlobalSearchPalette onClose={closeSearch} /> : null}
    </div>
  );
}
