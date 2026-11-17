/** 覆盖矩阵页（P5-9b，Phase 5 §4.8 / §7 P5-8 完成标准）：/api/research/plans
 *  选计划 → coverage-show 数据（rp-verify 子进程）渲染七维五态分计数矩阵 +
 *  盲区列表。未测试项与失败项分开显示（五态分计数逐列独立，不用百分比
 *  掩盖缺口）；盲区 = 零项（uncovered）或有项无结论（unjudged），两形态
 *  分开提示。§4.8 行级资产明细（点开单元格显示 Task/Item/Evidence）随
 *  数据面补齐批次，本批按维度聚合呈现，如实登记。 */

import { useState } from "react";
import { useApi } from "../../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../../components/ui";
import {
  COVERAGE_DIMENSIONS,
  COVERAGE_DIMENSION_LABELS,
  type CoverageMatrixResponse,
  type ResearchPlanList,
} from "./shared";

/** 五态列定义（序沿 §5.7 定义序；键与 CoverageCell 字段对应）。 */
const FIVE_STATE_COLUMNS = [
  { key: "confirmed", label: "CONFIRMED", tone: "text-error" },
  { key: "not_reproduced", label: "NOT_REPRODUCED", tone: "text-success" },
  { key: "inconclusive", label: "INCONCLUSIVE", tone: "text-warning" },
  { key: "not_tested", label: "NOT_TESTED", tone: "text-muted" },
  { key: "false_positive", label: "FALSE_POSITIVE", tone: "text-muted" },
] as const;

type CellKey = (typeof FIVE_STATE_COLUMNS)[number]["key"];

export function ResearchCoveragePage() {
  const plans = useApi<ResearchPlanList>("/api/research/plans?limit=100");
  const [planId, setPlanId] = useState<string>("");

  const enabled = planId !== "";
  const matrix = useApi<CoverageMatrixResponse>(
    enabled ? `/api/research/coverage?plan=${encodeURIComponent(planId)}` : null,
  );

  if (plans.loading) {
    return (
      <div>
        <PageHeader title="覆盖矩阵" />
        <SkeletonPanel lines={6} />
      </div>
    );
  }
  if (plans.error !== null) {
    return (
      <div>
        <PageHeader title="覆盖矩阵" />
        <ErrorPanel error={plans.error} onRetry={plans.retry} />
      </div>
    );
  }

  const planOptions = (plans.data?.items ?? []).map((plan) => ({
    value: plan.plan_id,
    label: `${plan.title ?? plan.plan_id}（${plan.plan_id}）`,
  }));
  const cells = matrix.data?.cells ?? [];
  const blindSpots = matrix.data?.blind_spots ?? [];
  const cellByDimension = new Map(cells.map((cell) => [cell.dimension, cell]));
  const totalItems = cells.reduce((sum, cell) => sum + cell.item_count, 0);
  const totalJudged = cells.reduce(
    (sum, cell) => sum + (cell.item_count - cell.not_tested),
    0,
  );

  return (
    <div className="space-y-4">
      <PageHeader title="覆盖矩阵" />
      <div className="flex flex-wrap items-center gap-3">
        <label className="form-control">
          <span className="label-text pb-1 text-xs">选择计划</span>
          <select
            data-testid="coverage-plan-select"
            className="select select-bordered select-sm w-72 max-w-full text-xs"
            value={planId}
            onChange={(event) => setPlanId(event.target.value)}
          >
            <option value="">选择计划（coverage-show 计划域）</option>
            {planOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        {enabled && !matrix.loading && matrix.error === null ? (
          <p className="text-[11px] text-muted" data-testid="coverage-total">
            分母明示：计划项共 {totalItems}，已判定（非 NOT_TESTED）{totalJudged}；
            不用百分比掩盖缺口。
          </p>
        ) : null}
      </div>

      {!enabled ? (
        <EmptyPanel text="选择一个验证计划后渲染其七维覆盖矩阵（P5-8 coverage-show 面）。" />
      ) : matrix.loading ? (
        <SkeletonPanel lines={6} />
      ) : matrix.error !== null ? (
        <ErrorPanel error={matrix.error} onRetry={matrix.retry} />
      ) : (
        <div className="space-y-3" data-testid="coverage-matrix-section">
          <div className="overflow-x-auto rounded-surface border border-edge bg-base-100">
            <table className="table table-sm">
              <thead>
                <tr>
                  <th>维度</th>
                  <th>计划项</th>
                  {FIVE_STATE_COLUMNS.map((column) => (
                    <th key={column.key} className="text-xs">
                      {column.label}
                    </th>
                  ))}
                  <th>盲区</th>
                </tr>
              </thead>
              <tbody>
                {COVERAGE_DIMENSIONS.map((dimension) => {
                  const cell = cellByDimension.get(dimension);
                  const isBlind =
                    cell === undefined ||
                    cell.item_count === 0 ||
                    cell.item_count === cell.not_tested;
                  return (
                    <tr key={dimension}>
                      <td className="text-xs">
                        {COVERAGE_DIMENSION_LABELS[dimension]}
                        <span className="ml-1 font-mono text-[10px] text-muted">
                          {dimension}
                        </span>
                      </td>
                      <td>{cell?.item_count ?? 0}</td>
                      {FIVE_STATE_COLUMNS.map((column) => (
                        <td key={column.key} className={`text-xs ${column.tone}`}>
                          {cell?.[column.key as CellKey] ?? 0}
                        </td>
                      ))}
                      <td>
                        {isBlind ? (
                          <span className="badge badge-sm badge-warning">
                            {cell === undefined || cell.item_count === 0
                              ? "uncovered"
                              : "unjudged"}
                          </span>
                        ) : (
                          <span className="badge badge-sm badge-success">有结论</span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {blindSpots.length > 0 ? (
            <div className="rounded-surface border border-warning/40 bg-warning/5 p-3">
              <p className="text-xs font-semibold">盲区（无命中不等于无漏洞，停止条件可见）</p>
              <p className="mt-1 font-mono text-[11px] text-warning">
                {blindSpots.join(" · ")}
              </p>
            </div>
          ) : null}
          <p className="text-[11px] text-muted">
            数据面：rp-verify coverage-show（P5-8）；未测试（NOT_TESTED）与失败
            （INCONCLUSIVE）逐列分开显示；excluded 项计入分母不算结论（排除是计划
            决策不是测试事实）。假设草稿登记走 rp-verify hypothesis-add（CLI 面）。
          </p>
        </div>
      )}
    </div>
  );
}
