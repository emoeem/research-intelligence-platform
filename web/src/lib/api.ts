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

// ---------------------------------------------------------------------------
// P6-8 插件面（/api/plugins，apps/server rp_server.py；写端点经 rp-plugin
// 子进程走 plugin-runtime 状态机与 audit-engine，服务端零直连 SQL 写）。
// ---------------------------------------------------------------------------

/** L1/L2 共享的插件目录行（/api/plugins）。 */
export interface PluginRow {
  plugin_id: string;
  version: string;
  name: string;
  kind: string;
  description: string;
  license: string;
  trust_level: string;
  state: string;
  update_pending: boolean;
  is_active: boolean;
  governance?: string | null;
  source_type?: string | null;
  source_ref?: string | null;
  installed_dir?: string | null;
  source_uninstalled?: boolean;
  capabilities_count?: number;
  updated_at?: string | null;
  /** SK-2：快照 Commit（009 pinned_json）。 */
  pinned?: { kind?: string; value?: string } | null;
  /** SK-2：来源知识统计（knowledge_source_entry 审核与类型计数）。 */
  knowledge_stats?: {
    review?: Record<string, number>;
    entry_kinds?: Record<string, number>;
    last_seen_at?: string | null;
  } | null;
}

export interface PluginList {
  available: boolean;
  reason?: string;
  message?: string;
  db?: string;
  total: number;
  items: PluginRow[];
  categories?: {
    installed: string[];
    updatable: string[];
    disabled: string[];
    uninstalled: string[];
  };
  audit_summary?: { action: string; n: number }[];
}

/** capability 清单行（L2）。 */
export interface PluginCapabilityRow {
  capability_id: string;
  capability_type: string;
  entry: string;
  summary: string;
  runtime?: string | null;
  requires: string[];
}

/** 兼容声明（L2）。 */
export interface PluginCompatibility {
  rip_core?: string;
  manifest_schema?: string[];
  runtimes?: string[];
  worker_protocol?: string;
}

/** L3：完整 Manifest + docs 正文。 */
/** SK-2：per-kind 许可与持久化策略声明（manifest provides.kinds）。 */
export interface PluginKindPolicy {
  license_policy?: string;
  source_path?: string;
  parser?: string;
}

export interface PluginDetail {
  level: 1 | 2 | 3;
  plugin_id: string;
  version: string;
  name: string;
  kind: string;
  description: string;
  license: string;
  trust_level: string;
  state: string;
  update_pending: boolean;
  is_active: boolean;
  governance?: string | null;
  source_type?: string | null;
  source_ref?: string | null;
  lock_sha256?: string | null;
  installed_dir?: string | null;
  source_uninstalled?: boolean;
  updated_at?: string | null;
  /** SK-2：快照 Commit 与来源主页。 */
  pinned?: { kind?: string; value?: string } | null;
  homepage?: string | null;
  /** SK-2：knowledge_source capability 的内容类型与 per-kind 策略声明。 */
  knowledge_provides?: {
    corpus_id?: string | null;
    content_kinds?: string[];
    kinds?: Record<string, PluginKindPolicy>;
  } | null;
  knowledge_stats?: {
    review?: Record<string, number>;
    entry_kinds?: Record<string, number>;
    last_seen_at?: string | null;
  } | null;
  context?: { summary?: string; docs?: string } | null;
  capabilities?: PluginCapabilityRow[];
  compatibility?: PluginCompatibility;
  dependencies?: { id: string; requirement: string }[];
  manifest?: Record<string, unknown>;
  docs?: { path?: string; body?: string | null } | null;
  audit?: {
    event_id: number;
    ts: string;
    actor: string;
    action: string;
    entity_id: string;
    detail?: string;
  }[];
}

/** 插件写操作结果（rp-plugin JSON stdout 包装）。 */
export interface PluginWriteResult {
  ok: boolean;
  action: string;
  plugin_id: string;
  result: Record<string, unknown>;
}

export async function fetchPlugins(options: { signal?: AbortSignal } = {}): Promise<PluginList> {
  return apiGet<PluginList>("/api/plugins", options);
}

export async function fetchPluginDetail(
  pluginId: string,
  level: 1 | 2 | 3 = 2,
  options: { signal?: AbortSignal } = {},
): Promise<PluginDetail> {
  return apiGet<PluginDetail>(
    `/api/plugins/${encodeURIComponent(pluginId)}?level=${level}`,
    options,
  );
}

export async function pluginAction(
  action: "enable" | "disable" | "uninstall" | "ingest",
  pluginId: string,
): Promise<PluginWriteResult> {
  return apiSend<PluginWriteResult>(
    "POST",
    `/api/plugins/${encodeURIComponent(pluginId)}/${action}`,
  );
}

export async function pluginInstall(
  manifestPath: string,
  trust: string,
): Promise<PluginWriteResult> {
  return apiSend<PluginWriteResult>("POST", "/api/plugins/install", {
    manifest_path: manifestPath,
    trust,
  });
}

export async function pluginUpgrade(
  pluginId: string,
  manifestPath: string,
): Promise<PluginWriteResult> {
  return apiSend<PluginWriteResult>(
    "POST",
    `/api/plugins/${encodeURIComponent(pluginId)}/upgrade`,
    {
      manifest_path: manifestPath,
    },
  );
}

// ---------------------------------------------------------------------------
// SK-2 知识来源条目（/api/knowledge_sources，knowledge_source_entry 011）。
// ---------------------------------------------------------------------------

/** 知识来源条目行（catalog_entry / document_markdown / skill_candidate）。 */
export interface KnowledgeSourceEntry {
  plugin_id: string;
  plugin_version: string;
  corpus_id: string;
  entry_kind: string;
  item_key: string;
  title: string;
  description?: string | null;
  category_path: string[];
  canonical_url?: string | null;
  license_status: string;
  rights_policy: string;
  review_status: string;
  source_commit?: string | null;
  source_url?: string | null;
  source_location?: Record<string, unknown>;
  body_bytes?: number;
  first_seen_at?: string | null;
  last_seen_at?: string | null;
}

export interface KnowledgeSourcePlugin {
  plugin_id: string;
  name: string;
  version: string;
  state: string;
  license: string;
}

export interface KnowledgeSourceList {
  available: boolean;
  reason?: string;
  message?: string;
  total: number;
  returned?: number;
  limit?: number;
  filters?: {
    plugin_id?: string | null;
    entry_kind?: string | null;
    review_status?: string | null;
    q?: string | null;
  };
  plugins: KnowledgeSourcePlugin[];
  summary?: { plugin_id: string; entry_kind: string; review_status: string; n: number }[];
  items: KnowledgeSourceEntry[];
}

export async function fetchKnowledgeSources(
  params: {
    plugin_id?: string;
    entry_kind?: string;
    review_status?: string;
    q?: string;
    limit?: number;
  } = {},
  options: { signal?: AbortSignal } = {},
): Promise<KnowledgeSourceList> {
  const search = new URLSearchParams();
  if (params.plugin_id) search.set("plugin_id", params.plugin_id);
  if (params.entry_kind) search.set("entry_kind", params.entry_kind);
  if (params.review_status) search.set("review_status", params.review_status);
  if (params.q) search.set("q", params.q);
  if (params.limit) search.set("limit", String(params.limit));
  const qs = search.toString();
  return apiGet<KnowledgeSourceList>(`/api/knowledge_sources${qs ? `?${qs}` : ""}`, options);
}
