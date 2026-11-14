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

/** P10-2：四档信任级策略条目（plugin-registry::trust_policy 单一来源，
 * /api/plugins/trust-policy）。 */
export interface TrustPolicyTier {
  trust_level: string;
  install_confirmation: string;
  confirmation_label: string;
  extra_steps: number;
  requires_provenance: boolean;
  requires_typed_id: boolean;
  required_confirm_fields: string[];
  auto_update_prompt: boolean;
}

export interface TrustPolicy {
  schema_version: string;
  source: string;
  tiers: TrustPolicyTier[];
  notes?: string;
}

export async function fetchTrustPolicy(
  options: { signal?: AbortSignal } = {},
): Promise<TrustPolicy> {
  return apiGet<TrustPolicy>("/api/plugins/trust-policy", options);
}

/** P10-2：按信任级取策略档；策略不可得时 fail-closed 走最强档
 *（remote：显式二次确认），绝不静默降档。 */
export function resolveTrustTier(
  policy: TrustPolicy | null | undefined,
  trustLevel: string,
): TrustPolicyTier {
  const fallback: TrustPolicyTier = {
    trust_level: trustLevel,
    install_confirmation: "double_confirm_typed",
    confirmation_label: "未知来源：输入插件 id 二次确认",
    extra_steps: 1,
    requires_provenance: false,
    requires_typed_id: true,
    required_confirm_fields: ["confirm", "confirm_id"],
    auto_update_prompt: false,
  };
  if (!policy?.tiers?.length) return fallback;
  return policy.tiers.find((tier) => tier.trust_level === trustLevel) ?? fallback;
}

/** P10-2：Manifest 只读预检（安装确认对话框预览面）。 */
export interface ManifestInspectResult {
  id: string;
  version: string;
  kind: string;
  license: string;
  homepage?: string | null;
  source: Record<string, unknown>;
  source_default_trust: string;
  install_confirmation: string;
  confirmation_label: string;
  auto_update_prompt: boolean;
}

export async function pluginManifestInspect(
  manifestPath: string,
): Promise<{ ok: boolean; result: ManifestInspectResult }> {
  return apiSend("POST", "/api/plugins/manifest-inspect", {
    manifest_path: manifestPath,
  });
}

export interface PluginConfirmFields {
  /** P10-2 确认分级：community/local 需要 true；official 可省略。 */
  confirm?: boolean;
  /** remote 显式二次确认：必须与 Manifest id 一致（服务端 CLI 预检对账）。 */
  confirm_id?: string;
}

export async function pluginInstall(
  manifestPath: string,
  trust: string,
  confirm?: PluginConfirmFields,
): Promise<PluginWriteResult> {
  return apiSend<PluginWriteResult>("POST", "/api/plugins/install", {
    manifest_path: manifestPath,
    trust,
    ...confirm,
  });
}

export async function pluginUpgrade(
  pluginId: string,
  manifestPath: string,
  confirm?: PluginConfirmFields,
): Promise<PluginWriteResult> {
  return apiSend<PluginWriteResult>(
    "POST",
    `/api/plugins/${encodeURIComponent(pluginId)}/upgrade`,
    {
      manifest_path: manifestPath,
      ...confirm,
    },
  );
}

/** P10-2 D6：回滚窗口清理（§5.3 第 5 步，confirm 后清理旧版本目录）。 */
export async function pluginCleanupRollback(pluginId: string): Promise<PluginWriteResult> {
  return apiSend<PluginWriteResult>(
    "POST",
    `/api/plugins/${encodeURIComponent(pluginId)}/cleanup-rollback`,
    { confirm: true },
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

// ---------------------------------------------------------------------------
// P7-7 Agent Studio 读面（/api/agents 与 /api/agent-runs，apps/server
// rp_server.py handle_agents/handle_agent_runs；主库加 aux 双 schema 归并，
// 只读 SELECT 零 SQL 写；详情口径见各 Payload 注释）。
// ---------------------------------------------------------------------------

/** 能力绑定引用（agent_definition.capability_refs_json 逐项）。 */
export interface AgentCapabilityRef {
  tool_id: string;
  version?: string | null;
}

/** 能力绑定对账行（详情侧：引用 tool 注册面 lifecycle）。 */
export interface AgentToolBinding {
  tool_id: string;
  version?: string | null;
  registered?: boolean | null;
  name?: string | null;
  lifecycle?: string | null;
  governance?: string | null;
}

/** Agent 定义行（006 agent_definition，一个版本一行）。 */
export interface AgentDefinitionRow {
  agent_id: string;
  version: string;
  name: string;
  context_policy?: unknown;
  capability_refs?: AgentCapabilityRef[];
  created_at?: string | null;
  updated_at?: string | null;
  /** 来源库（主库/aux 归并标注）。 */
  _src?: string;
}

/** /api/agents 列表（?id= 缺省）。 */
export interface AgentList {
  available: boolean;
  message?: string;
  total: number;
  agents: number;
  returned: number;
  limit?: number;
  source?: string;
  items: AgentDefinitionRow[];
  runs_total?: number;
  runs_by_state?: Record<string, number>;
}

/** AgentRun 摘要行（006 agent_run；trace_id 列按库水位条件出现）。 */
export interface AgentRunRow {
  run_id: string;
  task_id?: string | null;
  agent_id: string;
  agent_version: string;
  state: string;
  memory_refs?: string[] | null;
  trace_id?: string | null;
  steps_count?: number;
  created_at?: string | null;
  started_at?: string | null;
  finished_at?: string | null;
  updated_at?: string | null;
  _src?: string;
}

/** /api/agents?id=<agent_id> 详情：全版本定义 + 能力绑定对账 + run 摘要。 */
export interface AgentDetail {
  available: boolean;
  agent_id: string;
  definitions: AgentDefinitionRow[];
  tool_bindings: AgentToolBinding[];
  runs: AgentRunRow[];
  runs_total: number;
  runs_by_state?: Record<string, number>;
}

/** 步骤轨迹行（014 agent_run_step 投影 + input_json 解析）。 */
export interface AgentRunStepRow {
  seq: number;
  round: number;
  tool_id: string;
  tool_version?: string | null;
  tool_run_id?: string | null;
  status: string;
  artifact_id?: string | null;
  evidence_id?: string | null;
  content_hash?: string | null;
  summary?: string | null;
  error?: string | null;
  elapsed_ms?: number;
  input?: unknown;
  created_at?: string | null;
  updated_at?: string | null;
  _src?: string;
}

/** Task Memory 行（013 agent_task_memory）。 */
export interface AgentTaskMemoryRow {
  memory_id: string;
  run_id: string;
  task_id?: string | null;
  round: number;
  content?: unknown;
  summary?: string | null;
  lifecycle: string;
  created_at?: string | null;
  _src?: string;
}

/** Evidence Memory 行（013 agent_evidence_memory，指针台账）。 */
export interface AgentEvidenceMemoryRow {
  memory_id: string;
  run_id: string;
  evidence_id: string;
  artifact_id?: string | null;
  content_hash?: string | null;
  storage_ref?: string | null;
  lifecycle: string;
  created_at?: string | null;
  _src?: string;
}

/** Research Memory 行（013 agent_research_memory，Context Memory 雏形）。 */
export interface AgentResearchMemoryRow {
  memory_id: string;
  run_id: string;
  task_id?: string | null;
  agent_id?: string | null;
  title: string;
  summary?: string | null;
  refs?: unknown;
  lifecycle: string;
  created_at?: string | null;
  _src?: string;
}

/** /api/agent-runs?id= 详情：run 全行 + 步骤轨迹 + 三记忆视图 + 六 ID。 */
export interface AgentRunDetail {
  available: boolean;
  source?: string;
  run: AgentRunRow;
  steps: AgentRunStepRow[];
  memories: {
    task: AgentTaskMemoryRow[];
    evidence: AgentEvidenceMemoryRow[];
    research: AgentResearchMemoryRow[];
  };
  control?: {
    goal_text?: string | null;
    retry_of_run_id?: string | null;
    pause_requested_at?: string | null;
    cancel_requested_at?: string | null;
  } | null;
  correlation: {
    trace_id?: string | null;
    task_id?: string | null;
    agent_run_id: string;
    tool_run_ids: string[];
    artifact_ids: string[];
    evidence_ids: string[];
  };
  counts: {
    steps: number;
    task_memory: number;
    evidence_memory: number;
    research_memory: number;
  };
}

/** /api/agent-runs 列表。 */
export interface AgentRunList {
  available: boolean;
  message?: string;
  source?: string;
  total: number;
  returned: number;
  limit?: number;
  filters?: { state?: string | null; agent_id?: string | null };
  items: AgentRunRow[];
}

/** /api/agents 路径组装（列表；detail 由 fetchAgents(id) 走 ?id=）。 */
export function agentsPath(params: { limit?: number } = {}): string {
  const search = new URLSearchParams();
  if (params.limit) search.set("limit", String(params.limit));
  const qs = search.toString();
  return `/api/agents${qs ? `?${qs}` : ""}`;
}

/** /api/agent-runs 路径组装（列表过滤：state/agent_id/limit）。 */
export function agentRunsPath(
  params: { state?: string; agent_id?: string; limit?: number } = {},
): string {
  const search = new URLSearchParams();
  if (params.state) search.set("state", params.state);
  if (params.agent_id) search.set("agent_id", params.agent_id);
  if (params.limit) search.set("limit", String(params.limit));
  const qs = search.toString();
  return `/api/agent-runs${qs ? `?${qs}` : ""}`;
}

export async function fetchAgents(
  params: { limit?: number },
  options?: { signal?: AbortSignal },
): Promise<AgentList>;
export async function fetchAgents(
  id: string,
  options?: { signal?: AbortSignal },
): Promise<AgentDetail>;
export async function fetchAgents(
  idOrParams: string | { limit?: number },
  options: { signal?: AbortSignal } = {},
): Promise<AgentList | AgentDetail> {
  if (typeof idOrParams === "string") {
    return apiGet<AgentDetail>(`/api/agents?id=${encodeURIComponent(idOrParams)}`, options);
  }
  return apiGet<AgentList>(agentsPath(idOrParams), options);
}

export async function fetchAgentRuns(
  params: { state?: string; agent_id?: string; limit?: number } = {},
  options: { signal?: AbortSignal } = {},
): Promise<AgentRunList> {
  return apiGet<AgentRunList>(agentRunsPath(params), options);
}

export async function fetchAgentRunDetail(
  runId: string,
  options: { signal?: AbortSignal } = {},
): Promise<AgentRunDetail> {
  return apiGet<AgentRunDetail>(`/api/agent-runs?id=${encodeURIComponent(runId)}`, options);
}

// ---------------------------------------------------------------------------
// P8-4 Workflow Studio 画布面（/api/workflows 与 /api/workflow-runs 的画布
// 消费侧）：定义列表与逐版 graph（历史回看）、run 列表与详情（节点状态/
// 控制行/关联六 ID）、写面 run/pause/resume/retry（P8-3）与 put（画布保存，
// 版本存储侧递增）。写面一律经服务端 rp-workflow 子进程，前端零 SQL。
// ---------------------------------------------------------------------------

/** definition-v1 图节点（id/type/params，词表 = workflow-engine DSL 八类）。 */
export interface WorkflowGraphNode {
  id: string;
  type: string;
  params?: Record<string, unknown>;
}

/** definition-v1 图边（from/to，kind 缺省 seq，branch 为 condition/parallel
 *  出边词形，when 为声明式条件载荷）。 */
export interface WorkflowGraphEdge {
  from: string;
  to: string;
  kind?: string;
  when?: unknown;
}

/** definition-v1 graph（节点/边两向量）。 */
export interface WorkflowGraph {
  nodes: WorkflowGraphNode[];
  edges: WorkflowGraphEdge[];
}

/** workflow_definition 行（016；?id= 详情带逐版 graph）。 */
export interface WorkflowDefinitionRow {
  workflow_id: string;
  version: number;
  name: string;
  description?: string | null;
  compat?: string;
  node_count?: number;
  edge_count?: number;
  input_schema?: unknown;
  source?: string | null;
  created_at?: string | null;
  graph?: WorkflowGraph | null;
  _src?: string;
}

/** /api/workflows 列表（?id= 缺省）。 */
export interface WorkflowList {
  available: boolean;
  message?: string;
  source?: string;
  total_workflows: number;
  total_versions: number;
  items: WorkflowDefinitionRow[];
}

/** workflow_run 摘要行（017 + 018 六 ID 关联列，缺列库条件出现）。 */
export interface WorkflowRunRow {
  run_id: string;
  workflow_id: string;
  workflow_version: number;
  state: string;
  trace_id?: string | null;
  task_id?: string | null;
  agent_run_id?: string | null;
  tool_run_id?: string | null;
  artifact_id?: string | null;
  evidence_id?: string | null;
  created_at?: string | null;
  started_at?: string | null;
  finished_at?: string | null;
  _src?: string;
}

/** workflow_run_node 节点状态行（五态 + attempt + 实体链接列）。 */
export interface WorkflowRunNodeRow {
  node_id: string;
  node_kind: string;
  state: string;
  attempt: number;
  output?: unknown;
  error?: unknown;
  started_at?: string | null;
  finished_at?: string | null;
  tool_run_id?: string | null;
  agent_run_id?: string | null;
}

/** 018 workflow_run_control 控制行投影（awaiting_approval 判定服务端算好）。 */
export interface WorkflowRunControl {
  pause_requested_at?: string | null;
  cancel_requested_at?: string | null;
  approval_node_id?: string | null;
  approval_request?: unknown;
  approval?: unknown;
  approval_updated_at?: string | null;
  awaiting_approval?: boolean;
  _src?: string;
}

/** /api/workflow-runs?id= 详情（run 头行 + 节点行 + 控制行 + 六 ID 对账）。 */
export interface WorkflowRunDetail {
  available: boolean;
  source?: string;
  query_id: string;
  run: WorkflowRunRow & { input?: unknown; output?: unknown; error?: unknown };
  nodes: WorkflowRunNodeRow[];
  checkpoint_seq?: number | null;
  control?: WorkflowRunControl | null;
  correlation: {
    trace_id?: string | null;
    task_id?: string | null;
    agent_run_id?: string | null;
    tool_run_id?: string | null;
    artifact_id?: string | null;
    evidence_id?: string | null;
  };
  trace_query?: string | null;
}

/** /api/workflow-runs 列表。 */
export interface WorkflowRunList {
  available: boolean;
  message?: string;
  source?: string;
  total: number;
  returned: number;
  limit?: number;
  items: WorkflowRunRow[];
}

export async function fetchWorkflows(
  id: string,
  options: { signal?: AbortSignal } = {},
): Promise<WorkflowList> {
  const suffix = id ? `?id=${encodeURIComponent(id)}` : "?limit=100";
  return apiGet<WorkflowList>(`/api/workflows${suffix}`, options);
}

export async function fetchWorkflowRuns(
  id: string,
  options: { signal?: AbortSignal } = {},
): Promise<WorkflowRunList> {
  const suffix = id ? `?id=${encodeURIComponent(id)}` : "?limit=50";
  return apiGet<WorkflowRunList>(`/api/workflow-runs${suffix}`, options);
}

export async function fetchWorkflowRunDetail(
  runId: string,
  options: { signal?: AbortSignal } = {},
): Promise<WorkflowRunDetail> {
  return apiGet<WorkflowRunDetail>(`/api/workflow-runs?id=${encodeURIComponent(runId)}`, options);
}

/** run 受理（P8-3 集合路径）：受理即返回（异步承载），状态经 GET 轮询。 */
export interface WorkflowRunAccepted {
  result: string;
  action: string;
  run_id: string;
  pid?: number;
  log?: string;
  status_query?: string;
}

export function workflowRun(
  workflowId: string,
  options: { input?: Record<string, unknown>; version?: number } = {},
): Promise<WorkflowRunAccepted> {
  return apiSend<WorkflowRunAccepted>("POST", "/api/workflow-runs", {
    workflow_id: workflowId,
    input: options.input ?? {},
    ...(options.version ? { version: options.version } : {}),
  });
}

export type WorkflowRunAction = "pause" | "resume" | "retry";

/** run 控制动作（REST 动态路径）：pause 同步（请求位写入即回）；
 *  resume 带 inject（原样并入恢复态，批准参数按 human 节点 params.key 包键）；
 *  retry 带 node_id（新 run 承载，只重跑目标失败节点）。 */
export function workflowRunAction(
  action: WorkflowRunAction,
  runId: string,
  body: { inject?: Record<string, unknown>; node_id?: string } = {},
): Promise<Record<string, unknown>> {
  return apiSend<Record<string, unknown>>(
    "POST",
    `/api/workflow-runs/${encodeURIComponent(runId)}/${action}`,
    body,
  );
}

/** 画布保存（P8-4 put）：definition-v1 JSON 经 rp-workflow put 四道门后
 *  追加一版，版本由存储侧分配（现有最大版本 + 1）。 */
export interface WorkflowPutResult {
  result: string;
  workflow_id: string;
  version: number;
  nodes?: number;
  edges?: number;
  status_query?: string;
}

export function workflowPut(definition: Record<string, unknown>): Promise<WorkflowPutResult> {
  return apiSend<WorkflowPutResult>("POST", "/api/workflows", { definition });
}

// ---------------------------------------------------------------------------
// P9-4 MCP 目录同步状态（/api/mcp-catalog，apps/server rp_server.py；Tool
// Center MCP 状态卡数据面）：目录条数 + 读时对账结果。跨通道 wire 级对账
// 由 crates/rp-mcp-parity CI 门禁常驻承载，本端点是读面快照。
// ---------------------------------------------------------------------------

/** 对账漂移条目（注册面投影完整性检查失败项）。 */
export interface McpCatalogDrift {
  check: string;
  tool_id?: string | null;
  detail: string;
}

/** MCP 目录同步状态载荷（读时对账，不落状态行）。 */
export interface McpCatalogStatus {
  available?: boolean;
  source?: string;
  catalog_count: number;
  capability_count: number;
  capability_tools: string[];
  reconciliation: {
    ok: boolean;
    reconciled_at: string;
    drift: McpCatalogDrift[];
    scope: string;
  };
  note?: string;
  /** 数据面不可读时的诚实空态说明（available=false）。 */
  reason?: string;
  message?: string;
}

/** 取 MCP 目录同步状态（Tool Center MCP 状态卡）。 */
export async function fetchMcpCatalog(
  options: { signal?: AbortSignal } = {},
): Promise<McpCatalogStatus> {
  return apiGet<McpCatalogStatus>("/api/mcp-catalog", options);
}
