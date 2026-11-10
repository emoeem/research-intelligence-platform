/** API 客户端基础层：ApiError + 非 JSON 响应防御 + Bearer 头注入。
 *  模式与事故注释沿自参照实现 admin/src/api.ts（docs/phase0 的 UI 复用盘点
 *  §3.5 解耦切点）；认证模型换成 RIP 的 Bearer Token（apps/server rp_server.py
 *  的 Authorization 校验），无任何 Telegram 依赖，认证头按令牌提供者注入。 */

export class ApiError extends Error {
  readonly status: number;
  readonly data: Record<string, unknown> | null;

  constructor(status: number, message: string, data: Record<string, unknown> | null = null) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.data = data;
  }
}

/** 401（令牌缺失/过期）事件：参照实现在 401 时直接跳登录页，RIP 登录面尚未
 *  建立（P5-4 起定会话模型），改为派发事件，由壳层/页面决定去留。 */
export const AUTH_EXPIRED_EVENT = "rip:auth-expired";

type TokenProvider = () => string | null;

let tokenProvider: TokenProvider = () => null;

/** 注入 Bearer 令牌来源（P5-4 接会话面时调用；默认无令牌）。 */
export function setAuthTokenProvider(provider: TokenProvider): void {
  tokenProvider = provider;
}

function authHeaders(): Record<string, string> {
  let token: string | null = null;
  try {
    token = tokenProvider();
  } catch {
    token = null;
  }
  return token ? { Authorization: `Bearer ${token}` } : {};
}

/**
 * 解析 JSON body，body 不是 JSON 时拒绝凭空造一个空对象。
 *
 * 对 200 的 HTML 错误页返回 {} 曾伪装成成功响应，页面随即在
 * `data.items.length` 上抛 TypeError，根 ErrorBoundary 把整个应用换成
 * 「页面出错了」。抛出带状态码的 ApiError 把失败留在能解释它的页面里。
 */
async function parseResponse(response: Response): Promise<Record<string, unknown>> {
  const text = await response.text();
  if (!text.trim()) return {};
  try {
    return JSON.parse(text) as Record<string, unknown>;
  } catch {
    throw new ApiError(response.status, `服务端返回了非 JSON 响应（HTTP ${response.status}）`);
  }
}

async function throwApiError(response: Response): Promise<never> {
  if (response.status === 401) {
    window.dispatchEvent(new Event(AUTH_EXPIRED_EVENT));
  }
  const data = await parseResponse(response);
  // 代理/边缘层的错误页没有 message 字段：只说「请求失败」没人能行动，带上状态码。
  throw new ApiError(
    response.status,
    (data.message as string) || `请求失败（HTTP ${response.status}）`,
    data,
  );
}

async function request<T>(
  path: string,
  init: RequestInit & { headers?: Record<string, string> },
): Promise<T> {
  const response = await fetch(path, { ...init, headers: { ...authHeaders(), ...init.headers } });
  if (!response.ok) await throwApiError(response);
  return (await parseResponse(response)) as T;
}

export async function apiGet<T>(path: string, options: { signal?: AbortSignal } = {}): Promise<T> {
  return request<T>(path, { signal: options.signal });
}

export async function apiSend<T>(
  method: "POST" | "PATCH" | "PUT" | "DELETE",
  path: string,
  body?: Record<string, unknown>,
  options: { signal?: AbortSignal } = {},
): Promise<T> {
  return request<T>(path, {
    method,
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    headers: { "Content-Type": "application/json" },
    signal: options.signal,
  });
}
