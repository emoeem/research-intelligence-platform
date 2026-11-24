import { translateUi } from "../../i18n";
/** 验证计划列表与控制面（P5-6，Phase 5 §4.5.B/§5.9）：/api/research/plans
 *  真实列表（rp-verify plan-list 子进程读面）+ 计划状态过滤 + 五态结果计数
 *  展示 + 暂停/恢复/取消控制（POST /api/research/plans/<id>/<action> 经
 *  rp-verify 子进程，服务端零直连 SQL 写）。
 *
 *  状态语义（020 词表 / PlanStatus domain）：executing 计划在项间检查位响应
 *  暂停/取消（跨进程经 SQLite 状态行传播）；paused 计划经恢复回到 ready 后
 *  由 rp-verify execute-plan 续跑（幂等只派发未判定项）。本页只渲染可用的
 *  控制按钮，不渲染编造的操作；执行派发入口（execute-plan）是 CLI 面，
 *  不在本页提供。 */

import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useApi } from "../../lib/hooks";
import { apiSend, ApiError } from "../../lib/api";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../../components/ui";
import { FilterSearch, FilterSelect } from "../../components/data";
import { formatDateTime } from "../../lib/format";
import {
  PLAN_STATUSES,
  PLAN_STATUS_TONES,
  RESULT_STATUSES,
  RESULT_STATUS_TONES,
  type ResearchPlanList,
  type ResearchPlanRow,
} from "./shared";

type ControlAction = "pause" | "resume" | "cancel";

/** 状态 → 可用控制动作（§5.9：暂停/恢复/取消；终态不可迁出，不渲染按钮）。 */
function controlActions(status: string): ControlAction[] {
  switch (status) {
    case "executing":
      return ["pause", "cancel"];
    case "paused":
      return ["resume", "cancel"];
    default:
      return [];
  }
}

const ACTION_LABELS: Record<ControlAction, string> = {
  pause: "暂停",
  resume: "恢复",
  cancel: "取消",
};

export function ResearchPlansPage() {
  const [statusFilter, setStatusFilter] = useState("all");
  const [textFilter, setTextFilter] = useState("");
  const [busyPlan, setBusyPlan] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const queryClient = useQueryClient();
  // 活跃计划（executing/paused）轮询 5s：控制动作落在别的进程（CLI 子进程
  // 项间检查位），列表状态需要自动跟随后端收敛。
  const list = useApi<ResearchPlanList>("/api/research/plans?limit=100");

  const plans = list.data?.items ?? [];
  const filtered = useMemo(() => {
    const needle = textFilter.trim().toLowerCase();
    return plans.filter((plan) => {
      if (statusFilter !== "all" && plan.status !== statusFilter) return false;
      if (!needle) return true;
      return [plan.title ?? "", plan.plan_id, plan.task_ref ?? ""]
        .join("\n")
        .toLowerCase()
        .includes(needle);
    });
  }, [plans, statusFilter, textFilter]);

  async function runControl(plan: ResearchPlanRow, action: ControlAction) {
    setBusyPlan(plan.plan_id);
    setActionError(null);
    try {
      await apiSend<{ plan_id: string; status: string }>(
        "POST",
        `/api/research/plans/${encodeURIComponent(plan.plan_id)}/${action}`,
      );
      await queryClient.invalidateQueries({ queryKey: ["api", "/api/research/plans?limit=100"] });
    } catch (sendError) {
      if (sendError instanceof ApiError) {
        setActionError(
          `${translateUi(ACTION_LABELS[action])}${translateUi("失败")}（${plan.plan_id}）：${sendError.message}`,
        );
      } else {
        setActionError(
          `${translateUi(ACTION_LABELS[action])}${translateUi("失败")}（${plan.plan_id}）：${translateUi("未知错误")}`,
        );
      }
    } finally {
      setBusyPlan(null);
    }
  }

  if (list.loading) {
    return (
      <div>
        <PageHeader title={translateUi("验证计划")} />
        <SkeletonPanel lines={6} />
      </div>
    );
  }
  if (list.error !== null) {
    return (
      <div>
        <PageHeader title={translateUi("验证计划")} />
        <ErrorPanel error={list.error} onRetry={list.retry} />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <PageHeader title={`${translateUi("验证计划")}（${plans.length}）`} />
      <div className="flex flex-wrap items-center gap-3">
        <FilterSearch
          label={translateUi("搜索")}
          value={textFilter}
          onChange={setTextFilter}
          placeholder={translateUi("按标题/ID/任务引用过滤")}
        />
        <FilterSelect
          label={translateUi("状态")}
          value={statusFilter}
          onChange={setStatusFilter}
          options={[
            { value: "all", label: translateUi("全部状态") },
            ...PLAN_STATUSES.map((s) => ({ value: s, label: s })),
          ]}
        />
        {actionError ? (
          <p role="alert" className="text-xs text-error">
            {actionError}
          </p>
        ) : null}
      </div>
      {filtered.length === 0 ? (
        <EmptyPanel
          text={translateUi(
            "没有匹配的验证计划；计划经 rp-verify plan-create / plan-build 创建（CLI 面）。",
          )}
        />
      ) : (
        <div className="space-y-3">
          {filtered.map((plan) => {
            const actions = controlActions(plan.status);
            const counts = plan.result_counts ?? {};
            return (
              <div
                key={plan.plan_id}
                className="rounded-surface border border-edge bg-base-100 p-4"
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span
                        className={`badge badge-sm ${PLAN_STATUS_TONES[plan.status] ?? "badge-ghost"}`}
                      >
                        {plan.status}
                      </span>
                      <p className="truncate text-sm font-medium">{plan.title ?? "(无标题)"}</p>
                    </div>
                    <p className="mt-1 font-mono text-[11px] text-muted">
                      {plan.plan_id}
                      {plan.task_ref ? ` · ${plan.task_ref}` : ""}
                    </p>
                  </div>
                  <div className="flex items-center gap-2">
                    {actions.map((action) => (
                      <button
                        key={action}
                        type="button"
                        className={`btn btn-xs ${action === "cancel" ? "btn-outline btn-error" : "btn-outline"}`}
                        disabled={busyPlan === plan.plan_id}
                        onClick={() => void runControl(plan, action)}
                      >
                        {translateUi(ACTION_LABELS[action])}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="mt-3 flex flex-wrap items-center gap-4 text-xs text-muted">
                  <span>
                    {translateUi("计划项")} {plan.item_count}
                  </span>
                  <span className="flex flex-wrap items-center gap-1">
                    {RESULT_STATUSES.map((status) => {
                      const count = counts[status];
                      return count ? (
                        <span
                          key={status}
                          className={`badge badge-sm badge-outline ${RESULT_STATUS_TONES[status] ?? ""}`}
                        >
                          {status} {count}
                        </span>
                      ) : null;
                    })}
                    {Object.values(counts).reduce((sum, n) => sum + n, 0) === 0 ? (
                      <span>{translateUi("五态未判定")}</span>
                    ) : null}
                  </span>
                  <span>
                    {translateUi("更新")} {formatDateTime(plan.updated_at)}
                  </span>
                </div>
              </div>
            );
          })}
        </div>
      )}
      <p className="text-[11px] text-muted">
        {translateUi(
          "派发与执行细节：rp-verify execute-plan（CLI 面）逐项 Provider 执行 + 五态归一化回写；取消/暂停在执行进程的项间检查位真实生效，已完成计划项的结论与 Evidence 不受影响。",
        )}
      </p>
    </div>
  );
}
