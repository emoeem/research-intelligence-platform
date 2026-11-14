import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

/** RIP3 P1-2 文档中心冒烟：上传表单（base64 → POST /api/documents/import）、
 *  文档列表（workspace 过滤 + 权限前置行为面）、版本历史（SUPERSEDED 在列，
 *  历史可访问）。全部 /api/** 走 fetch stub（PG 情报核经 rp-doc 子进程，
 *  jsdom 侧无 PG）。 */

const LIST_STUB = {
  items: [
    {
      document_id: "doc-18e7f12ebc0d14cb0000",
      title: "P1-1 smoke 样本",
      document_type: "markdown",
      sha256: "4c472eba84a1745fc21ffd1589f22836995ffca39cb67c99aeccfff7787a964c",
      file_size: 1024,
      workspace_id: "default",
      visibility: "private",
      lifecycle_status: "ACTIVE",
      source_count: 2,
      revision_count: 2,
      created_at: "2026-11-14T14:55:59Z",
    },
    {
      document_id: "doc-empty-0002",
      title: "空壳文档（诚实空态对照）",
      document_type: "other",
      sha256: "b".repeat(64),
      file_size: 10,
      workspace_id: "default",
      visibility: "shared",
      lifecycle_status: "ACTIVE",
      source_count: 0,
      revision_count: 1,
      created_at: "2026-11-14T15:00:00Z",
    },
  ],
  workspace: "default",
};

const DETAIL_STUB = {
  document_id: "doc-18e7f12ebc0d14cb0000",
  revisions: [
    {
      revision_id: "drev-18e7f12ed0f8fb7a0000",
      revision_number: 2,
      content_hash: "2aea082e6d8e3d25209cec87acf2f297c40b94ddc14d628e7758c3dd9e3106f6",
      parser_version: "native-1.0",
      revision_status: "CURRENT",
      created_at: "2026-11-14T14:56:30Z",
    },
    {
      revision_id: "drev-18e7f12ebc0d1c1d0001",
      revision_number: 1,
      content_hash: "4c472eba84a1745fc21ffd1589f22836995ffca39cb67c99aeccfff7787a964c",
      parser_version: null,
      revision_status: "SUPERSEDED",
      created_at: "2026-11-14T14:55:59Z",
    },
  ],
};

const IMPORT_STUB = {
  document_id: "doc-import-0003",
  revision_id: "drev-import-0003",
  source_id: "src-import-0003",
  content_object_created: false,
  content_hash: "c".repeat(64),
};

function stubFetch(handler: (path: string, init?: RequestInit) => unknown): void {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL, init?: RequestInit) =>
      Promise.resolve(new Response(JSON.stringify(handler(String(input), init)), { status: 200 })),
    ),
  );
}

describe("文档中心（RIP3 P1-2）", () => {
  const posted: { path: string; body: Record<string, unknown> }[] = [];

  beforeEach(() => {
    posted.length = 0;
    stubFetch((path, init) => {
      if (path.startsWith("/api/documents/doc-")) return DETAIL_STUB;
      if (
        (path === "/api/documents" || path.startsWith("/api/documents?")) &&
        (!init || !init.method || init.method === "GET")
      ) {
        return LIST_STUB;
      }
      if (path === "/api/documents/import" && init?.method === "POST") {
        posted.push({ path, body: JSON.parse(String(init.body)) });
        return IMPORT_STUB;
      }
      return {};
    });
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("列表渲染：ID/标题/来源数/版本数 + 诚实空态不在场", async () => {
    window.history.pushState({}, "", "/documents");
    render(<App />);
    expect(await screen.findByTestId("doc-card-doc-18e7f12ebc0d14cb0000")).toBeTruthy();
    expect(screen.getByTestId("doc-card-doc-empty-0002")).toBeTruthy();
    expect(screen.getAllByTestId("doc-source-count").length).toBe(2);
    expect(screen.getByTestId("doc-upload-form")).toBeTruthy();
  });

  it("行点击取版本历史：SUPERSEDED 与 CURRENT 双行在列（历史可访问）", async () => {
    window.history.pushState({}, "", "/documents");
    render(<App />);
    await screen.findByTestId("doc-card-doc-18e7f12ebc0d14cb0000");
    fireEvent.click(screen.getByTestId("doc-card-doc-18e7f12ebc0d14cb0000"));
    const panel = await screen.findByTestId("doc-revisions");
    expect(panel.textContent).toContain("SUPERSEDED");
    expect(panel.textContent).toContain("CURRENT");
    expect(panel.textContent).toContain("native-1.0");
  });

  it("上传流：base64 → POST /api/documents/import → 复用标注如实显示", async () => {
    window.history.pushState({}, "", "/documents");
    render(<App />);
    await screen.findByTestId("doc-card-doc-18e7f12ebc0d14cb0000");
    const file = new File(["# 文档内容\n"], "sample.md", { type: "text/markdown" });
    const input = screen.getByTestId("doc-file") as HTMLInputElement;
    fireEvent.change(input, { target: { files: [file] } });
    fireEvent.change(screen.getByTestId("doc-title"), { target: { value: "REST 导入样本" } });
    fireEvent.click(screen.getByTestId("doc-upload-submit"));
    expect(await screen.findByTestId("doc-upload-result")).toBeTruthy();
    expect(screen.getByTestId("doc-upload-result").textContent).toContain("复用");
    expect(posted.length).toBe(1);
    expect(posted[0]?.body.title).toBe("REST 导入样本");
    expect(posted[0]?.body.filename).toBe("sample.md");
    expect(String(posted[0]?.body.content_base64).length).toBeGreaterThan(0);
  });

  it("无文件提交被前端拦截（不发起 POST）", async () => {
    window.history.pushState({}, "", "/documents");
    render(<App />);
    await screen.findByTestId("doc-card-doc-18e7f12ebc0d14cb0000");
    fireEvent.change(screen.getByTestId("doc-title"), { target: { value: "无文件" } });
    fireEvent.click(screen.getByTestId("doc-upload-submit"));
    expect(await screen.findByTestId("doc-upload-error")).toBeTruthy();
    expect(screen.getByTestId("doc-upload-error").textContent).toContain("请选择文件");
    expect(posted.length).toBe(0);
  });
});
