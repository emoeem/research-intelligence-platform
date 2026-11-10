/** 主题注册表：localStorage 持久化 + data-theme 属性切换。
 *  机制沿自参照实现（admin theme 模块）的实现模式（复制改造进本仓库，
 *  不含其 foxgold 色值与退役主题表内容）。首帧防闪烁由 index.html 的内联
 *  脚本完成；本文件在应用启动时按同一映射重设一次，两处映射必须同步改。 */

export const THEME_OPTIONS = [
  { id: "rip-dark", name: "深空" },
  { id: "rip-light", name: "晨白" },
] as const;

export type ThemeId = (typeof THEME_OPTIONS)[number]["id"];

export const DEFAULT_THEME: ThemeId = "rip-dark";

const STORAGE_KEY = "rip_theme";

/** 退役 id 到现存主题的归一化映射。RIP 首版无退役主题，机制先落位；
 *  以后下线主题 id 时在这里补映射，避免老用户闪到错误色板。 */
const LEGACY_THEME_MAP: Record<string, ThemeId> = {
  system: DEFAULT_THEME,
};

function normalizeTheme(value: string | null): ThemeId {
  if (!value) return DEFAULT_THEME;
  const legacy = LEGACY_THEME_MAP[value];
  if (legacy) return legacy;
  return THEME_OPTIONS.some((option) => option.id === value) ? (value as ThemeId) : DEFAULT_THEME;
}

export function getStoredTheme(): ThemeId {
  try {
    return normalizeTheme(localStorage.getItem(STORAGE_KEY));
  } catch {
    return DEFAULT_THEME;
  }
}

export function applyTheme(theme: ThemeId): void {
  const concrete = normalizeTheme(theme);
  document.documentElement.setAttribute("data-theme", concrete);
  try {
    localStorage.setItem(STORAGE_KEY, concrete);
  } catch {
    /* storage 不可用（隐私模式等）：属性已设置，不持久化即可 */
  }
}
