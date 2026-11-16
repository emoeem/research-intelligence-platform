/** Activity「任务队列」页签的验证计划实时面（P5-6）：运行中（executing）与
 *  暂停（paused）的验证计划轮询呈现（5s 跟随后端收敛；控制动作的执行进程
 *  在项间检查位响应，本组件只读不渲染按钮）。
 *
 *  数据源 /api/research/plans（rp-verify plan-list 子进程读面）；完成标准
 *  「前端和后端任务状态一致」：终态（completed/cancelled）计划即时从本视图
 *  消失，历史在 /research/plans 页可查。 */

import { Link } from "react-router";
import { useApi } from "../lib/hooks";
import { PLAN_STATUS_TONES, type ResearchPlanList } from "../routes/research/shared";

const ACTIVE_PLAN_STATES = new Set(["executing", "paused"]);

export function ActivityPlansLive() {
  const list = useApi<ResearchPlanList>("/api/research/plans?limit=100", {
    refetchIntervalMs: 5000,
  });
  const active = (list.data?.items ?? []).filter((plan) => ACTIVE_PLAN_STATES.has(plan.status));

  if (list.error !== null) {
    return (
      <p className="activity-note">
        验证计划状态不可用（{list.error.message}）；完整列表在
        <Link to="/research/plans">验证计划页</Link>。
      </p>
    );
  }
  if (active.length === 0) {
    return (
      <p className="activity-note">
        当前无运行中或暂停的验证计划；验证计划页与执行派发见
        <Link to="/research/plans">验证计划</Link>（rp-verify CLI 面）。
      </p>
    );
  }
  return (
    <ul className="space-y-2" aria-label="运行中的验证计划">
      {active.map((plan) => (
        <li key={plan.plan_id} className="flex items-center gap-2 text-xs">
          <span className={`badge badge-sm ${PLAN_STATUS_TONES[plan.status] ?? "badge-ghost"}`}>
            {plan.status}
          </span>
          <Link
            to="/research/plans"
            className="min-w-0 flex-1 truncate text-base-content/80 hover:underline"
          >
            {plan.title ?? plan.plan_id}
          </Link>
          <span className="shrink-0 font-mono text-[10px] text-muted">{plan.plan_id}</span>
        </li>
      ))}
    </ul>
  );
}
