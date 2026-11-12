import { WorkspacePlaceholder } from "./WorkspacePlaceholder";

/** 设置工作区（占位，P7-1）：界面偏好与系统设置由后续批次实施
 *  （计划 §10.3 System）。主题切换已在图标轨底部可用，本批不重复入口。 */
export function SettingsPage() {
  return (
    <WorkspacePlaceholder
      title="设置"
      workspaceNumber="11"
      fillsIn="后续批次"
      planItems={["界面偏好（主题、密度、语言）", "系统设置（运行时、集成、账号）"]}
      related={[{ to: "/runtime", label: "运行时状态" }]}
    />
  );
}
