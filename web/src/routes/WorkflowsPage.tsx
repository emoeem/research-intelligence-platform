import { WorkspacePlaceholder } from "./WorkspacePlaceholder";

/** 流程工作区（占位，P7-1）：Workflow Studio（画布、节点配置、运行与
 *  版本）由 Phase 8 实施（SESSION-HANDOFF §11.4，计划 §10.4 G）。 */
export function WorkflowsPage() {
  return (
    <WorkspacePlaceholder
      title="Workflow Studio"
      workspaceNumber="8"
      fillsIn="Phase 8"
      planItems={[
        "流程画布与节点配置（P8-4）",
        "Workflow 模板与版本",
        "WorkflowRun 运行状态与日志",
      ]}
      related={[
        { to: "/runs", label: "运行记录" },
        { to: "/projects", label: "项目区" },
      ]}
    />
  );
}
