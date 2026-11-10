import { afterEach, describe, expect, it, vi } from "vitest";
import {
  AUTH_EXPIRED_EVENT,
  ApiError,
  apiGet,
  apiSend,
  setAuthTokenProvider,
} from "../src/lib/api";

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json; charset=utf-8" },
  });
}

function htmlResponse(status = 200): Response {
  return new Response(
    "<!doctype html><html><body><h1>502 Bad Gateway</h1><p>nginx error page</p></body></html>",
    { status, headers: { "content-type": "text/html; charset=utf-8" } },
  );
}

/** P5-2 基础件包：api 层用例（非 JSON 防御、401 事件化、Bearer 头注入）。 */
describe("lib/api（P5-2）", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    setAuthTokenProvider(() => null);
  });

  it("成功路径解析 JSON 并返回数据", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse({ items: [1, 2] })),
    );
    await expect(apiGet<{ items: number[] }>("/api/stats")).resolves.toEqual({ items: [1, 2] });
  });

  it("拒收 text/html 响应：HTML 错误页不得伪装成成功", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => htmlResponse(200)),
    );
    const error: unknown = await apiGet("/api/stats").catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).status).toBe(200);
    expect((error as ApiError).message).toContain("非 JSON");
  });

  it("401 触发身份事件并抛出带状态的 ApiError（不跳转）", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse({ message: "缺少或无效的访问令牌" }, 401)),
    );
    const seen: string[] = [];
    const handler = (event: Event) => seen.push(event.type);
    window.addEventListener(AUTH_EXPIRED_EVENT, handler);
    try {
      const error: unknown = await apiGet("/api/stats").catch((e: unknown) => e);
      expect(error).toBeInstanceOf(ApiError);
      expect((error as ApiError).status).toBe(401);
      expect((error as ApiError).message).toBe("缺少或无效的访问令牌");
    } finally {
      window.removeEventListener(AUTH_EXPIRED_EVENT, handler);
    }
    expect(seen).toEqual([AUTH_EXPIRED_EVENT]);
  });

  it("无 message 字段的错误响应回退到带状态码的默认文案", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse({ error: "boom" }, 500)),
    );
    const error: unknown = await apiGet("/api/stats").catch((e: unknown) => e);
    expect((error as ApiError).message).toBe("请求失败（HTTP 500）");
    expect((error as ApiError).data).toEqual({ error: "boom" });
  });

  it("注入令牌后请求带 Bearer 头", async () => {
    setAuthTokenProvider(() => "dev-token");
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) =>
      jsonResponse({}),
    );
    vi.stubGlobal("fetch", fetchMock);
    await apiGet("/api/tasks");
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const init = fetchMock.mock.calls[0]?.[1];
    const headers = (init?.headers ?? {}) as Record<string, string>;
    expect(headers.Authorization).toBe("Bearer dev-token");
  });

  it("apiSend 走 JSON 请求体", async () => {
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) =>
      jsonResponse({ ok: true }),
    );
    vi.stubGlobal("fetch", fetchMock);
    await apiSend("POST", "/api/echo", { a: 1 });
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit | undefined;
    expect(init?.method).toBe("POST");
    expect(init?.body).toBe(JSON.stringify({ a: 1 }));
    expect(((init?.headers ?? {}) as Record<string, string>)["Content-Type"]).toBe(
      "application/json",
    );
  });
});
