import { NODE_CATALOG } from "../../lib/workflow-graph";

/** 第一区左侧节点分类面板（P8-4，计划 §10.4 G 左栏）：按 DSL 词表单源分
 *  组（workflow-engine 八类），点击追加节点到画布空闲位，HTML5 拖拽把
 *  dataTransfer 交给画布 onDrop 落点（拖拽是 E2E 六步第一动作）。
 *  「数据转换/证据保存」暂无 DSL 词值，不伪造面板项（词表扩展后追加）。 */

export const PALETTE_DND_TYPE = "application/x-rip-workflow-node";

export function NodePalette({
  onAdd,
  onDragStart,
}: {
  onAdd: (type: string) => void;
  onDragStart: (event: React.DragEvent, type: string) => void;
}) {
  return (
    <aside
      data-testid="workflows-palette-section"
      aria-label="节点分类面板"
      className="flex h-full min-h-0 flex-col gap-2 overflow-y-auto rounded-surface border border-edge bg-base-100 p-3"
    >
      <h2 className="text-xs font-semibold text-base-content/70">节点分类面板</h2>
      <p className="text-[10px] leading-4 text-muted">
        点击追加或拖入画布；词表与 workflow-engine DSL 单源
      </p>
      <ul className="flex flex-col gap-1.5">
        {NODE_CATALOG.map((entry) => (
          <li key={entry.type}>
            <button
              type="button"
              data-testid={`palette-${entry.type}`}
              data-node-type={entry.type}
              draggable
              onDragStart={(event) => onDragStart(event, entry.type)}
              onClick={() => onAdd(entry.type)}
              className="w-full cursor-grab rounded-control border border-edge-soft bg-base-100 p-2 text-left transition-colors hover:border-primary active:cursor-grabbing"
              title={`${entry.label}：${entry.hint}`}
            >
              <span className="text-xs font-medium">{entry.label}</span>
              <span className="ml-1.5 font-mono text-[10px] text-muted">
                {entry.type}
              </span>
              <p className="mt-0.5 text-[10px] leading-4 text-muted">{entry.hint}</p>
              {entry.required.length > 0 ? (
                <p className="text-[10px] text-warning">必填参数：{entry.required.join("、")}</p>
              ) : null}
            </button>
          </li>
        ))}
      </ul>
    </aside>
  );
}
