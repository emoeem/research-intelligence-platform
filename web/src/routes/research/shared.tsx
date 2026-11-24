/** Research Workspace 共享类型与常量（P5-1）：/api/tasks 与 /api/runs 既有
 *  响应的行类型、状态徽标 tone 映射与 RTIS 任务状态词表。
 *
 *  状态词表来源：rip-data/dev/webtest-platform.db research_task.state 实测值
 *  （CREATED/AUTHORIZATION_REVIEW/ENVIRONMENT_PROFILING/LOCAL_KNOWLEDGE_RETRIEVAL/
 *  COMPLETED 等 RTIS 源值直通，服务端不归一化）。未知状态走 badge-ghost 原文
 *  展示，不猜测语义。
 *
 *  五态（CONFIRMED/NOT_REPRODUCED/NOT_TESTED/INCONCLUSIVE/FALSE_POSITIVE）
 *  属验证结果域（P5-7 交付），与研究任务状态（本表）是两个维度，不混用。 */

import type { DataColumn } from "../../components/data";
import { translateUi } from "../../i18n";

/** /api/tasks 行（research_task ETL 镜像，只读）。 */
export interface ResearchTaskRow {
  task_id: string;
  title: string;
  objective?: string | null;
  target?: string | null;
  task_type?: string | null;
  state?: string | null;
  status_history?: string | null;
  source_file?: string | null;
  source_system?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
  schema_version?: number | null;
}

export interface ResearchTaskList {
  total?: number;
  by_state?: Record<string, number>;
  by_type?: Record<string, number>;
  returned?: number;
  limit?: number;
  message?: string;
  items?: ResearchTaskRow[];
}

/** 研究任务状态词表（实测值 + 观察面兜底）。 */
export const RESEARCH_TASK_STATES = [
  "CREATED",
  "AUTHORIZATION_REVIEW",
  "ENVIRONMENT_PROFILING",
  "LOCAL_KNOWLEDGE_RETRIEVAL",
  "WORKFLOW_PLANNING",
  "EXECUTING",
  "AWAITING_REVIEW",
  "COMPLETED",
] as const;

/** 研究任务状态 → 徽标 tone（daisyUI badge 修饰类）。 */
export const TASK_STATE_TONES: Record<string, string> = {
  CREATED: "badge-ghost",
  AUTHORIZATION_REVIEW: "badge-warning",
  ENVIRONMENT_PROFILING: "badge-info",
  LOCAL_KNOWLEDGE_RETRIEVAL: "badge-info",
  WORKFLOW_PLANNING: "badge-info",
  EXECUTING: "badge-primary",
  AWAITING_REVIEW: "badge-warning",
  COMPLETED: "badge-success",
  FAILED: "badge-error",
  CANCELLED: "badge-error",
};

/** /api/runs 列表行（tool_run；本工作区只读展示）。 */
export interface ResearchRunRow {
  run_id: string;
  tool_id: string;
  tool_version?: string | null;
  task_id?: string | null;
  trace_id?: string | null;
  status?: string | null;
  error_category?: string | null;
  duration_ms?: number | null;
  created_at?: string | null;
}

export interface ResearchRunList {
  available?: boolean;
  source?: string;
  total?: number;
  returned?: number;
  reason?: string;
  message?: string;
  items?: ResearchRunRow[];
}

/** Run 状态 tone（与 RunsPage 口径一致）。 */
export const RUN_STATUS_TONES: Record<string, string> = {
  QUEUED: "badge-ghost",
  WAITING_FOR_RUNNER: "badge-ghost",
  PREPARING: "badge-info",
  RUNNING: "badge-info",
  SUCCEEDED: "badge-success",
  FAILED: "badge-error",
  TIMED_OUT: "badge-error",
  CANCELLED: "badge-warning",
};

/** 解析 status_history JSON（RTIS 源数组：{at, state, reason?, skipped?}）。 */
export function parseStatusHistory(raw: string | null | undefined): StatusHistoryEntry[] {
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (entry): entry is StatusHistoryEntry =>
        typeof entry === "object" &&
        entry !== null &&
        typeof (entry as Record<string, unknown>).state === "string",
    );
  } catch {
    return [];
  }
}

export interface StatusHistoryEntry {
  at?: string;
  state: string;
  reason?: string;
  skipped?: unknown[];
}

/** 任务表格列定义（Tasks 列表与 Overview 最近任务共用；行点击由 DataTable
 *  的 onRowActivate 承载，列配置不含导航语义）。 */
export function getTaskColumns(): DataColumn<ResearchTaskRow>[] {
  return [
    {
      id: "state",
      label: translateUi("状态"),
      width: "w-44",
      render: (row) => (
        <span className={`badge badge-sm ${TASK_STATE_TONES[row.state ?? ""] ?? "badge-ghost"}`}>
          {row.state ?? "-"}
        </span>
      ),
    },
    {
      id: "task_type",
      label: translateUi("类型"),
      width: "w-20",
      render: (row) => <span className="badge badge-sm badge-outline">{row.task_type ?? "-"}</span>,
    },
    {
      id: "title",
      label: translateUi("任务"),
      render: (row) => (
        <div className="min-w-0">
          <p className="truncate text-sm font-medium">{row.title ?? "-"}</p>
          {row.target ? <p className="truncate text-[11px] text-muted">{row.target}</p> : null}
        </div>
      ),
    },
    {
      id: "updated_at",
      label: translateUi("更新时间"),
      width: "w-40",
      render: (row) => <span className="text-xs text-muted">{row.updated_at ?? "-"}</span>,
    },
    {
      id: "task_id",
      label: translateUi("任务 ID"),
      width: "w-56",
      render: (row) => <span className="font-mono text-[11px] text-muted">{row.task_id}</span>,
    },
  ];
}

// ---------------------------------------------------------------------------
// 验证计划面（P5-6）：/api/research/plans（rp-verify plan-list 子进程读面）
// 与控制端点（POST pause/resume/cancel → rp-verify 子进程）。计划状态是
// 编排状态机词表（020 迁移 CHECK），五态 result_counts 是研究结论维度。
// ---------------------------------------------------------------------------

/** /api/research/plans 行（PlanSummary）。 */
export interface ResearchPlanRow {
  plan_id: string;
  task_ref?: string | null;
  status: string;
  title?: string | null;
  item_count: number;
  /** 五态计数键值（CONFIRMED/NOT_REPRODUCED/NOT_TESTED/INCONCLUSIVE/
   *  FALSE_POSITIVE → 计数；未判定项不在键内）。 */
  result_counts?: Record<string, number> | null;
  created_at: string;
  updated_at: string;
}

export interface ResearchPlanList {
  available?: boolean;
  source?: string;
  limit?: number;
  items?: ResearchPlanRow[];
}

/** 计划状态词表（020 迁移 CHECK；与 PlanStatus domain 词表一致）。 */
export const PLAN_STATUSES = [
  "draft",
  "ready",
  "executing",
  "paused",
  "completed",
  "cancelled",
] as const;

/** 计划状态 → 徽标 tone。 */
export const PLAN_STATUS_TONES: Record<string, string> = {
  draft: "badge-ghost",
  ready: "badge-info",
  executing: "badge-primary",
  paused: "badge-warning",
  completed: "badge-success",
  cancelled: "badge-error",
};

/** 五态研究结论 → 徽标 tone（P5-7 词表；未知走 ghost）。 */
export const RESULT_STATUS_TONES: Record<string, string> = {
  CONFIRMED: "badge-error",
  NOT_REPRODUCED: "badge-success",
  NOT_TESTED: "badge-ghost",
  INCONCLUSIVE: "badge-warning",
  FALSE_POSITIVE: "badge-outline",
};

/** 五态展示序（§5.7 定义序）。 */
export const RESULT_STATUSES = [
  "CONFIRMED",
  "NOT_REPRODUCED",
  "NOT_TESTED",
  "INCONCLUSIVE",
  "FALSE_POSITIVE",
] as const;

// ---------------------------------------------------------------------------
// 证据档与覆盖矩阵面（P5-9b）：/api/research/evidence（rp-verify
// evidence-list/evidence-get 子进程读面）与 coverage-show 矩阵。
// payload_json 是采集时点原始快照（append-only 不可变，021 语义）。
// ---------------------------------------------------------------------------

/** /api/research/evidence 行（EvidenceRecord）。 */
export interface ResearchEvidenceRow {
  evidence_id: string;
  item_id: string;
  plan_id: string;
  tool_run_id?: string | null;
  trace_id?: string | null;
  /** 词表 provider_result / normalized_result / log_excerpt（021 CHECK）。 */
  evidence_type: string;
  /** payload 紧凑序列化 SHA256 hex（重复观察识别键）。 */
  content_hash: string;
  payload: unknown;
  created_at: string;
}

export interface ResearchEvidenceList {
  plan_id?: string;
  item_id?: string;
  available?: boolean;
  source?: string;
  items?: ResearchEvidenceRow[];
}

/** 证据类型 → 徽标 tone（词表三值，未知走 ghost）。 */
export const EVIDENCE_TYPE_TONES: Record<string, string> = {
  provider_result: "badge-info",
  normalized_result: "badge-success",
  log_excerpt: "badge-warning",
};

/** 七维覆盖维词表（domain CoverageDimension::ALL 同序；展示用中文标签）。 */
export const COVERAGE_DIMENSIONS = [
  "assets",
  "api_endpoints",
  "identity_permissions",
  "input_points",
  "business_flows",
  "file_processing",
  "data_flows",
] as const;

export function getCoverageDimensionLabels(): Record<string, string> {
  return {
    assets: translateUi("资产"),
    api_endpoints: translateUi("API 与端点"),
    identity_permissions: translateUi("身份与权限"),
    input_points: translateUi("输入点"),
    business_flows: translateUi("业务流"),
    file_processing: translateUi("文件处理"),
    data_flows: translateUi("数据流"),
  };
}

/** coverage-show 单维覆盖格（P5-8 coverage.rs CoverageCell 直通）。 */
export interface CoverageCell {
  dimension: string;
  item_count: number;
  confirmed: number;
  not_reproduced: number;
  inconclusive: number;
  not_tested: number;
  false_positive: number;
}

/** coverage-show 响应（cells 七维 + blind_spots 维词列表）。 */
export interface CoverageMatrixResponse {
  plan_id?: string;
  cells?: CoverageCell[];
  blind_spots?: string[];
}
