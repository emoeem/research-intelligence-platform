/** Research Tasks 列表（P5-1，Phase 5 §4.5.B 首版）：/api/tasks 真实列表 +
 *  状态/类型/文本过滤（客户端过滤：服务端列表面无 q 参数，分母如实显示）+
 *  行点击进入任务详情（URL 深链接 /research/tasks/:taskId）。三态统一。
 *
 *  注意：本页数据是 research_task 的 RTIS ETL 只读镜像（P5-0 审计结论 G2）；
 *  编排容器（创建/暂停/恢复/取消）随 P5-2 验证契约批次交付，本版不渲染
 *  不可用的操作按钮。 */

import { useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { useApi } from "../../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../../components/ui";
import { CountChips, DataTable, FilterSearch, FilterSelect } from "../../components/data";
import {
  RESEARCH_TASK_STATES,
  TASK_COLUMNS,
  TASK_STATE_TONES,
  type ResearchTaskList,
  type ResearchTaskRow,
} from "./shared";

export function ResearchTasksPage() {
  const navigate = useNavigate();
  const [stateFilter, setStateFilter] = useState("all");
  const [typeFilter, setTypeFilter] = useState("all");
  const [textFilter, setTextFilter] = useState("");
  const list = useApi<ResearchTaskList>("/api/tasks?limit=200");

  const items = list.data?.items ?? [];
  const byState = Object.entries(list.data?.by_state ?? {});
  const byType = Object.entries(list.data?.by_type ?? {});

  const filtered = useMemo(() => {
    const needle = textFilter.trim().toLowerCase();
    return items.filter((task) => {
      if (stateFilter !== "all" && task.state !== stateFilter) return false;
      if (typeFilter !== "all" && task.task_type !== typeFilter) return false;
      if (!needle) return true;
      const haystack = [task.title ?? "", task.objective ?? "", task.target ?? "", task.task_id]
        .join("\n")
        .toLowerCase();
      return haystack.includes(needle);
    });
  }, [items, stateFilter, typeFilter, textFilter]);

  if (list.loading) {
    return (
      <div>
        <PageHeader title="研究任务" />
        <SkeletonPanel lines={6} />
      </div>
    );
  }
  if (list.error !== null) {
    return (
      <div>
        <PageHeader title="研究任务" />
        <ErrorPanel error={list.error} onRetry={list.retry} />
      </div>
    );
  }

  return (
    <div>
      <PageHeader
        title="研究任务"
        actions={
          <span className="badge badge-ghost">数据源：/api/tasks（RTIS 任务镜像，只读）</span>
        }
      />

      {items.length === 0 ? (
        <EmptyPanel text={list.data?.message ?? "暂无研究任务。"} />
      ) : (
        <>
          <p className="mb-2 text-xs text-muted">
            共 {list.data?.total ?? 0} 个任务（载入 {items.length}，过滤后 {filtered.length}）
          </p>
          <div className="mb-3 flex flex-wrap gap-3">
            <CountChips entries={byState} tones={TASK_STATE_TONES} label="状态计数" />
            <CountChips entries={byType} label="类型计数" outline />
          </div>
          <div className="mb-3 flex flex-wrap items-center gap-3">
            <FilterSelect
              label="状态"
              value={stateFilter}
              onChange={setStateFilter}
              options={[
                { value: "all", label: "全部状态" },
                ...RESEARCH_TASK_STATES.map((state) => ({ value: state, label: state })),
                ...byState
                  .map(([state]) => ({ value: state, label: state }))
                  .filter(
                    (option) => !(RESEARCH_TASK_STATES as readonly string[]).includes(option.value),
                  ),
              ]}
            />
            <FilterSelect
              label="类型"
              value={typeFilter}
              onChange={setTypeFilter}
              options={[
                { value: "all", label: "全部类型" },
                ...byType.map(([type]) => ({ value: type, label: type })),
              ]}
            />
            <FilterSearch
              label="搜索"
              value={textFilter}
              placeholder="标题 / 目标 / 任务 ID"
              onChange={setTextFilter}
            />
          </div>
          <DataTable<ResearchTaskRow>
            caption="研究任务列表"
            columns={TASK_COLUMNS}
            rows={filtered}
            rowKey={(row) => row.task_id}
            emptyText="没有匹配过滤条件的任务。"
            onRowActivate={(row) => {
              void navigate(`/research/tasks/${encodeURIComponent(row.task_id)}`);
            }}
          />
        </>
      )}
    </div>
  );
}
