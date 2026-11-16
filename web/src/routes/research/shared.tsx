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
export const TASK_COLUMNS: DataColumn<ResearchTaskRow>[] = [
  {
    id: "state",
    label: "状态",
    width: "w-44",
    render: (row) => (
      <span className={`badge badge-sm ${TASK_STATE_TONES[row.state ?? ""] ?? "badge-ghost"}`}>
        {row.state ?? "-"}
      </span>
    ),
  },
  {
    id: "task_type",
    label: "类型",
    width: "w-20",
    render: (row) => <span className="badge badge-sm badge-outline">{row.task_type ?? "-"}</span>,
  },
  {
    id: "title",
    label: "任务",
    render: (row) => (
      <div className="min-w-0">
        <p className="truncate text-sm font-medium">{row.title ?? "-"}</p>
        {row.target ? <p className="truncate text-[11px] text-muted">{row.target}</p> : null}
      </div>
    ),
  },
  {
    id: "updated_at",
    label: "更新时间",
    width: "w-40",
    render: (row) => <span className="text-xs text-muted">{row.updated_at ?? "-"}</span>,
  },
  {
    id: "task_id",
    label: "任务 ID",
    width: "w-56",
    render: (row) => <span className="font-mono text-[11px] text-muted">{row.task_id}</span>,
  },
];
