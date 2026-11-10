import { describe, expect, it } from "vitest";
import {
  GLOBAL_SEARCH_LIMIT_PER_GROUP,
  normalizeSearchText,
  searchGlobalSearch,
  type GlobalSearchGroupSource,
} from "../src/lib/global-search";

const pages: GlobalSearchGroupSource = {
  group: "pages",
  label: "导航",
  items: [
    { text: "总览 Overview", to: "/" },
    { text: "工具中心 Tool Center", to: "/tools" },
    // 「包含命中」排在数据顺序更靠前的位置，用来验证前缀命中仍排前面。
    { text: "核心 RIP 平台能力", to: "/somewhere" },
    { text: "RIP 工作台", to: "/elsewhere" },
    { text: "运行记录 Runs", to: "/runs" },
  ],
};

const tools: GlobalSearchGroupSource = {
  group: "tools",
  label: "工具",
  items: [
    { text: "rip.http.inspect", to: "/tools/rip.http.inspect", meta: "只读" },
    { text: "rip.search.knowledge", to: "/tools/rip.search.knowledge" },
    { text: null, to: "/tools/hidden" },
  ],
};

/** P5-2 基础件包：全局搜索纯匹配模块用例（全角折叠、AND 多词、排序、截断）。 */
describe("lib/global-search（P5-2）", () => {
  it("规范化：全角 ASCII 转半角、全角空格与连续空白折叠", () => {
    expect(normalizeSearchText("ＲＩＰ　ＷＯＲＫＢＥＮＣＨ")).toBe("rip workbench");
    expect(normalizeSearchText("  工具　　 中心  ")).toBe("工具 中心");
  });

  it("全角查询命中半角索引", () => {
    const result = searchGlobalSearch([pages], "ＲＩＰ");
    expect(result.hits.map((hit) => hit.title)).toContain("RIP 工作台");
  });

  it("AND 多词匹配：所有关键词都命中才算命中", () => {
    const result = searchGlobalSearch([pages], "工具 中心");
    // 「工具中心 Tool Center」两词齐中；「运行记录 Runs」「总览」不含「工具」。
    expect(result.hits).toHaveLength(1);
    expect(result.hits[0]?.title).toBe("工具中心 Tool Center");
  });

  it("组内排序：首词前缀命中排在包含命中前面（与数据顺序无关）", () => {
    const result = searchGlobalSearch([pages], "rip");
    expect(result.hits.map((hit) => hit.title)).toEqual(["RIP 工作台", "核心 RIP 平台能力"]);
  });

  it("组间顺序随数据源，扁平 hits 为组序串联", () => {
    const result = searchGlobalSearch([pages, tools], "rip");
    expect(result.groups.map((group) => group.group)).toEqual(["pages", "tools"]);
    expect(result.groups[0]?.label).toBe("导航");
    expect(result.hits.slice(0, 2).every((hit) => hit.group === "pages")).toBe(true);
    expect(result.hits.at(-1)?.title).toBe("rip.search.knowledge");
  });

  it("text 为 null 的条目跳过匹配", () => {
    const result = searchGlobalSearch([tools], "hidden");
    expect(result.hits).toHaveLength(0);
  });

  it("每组命中数默认截断为 5，可用 options 覆盖", () => {
    const bulky: GlobalSearchGroupSource = {
      group: "bulk",
      label: "批量",
      items: Array.from({ length: 8 }, (_, index) => ({
        text: `条目 ${index}`,
        to: `/bulk/${index}`,
      })),
    };
    const byWord = searchGlobalSearch([bulky], "条目");
    expect(byWord.hits).toHaveLength(GLOBAL_SEARCH_LIMIT_PER_GROUP);
    const narrowed = searchGlobalSearch([bulky], "条目", { limitPerGroup: 2 });
    expect(narrowed.hits).toHaveLength(2);
  });

  it("空查询与全空白查询返回空结果", () => {
    expect(searchGlobalSearch([pages], "")).toEqual({ query: "", groups: [], hits: [] });
    expect(searchGlobalSearch([pages], "　 ")).toEqual({ query: "", groups: [], hits: [] });
  });
});
