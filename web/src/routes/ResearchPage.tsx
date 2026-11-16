/** 研究工作台（P5-1）：Research Workspace 骨架入口 = 总览子页。子页路由族
 *  见 App.tsx（/research/*）；工作区 tab 数据源见 workspaces.ts research 面。
 *  通用占位（WorkspacePlaceholder）自本批起移除——本页是真实 API 驱动的
 *  工作区（P5-0 审计差距 G1 收口）。 */

import { ResearchOverviewPage } from "./research/ResearchOverviewPage";

export function ResearchPage() {
  return <ResearchOverviewPage />;
}
