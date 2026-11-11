import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

/** P5-6 Knowledge 页冒烟：列表渲染、q= 检索（防抖后带关键词请求）、无命中
 *  真实空态（服务端 message）、Loading/Error 三态、详情侧栏（knowledge_id
 *  全字段拉取 + 来源/元数据/正文摘要呈现）。 */

const LIST_STUB = {
  available: true,
  source: "main",
  total: 2,
  returned: 2,
  items: [
    {
      knowledge_id: "k-redirect",
      lifecycle: "VERIFIED",
      title: "站群/落地页开放重定向专项测试法",
      component: "landing",
      severity: "high",
      verified: 1,
      hit_count: 7,
      updated_at: "2026-11-09T10:00:00Z",
    },
    {
      knowledge_id: "k-hex2bin",
      lifecycle: "CANDIDATE",
      title: "hex2bin 解码技巧在参数矩阵中的应用",
      component: null,
      severity: null,
      verified: 0,
      hit_count: 0,
      updated_at: "2026-11-08T09:00:00Z",
    },
  ],
};

const DETAIL_STUB = {
  available: true,
  source: "main",
  item: {
    knowledge_id: "k-redirect",
    record_kind: "own",
    lifecycle: "VERIFIED",
    title: "站群/落地页开放重定向专项测试法",
    component: "landing",
    cve: null,
    severity: "high",
    language: null,
    evidence_level: "K3",
    verified: 1,
    verified_note: "3 站实测成立",
    hit_count: 7,
    description: "覆盖 h2 陷阱与路径面。",
    usage: "示例用法",
    content: "正文第一段\n第二段：参数矩阵。",
    source: "web",
    source_url: "https://example.org/note",
    tags: ["open-redirect", "landing"],
    updated_at: "2026-11-09T10:00:00Z",
  },
};

function okFetch(): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      let body: unknown = {};
      if (url.includes("knowledge_id=")) body = DETAIL_STUB;
      else if (url.includes("q="))
        body = { ...LIST_STUB, total: 1, returned: 1, items: [LIST_STUB.items[0]] };
      else if (
        url.startsWith("http://localhost:5173/api/knowledge") ||
        url.includes("/api/knowledge")
      )
        body = LIST_STUB;
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json; charset=utf-8" },
      });
    }),
  );
}

beforeEach(() => {
  okFetch();
  window.history.pushState({}, "", "/knowledge");
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("Knowledge 页（P5-6）", () => {
  it("渲染列表：标题、生命周期徽标、验证状态与总数", async () => {
    render(<App />);
    expect(await screen.findByText("站群/落地页开放重定向专项测试法")).toBeTruthy();
    expect(screen.getByText("hex2bin 解码技巧在参数矩阵中的应用")).toBeTruthy();
    expect(screen.getByText("共 2 条（显示 2）")).toBeTruthy();
    expect(screen.getAllByText("VERIFIED").length).toBeGreaterThan(0);
    expect(screen.getAllByText("已验证").length).toBeGreaterThan(0);
    expect(screen.getAllByText("未验证").length).toBeGreaterThan(0);
  });

  it("检索走 q= 参数且防抖生效：输入后 300ms 才发带关键词的请求", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.includes("q=")
        ? { ...LIST_STUB, total: 1, returned: 1, items: [LIST_STUB.items[0]] }
        : LIST_STUB;
      return new Response(JSON.stringify(body), { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await screen.findByText("共 2 条（显示 2）");
    const callsBefore = fetchMock.mock.calls.filter((call) =>
      String(call[0]).includes("q="),
    ).length;
    const input = screen.getByRole("textbox", { name: "检索知识条目" });
    fireEvent.change(input, { target: { value: "重定向" } });
    // 防抖窗口内（<300ms）：还没有带 q= 的请求
    await new Promise((resolve) => setTimeout(resolve, 120));
    const callsDuringDebounce = fetchMock.mock.calls.filter((call) =>
      String(call[0]).includes("q="),
    ).length;
    expect(callsDuringDebounce).toBe(callsBefore);
    // 防抖结束：请求带 q=（encodeURIComponent 后的「重定向」）且列表更新
    await waitFor(
      () => {
        const withQ = fetchMock.mock.calls.filter((call) => {
          const url = String(call[0]);
          return url.includes("q=") && decodeURIComponent(url).includes("重定向");
        });
        if (withQ.length === 0) throw new Error("带 q= 的检索请求未发出");
      },
      { timeout: 1500 },
    );
    expect(await screen.findByText("检索「重定向」：共 1 条（显示 1）")).toBeTruthy();
  });

  it("无命中显示服务端真实空态文案", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        const body = url.includes("q=")
          ? {
              available: true,
              total: 0,
              returned: 0,
              items: [],
              message: "没有匹配的知识条目，调整关键词或生命周期后重试",
            }
          : LIST_STUB;
        return new Response(JSON.stringify(body), { status: 200 });
      }),
    );
    render(<App />);
    await screen.findByText("共 2 条（显示 2）");
    fireEvent.change(screen.getByRole("textbox", { name: "检索知识条目" }), {
      target: { value: "zzz-不存在" },
    });
    expect(await screen.findByText("没有匹配的知识条目，调整关键词或生命周期后重试")).toBeTruthy();
  });

  it("Loading 态渲染骨架，Error 态渲染 ErrorPanel 且可重试", async () => {
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      if (String(input).includes("/api/knowledge") && !String(input).includes("q=")) {
        return new Promise<Response>(() => {});
      }
      return Promise.resolve(new Response(JSON.stringify(LIST_STUB), { status: 200 }));
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    expect(document.querySelector(".skeleton")).toBeTruthy();
    // 切 Error
    cleanup();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify({ message: "内部错误" }), { status: 500 })),
    );
    render(<App />);
    expect(await screen.findByText("加载失败")).toBeTruthy();
    expect(screen.getByRole("button", { name: /重试/ })).toBeTruthy();
  });

  it("点击列表项：详情侧栏按 knowledge_id= 拉取并呈现来源/元数据/正文摘要", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.includes("knowledge_id=")
        ? DETAIL_STUB
        : url.includes("q=")
          ? { ...LIST_STUB, total: 1, returned: 1, items: [LIST_STUB.items[0]] }
          : LIST_STUB;
      return new Response(JSON.stringify(body), { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    fireEvent.click(await screen.findByText("站群/落地页开放重定向专项测试法"));
    const aside = await screen.findByRole("complementary", { name: "条目详情" });
    await waitFor(() => {
      const called = fetchMock.mock.calls.some((call) =>
        String(call[0]).includes("knowledge_id=k-redirect"),
      );
      if (!called) throw new Error("详情请求未发出");
    });
    // 来源
    expect(aside.textContent).toContain("来源");
    expect(aside.textContent).toContain("https://example.org/note");
    // 元数据
    expect(aside.textContent).toContain("landing");
    expect(aside.textContent).toContain("K3");
    expect(aside.textContent).toContain("3 站实测成立");
    expect(aside.textContent).toContain("open-redirect");
    // 正文摘要
    expect(aside.textContent).toContain("正文摘要");
    expect(aside.textContent).toContain("第二段：参数矩阵。");
  });

  it("深链契约：?knowledge_id= 直接打开详情侧栏并同步选中态", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.includes("knowledge_id=")
        ? DETAIL_STUB
        : url.includes("q=")
          ? { ...LIST_STUB, total: 1, returned: 1, items: [LIST_STUB.items[0]] }
          : LIST_STUB;
      return new Response(JSON.stringify(body), { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    // ⌘K 面板知识命中的落点形态：/knowledge?knowledge_id=k-redirect
    window.history.pushState({}, "", "/knowledge?knowledge_id=k-redirect");
    render(<App />);
    const aside = await screen.findByRole("complementary", { name: "条目详情" });
    await waitFor(() => {
      const called = fetchMock.mock.calls.some((call) =>
        String(call[0]).includes("knowledge_id=k-redirect"),
      );
      if (!called) throw new Error("深链详情请求未发出");
    });
    // 详情侧栏呈现深链条目
    expect(aside.textContent).toContain("站群/落地页开放重定向专项测试法");
    // 等列表渲染完成后再断言选中态与 URL 同步（高亮属性在行按钮上）
    await screen.findByText("共 2 条（显示 2）");
    const activeRow = document.querySelector('button[aria-current="true"]');
    expect(activeRow?.textContent).toContain("站群/落地页开放重定向专项测试法");
    // 点击另一行：URL 参数同步更新（replace 语义）
    fireEvent.click(screen.getByText("hex2bin 解码技巧在参数矩阵中的应用"));
    await waitFor(() => {
      if (!window.location.search.includes("knowledge_id=k-hex2bin")) {
        throw new Error("URL 未随选中同步");
      }
    });
    expect(window.location.search).toContain("knowledge_id=k-hex2bin");
  });
});
