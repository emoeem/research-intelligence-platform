import { formatDateTime } from "../../lib/format";
import type { WorkflowDefinitionRow } from "../../lib/api";

/** 第五区版本列表与历史回看（P8-4）：workflow 选择器 + 全版本 chips
 *  （016 workflow_definition 逐版行），选中历史版本进入只读回看（画布渲染
 *  该版 graph），保存走 P8-4 put（画布图转 definition JSON，版本存储侧
 *  递增 = 现有最大版本 + 1）。 */

export interface VersionListProps {
  workflowOptions: string[];
  workflowId: string;
  versions: WorkflowDefinitionRow[];
  selectedVersion: number | null;
  dirty: boolean;
  saving: boolean;
  saveMessage: string | null;
  saveIssues: string[];
  onSelectWorkflow: (id: string) => void;
  onSelectVersion: (version: number | null) => void;
  onSave: () => void;
}

export function VersionList({
  workflowOptions,
  workflowId,
  versions,
  selectedVersion,
  dirty,
  saving,
  saveMessage,
  saveIssues,
  onSelectWorkflow,
  onSelectVersion,
  onSave,
}: VersionListProps) {
  const latest = versions.length > 0 ? Math.max(...versions.map((v) => v.version)) : null;
  const readOnly = selectedVersion !== null;
  return (
    <section
      data-testid="workflows-versions-section"
      className="rounded-surface border border-edge bg-base-100 p-3"
      aria-label="Workflow 版本列表与历史回看"
    >
      <h2 className="text-xs font-semibold text-base-content/70">
        Workflow 版本与历史回看
        <span className="ml-2 text-[10px] font-normal text-muted">
          016 workflow_definition · 存储侧版本递增
        </span>
      </h2>
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <label className="flex items-center gap-1.5 text-xs">
          <span className="text-muted">workflow</span>
          <input
            type="text"
            className="input input-bordered input-sm w-[220px] font-mono text-xs"
            list="workflow-id-options"
            data-testid="workflow-id-select"
            value={workflowId}
            placeholder="（无定义时可键入新 workflow_id）"
            onChange={(event) => onSelectWorkflow(event.target.value)}
          />
          <datalist id="workflow-id-options">
            {workflowOptions.map((id) => (
              <option key={id} value={id} />
            ))}
          </datalist>
        </label>
        <div className="flex flex-wrap items-center gap-1" data-testid="version-chips">
          {versions.length === 0 ? (
            <span className="text-[10px] text-muted">该 workflow 尚无已保存版本</span>
          ) : (
            versions.map((row) => (
              <button
                key={row.version}
                type="button"
                data-testid={`version-chip-${row.version}`}
                className={`btn btn-xs ${
                  selectedVersion === row.version ? "btn-primary" : "btn-outline"
                }`}
                title={`${row.name} · ${row.node_count ?? "?"} 节点 ${row.edge_count ?? "?"} 边 · ${
                  row.created_at ? formatDateTime(row.created_at) : ""
                }`}
                onClick={() => onSelectVersion(row.version)}
              >
                v{row.version}
              </button>
            ))
          )}
        </div>
        {readOnly ? (
          <span className="badge badge-ghost badge-sm" data-testid="readonly-history-badge">
            历史回看 v{selectedVersion}（只读）
          </span>
        ) : null}
        {readOnly || latest !== null ? (
          <button
            type="button"
            className="btn btn-outline btn-xs"
            onClick={() => onSelectVersion(null)}
            disabled={!readOnly}
          >
            编辑最新{latest !== null ? `（v${latest} 基础上）` : ""}
          </button>
        ) : null}
        <span className="ml-auto flex items-center gap-2">
          {dirty ? <span className="badge badge-warning badge-sm">未保存改动</span> : null}
          <button
            type="button"
            data-testid="save-definition-btn"
            className="btn btn-primary btn-xs"
            disabled={saving || readOnly}
            onClick={onSave}
          >
            {saving ? "保存中…" : "保存（版本递增）"}
          </button>
        </span>
      </div>
      {saveIssues.length > 0 ? (
        <ul
          className="mt-2 list-inside list-disc text-[11px] text-warning"
          data-testid="save-issues"
        >
          {saveIssues.map((issue) => (
            <li key={issue}>{issue}</li>
          ))}
        </ul>
      ) : null}
      {saveMessage ? (
        <p className="mt-2 break-all text-[11px] text-success" data-testid="save-message">
          {saveMessage}
        </p>
      ) : null}
      <p className="mt-1 text-[10px] leading-4 text-muted">
        画布图保存为 definition-v1 JSON：graph 节点/边向量经 rp-workflow put 四道门后追加一版， 文档
        version 仅作声明，落库版本由存储侧分配（现有最大版本 + 1）。
      </p>
    </section>
  );
}
