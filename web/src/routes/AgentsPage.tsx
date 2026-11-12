import { WorkspacePlaceholder } from "./WorkspacePlaceholder";

/** Agent 工作区（占位，P7-1）：Agent Studio（配置、能力绑定、运行记录、
 *  记忆）由 P7-7 实施（SESSION-HANDOFF §11.4 与 PHASE-ROADMAP P7-6/7-7）。 */
export function AgentsPage() {
  return (
    <WorkspacePlaceholder
      title="Agent Studio"
      workspaceNumber="7"
      fillsIn="P7-7"
      planItems={[
        "Agent 配置与能力绑定（工具/知识引用）",
        "Agent 运行记录与轨迹回放",
        "Task/Evidence/Research Memory 视图",
      ]}
      related={[
        { to: "/tools", label: "工具中心" },
        { to: "/runs", label: "运行记录" },
      ]}
    />
  );
}
