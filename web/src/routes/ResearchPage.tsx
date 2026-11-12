import { WorkspacePlaceholder } from "./WorkspacePlaceholder";

/** 研究工作区（占位，P7-1）：研究项目、目标、任务计划与研究时间线
 *  由后续批次实施（计划 §10.3 Research Workspace）。 */
export function ResearchPage() {
  return (
    <WorkspacePlaceholder
      title="研究工作台"
      workspaceNumber="3"
      fillsIn="后续批次"
      planItems={["研究项目与目标管理", "任务计划与研究时间线", "研究过程记录与上下文"]}
      related={[
        { to: "/projects", label: "项目区（现有任务树）" },
        { to: "/knowledge", label: "知识库" },
      ]}
    />
  );
}
