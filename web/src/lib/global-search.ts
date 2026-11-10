/**
 * 全局搜索（命令面板）的纯匹配模块。
 *
 * 骨架沿自参照实现 admin/src/global-search.ts（UI 复用盘点 §2.2 解耦切点）：
 * 规范化、匹配、分组、排序保持零 import、零副作用（纯模块保证无 DOM 的
 * 纯 tsc 环境也能编译）；三组问卷业务类型换成 RIP 的通用组结构，组的数据
 * 与展示名由调用方（P5-3 的搜索面板）注入，本模块不感知任何业务路由。
 *
 * 设计取舍：
 *  - 匹配是「全部关键词都命中」（AND）：多词查询天然收窄结果。
 *  - 组内排序：首词前缀命中排前，其余按数据原有顺序（列表按更新时间倒序时，
 *    稳定排序等于「相关性优先、其次最新」）。
 *  - 每组命中数有上限：面板一屏可读，更多结果引导用户精化关键词。
 */

/** 单条可搜索条目：text 为匹配文本（null 表示不可搜索，匹配阶段跳过），
 *  title 缺省时展示 text，跳转路由与徽标由数据源给出。 */
export interface GlobalSearchItem {
  text: string | null;
  title?: string;
  to: string;
  meta?: string;
}

/** 一个组的数据源：组的顺序即结果组间顺序。 */
export interface GlobalSearchGroupSource {
  group: string;
  label: string;
  items: readonly GlobalSearchItem[];
}

export interface GlobalSearchHit {
  group: string;
  /** 回车 / 点击后的跳转路由。 */
  to: string;
  /** 展示标题。 */
  title: string;
  /** 右侧徽标，可缺省。 */
  meta?: string;
}

export interface GlobalSearchGroupResult {
  group: string;
  label: string;
  hits: GlobalSearchHit[];
}

export interface GlobalSearchResult {
  query: string;
  /** 只含有命中的组（无结果的组不渲染），组间顺序 = 数据源顺序。 */
  groups: GlobalSearchGroupResult[];
  /** groups 的扁平串联 = 键盘上下选择的顺序。 */
  hits: GlobalSearchHit[];
}

export const GLOBAL_SEARCH_LIMIT_PER_GROUP = 5;

export interface GlobalSearchOptions {
  /** 每组保留的命中数上限（默认 GLOBAL_SEARCH_LIMIT_PER_GROUP）。 */
  limitPerGroup?: number;
}

/**
 * 查询与索引共用的规范化：小写化、全角 ASCII（U+FF01-FF5E）转半角、
 * 全角空格（U+3000）与连续空白折叠成单个空格。两侧同口径，中文不受影响，
 * 「ＲＩＰ」「rip」「RIP」互相命中。
 */
export function normalizeSearchText(input: string): string {
  return input
    .toLowerCase()
    .replace(/[\uFF01-\uFF5E]/g, (char) => String.fromCharCode(char.charCodeAt(0) - 0xfee0))
    .replace(/\u3000/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

/** 组内排名：0 = 首词前缀命中（排前），1 = 包含命中；null = 未命中。 */
const PREFIX_RANK = 0;
const INCLUDES_RANK = 1;

function matchRank(haystack: string, tokens: readonly string[]): number | null {
  const first = tokens[0];
  if (first === undefined || first === "") return null;
  // 首词决定秩：位置 0 是前缀命中，其余出现位置是包含命中；没出现直接未命中。
  const firstIndex = haystack.indexOf(first);
  if (firstIndex === -1) return null;
  const rank = firstIndex === 0 ? PREFIX_RANK : INCLUDES_RANK;
  for (let i = 1; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (token === undefined) continue;
    if (!haystack.includes(token)) return null;
  }
  return rank;
}

/**
 * 多组匹配 + 排序 + 截断。查询为空（或全空白）返回空结果，空态由 UI 渲染
 * 引导内容，不是匹配逻辑的职责。
 */
export function searchGlobalSearch(
  sources: readonly GlobalSearchGroupSource[],
  rawQuery: string,
  options?: GlobalSearchOptions,
): GlobalSearchResult {
  const query = normalizeSearchText(rawQuery);
  if (query === "") return { query, groups: [], hits: [] };
  const tokens = query.split(" ").filter((token) => token !== "");
  if (tokens.length === 0) return { query, groups: [], hits: [] };
  const limit = options?.limitPerGroup ?? GLOBAL_SEARCH_LIMIT_PER_GROUP;

  const groups: GlobalSearchGroupResult[] = [];
  for (const source of sources) {
    const matched: Array<{ rank: number; index: number; item: GlobalSearchItem }> = [];
    source.items.forEach((item, index) => {
      if (item.text === null) return;
      const rank = matchRank(normalizeSearchText(item.text), tokens);
      if (rank === null) return;
      matched.push({ rank, index, item });
    });
    if (matched.length === 0) continue;
    matched.sort((a, b) => a.rank - b.rank || a.index - b.index);
    groups.push({
      group: source.group,
      label: source.label,
      hits: matched.slice(0, limit).map(({ item }) => ({
        group: source.group,
        to: item.to,
        title: item.title ?? item.text ?? "",
        ...(item.meta === undefined ? {} : { meta: item.meta }),
      })),
    });
  }

  return { query, groups, hits: groups.flatMap((group) => group.hits) };
}
