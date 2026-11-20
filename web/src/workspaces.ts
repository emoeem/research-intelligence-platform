/**
 * 工作区信息架构的唯一数据源（RIP 版，模型沿自参照实现 admin/src/workspaces.ts，
 * UI 复用盘点 §2.1：零 import 纯数据、图标字符串键、稳定编号、buildFeatureMap）。
 *
 * Layout 的图标轨 / 工作区切换器 / 中央多标签工作区的固定 tab 与总览页的功能
 * 地图都从这里取数，数据只维护一份。本模块保持零 import：图标以字符串键存储，
 * 由 Layout 侧映射成 lucide 组件；类型全部就地声明。
 *
 * RIP 十一工作区信息架构（SESSION-HANDOFF §11.2，计划 §10.3）：
 * Dashboard、Projects、Research、Knowledge、Tools、Plugins、Agents、
 * Workflows、Runs、Runtime、Settings。概念分离硬约束（§11.3）：
 * Workspace 是界面组织方式，不是领域对象；Project 是研究对象、Agent 是
 * 执行主体、Workflow 是执行过程，本文件只描述界面导航，不建领域模型。
 *
 * 本批（P7-1）每个工作区先落一个着陆 tab（entry 即唯一 tab）；工作区内的
 * 分页（如工具中心/工具包）随对应批次落页后再扩 tabs，编号坐标不变。
 */

export type WorkspaceId =
  | "dashboard"
  | "projects"
  | "research"
  | "knowledge"
  | "tools"
  | "plugins"
  | "agents"
  | "workflows"
  | "runs"
  | "runtime"
  | "settings"
  | "websec";

/** 图标键 → Layout 侧 ICONS 表映射为 lucide-react 组件（Record 全键覆盖，漏键 tsc 报错）。 */
export type WorkspaceIconKey =
  | "layoutDashboard"
  | "folderKanban"
  | "radar"
  | "database"
  | "wrench"
  | "puzzle"
  | "bot"
  | "workflow"
  | "circlePlay"
  | "serverCog"
  | "shieldCheck"
  | "settings"
  | "globe"
  | "refreshCw";

/** 键全集（测试用：数据里出现的图标键必须都在其中）。 */
export const WORKSPACE_ICON_KEYS: readonly WorkspaceIconKey[] = [
  "layoutDashboard",
  "folderKanban",
  "radar",
  "database",
  "wrench",
  "puzzle",
  "bot",
  "workflow",
  "circlePlay",
  "serverCog",
  "shieldCheck",
  "settings",
  "globe",
  "refreshCw",
];

export interface WorkspaceNavItem {
  /** 工作区内序号（1 起连续）：编号取自数据，过滤后不漂移。 */
  order: number;
  to: string;
  label: string;
  icon: WorkspaceIconKey;
  /** 一句话说明（tab tooltip 与功能地图芯片）。 */
  hint: string;
}

export interface Workspace {
  id: WorkspaceId;
  /** 工作区序号（1 起）：图标轨 tooltip、tab 条编号与功能地图共用。 */
  order: number;
  label: string;
  icon: WorkspaceIconKey;
  /** 图标轨点击后的着陆页（第一个 tab）。 */
  entry: string;
  /** 一句话说明（图标轨 tooltip / 功能地图工作区卡副标题）。 */
  hint: string;
  /** 工作区顶部 tab 条（含着陆页自身）。 */
  tabs: WorkspaceNavItem[];
}

export const WORKSPACES: readonly Workspace[] = [
  {
    id: "dashboard",
    order: 1,
    label: "Dashboard",
    icon: "layoutDashboard",
    entry: "/",
    hint: "平台 KPI 与数据面状态",
    tabs: [
      {
        order: 1,
        to: "/",
        label: "总览",
        icon: "layoutDashboard",
        hint: "KPI、功能地图与数据面状态",
      },
    ],
  },
  {
    id: "projects",
    order: 2,
    label: "Projects",
    icon: "folderKanban",
    entry: "/projects",
    hint: "研究项目与任务树",
    tabs: [
      {
        order: 1,
        to: "/projects",
        label: "项目",
        icon: "folderKanban",
        hint: "项目、任务树与研究时间线",
      },
    ],
  },
  {
    id: "research",
    order: 3,
    label: "Research",
    icon: "radar",
    entry: "/research",
    hint: "研究工作台（Phase 5 建设中）",
    tabs: [
      {
        order: 1,
        to: "/research",
        label: "研究总览",
        icon: "radar",
        hint: "研究概况、任务状态与活跃运行（真实 API）",
      },
      {
        order: 2,
        to: "/research/tasks",
        label: "任务",
        icon: "circlePlay",
        hint: "研究任务列表与详情（RTIS 任务镜像，只读）",
      },
      {
        order: 3,
        to: "/research/assets",
        label: "资产",
        icon: "folderKanban",
        hint: "研究目标与资产（域随 P5-2/P5-5 落库）",
      },
      {
        order: 4,
        to: "/research/knowledge",
        label: "知识上下文",
        icon: "database",
        hint: "知识检索入口（Matcher 解释面随 P5-3 升级）",
      },
      {
        order: 5,
        to: "/research/plans",
        label: "验证计划",
        icon: "shieldCheck",
        hint: "验证计划与五态结果（P5-6：暂停/恢复/取消控制面）",
      },
      {
        order: 6,
        to: "/research/evidence",
        label: "证据",
        icon: "database",
        hint: "证据档列表与详情（P5-9b：SHA256/六 ID 跳转/Raw 与 Pretty）",
      },
      {
        order: 7,
        to: "/research/coverage",
        label: "覆盖矩阵",
        icon: "radar",
        hint: "七维五态覆盖与盲区（P5-8 coverage-show 面）",
      },
    ],
  },
  {
    id: "knowledge",
    order: 4,
    label: "Knowledge",
    icon: "database",
    entry: "/knowledge",
    hint: "知识条目检索与治理",
    tabs: [
      { order: 1, to: "/knowledge", label: "知识库", icon: "database", hint: "知识条目检索与详情" },
      {
        order: 2,
        to: "/documents",
        label: "文档中心",
        icon: "folderKanban",
        hint: "研究资料导入、版本与来源管理（RIP3）",
      },
    ],
  },
  {
    id: "tools",
    order: 5,
    label: "Tools",
    icon: "wrench",
    entry: "/tools",
    hint: "工具目录与详情",
    tabs: [
      { order: 1, to: "/tools", label: "工具中心", icon: "wrench", hint: "工具目录、详情与试运行" },
    ],
  },
  {
    id: "plugins",
    order: 6,
    label: "Plugins",
    icon: "puzzle",
    entry: "/plugins",
    hint: "插件目录与生命周期",
    tabs: [
      {
        order: 1,
        to: "/plugins",
        label: "插件中心",
        icon: "puzzle",
        hint: "插件目录、生命周期与 Manifest 详情",
      },
    ],
  },
  {
    id: "agents",
    order: 7,
    label: "Agents",
    icon: "bot",
    entry: "/agents",
    hint: "Agent Studio",
    tabs: [
      {
        order: 1,
        to: "/agents",
        label: "Agent Studio",
        icon: "bot",
        hint: "Agent 定义与能力绑定、运行轨迹回放、三类 Memory 视图",
      },
    ],
  },
  {
    id: "workflows",
    order: 8,
    label: "Workflows",
    icon: "workflow",
    entry: "/workflows",
    hint: "Workflow Studio（占位）",
    tabs: [
      {
        order: 1,
        to: "/workflows",
        label: "Workflow Studio",
        icon: "workflow",
        hint: "流程画布、节点配置与版本（Phase 8 填充）",
      },
    ],
  },
  {
    id: "runs",
    order: 9,
    label: "Runs",
    icon: "circlePlay",
    entry: "/runs",
    hint: "运行记录与 Artifact",
    tabs: [
      {
        order: 1,
        to: "/runs",
        label: "运行记录",
        icon: "circlePlay",
        hint: "Run 列表与 Artifact 预览",
      },
    ],
  },
  {
    id: "runtime",
    order: 10,
    label: "Runtime",
    icon: "serverCog",
    entry: "/runtime",
    hint: "运行时节点与心跳",
    tabs: [
      { order: 1, to: "/runtime", label: "运行时", icon: "serverCog", hint: "节点在线状态与心跳" },
    ],
  },
  {
    id: "settings",
    order: 11,
    label: "Settings",
    icon: "settings",
    entry: "/settings",
    hint: "设置（含 WSW 代理/证书状态）",
    tabs: [
      {
        order: 1,
        to: "/settings",
        label: "设置",
        icon: "settings",
        hint: "界面偏好与系统设置（后续批次填充）",
      },
      {
        order: 2,
        to: "/settings/wsw-proxy",
        label: "代理状态",
        icon: "serverCog",
        hint: "WSW-2 Listener 六态/计数/拦截队列摘要（W2-D8）",
      },
      {
        order: 3,
        to: "/settings/wsw-cert",
        label: "证书状态",
        icon: "shieldCheck",
        hint: "WSW-2 CA 指纹/有效期/显式导出与重生成（W2-D8）",
      },
    ],
  },
  {
    id: "websec",
    order: 12,
    label: "Web Security",
    icon: "shieldCheck",
    entry: "/websec/history",
    hint: "HTTP History 与 Repeater（WSW-3）",
    tabs: [
      {
        order: 1,
        to: "/websec/history",
        label: "HTTP History",
        icon: "globe",
        hint: "捕获/发送交换行（游标分页 + 过滤搜索 + 六视图详情，W3-D8.3）",
      },
      {
        order: 2,
        to: "/websec/repeater",
        label: "Repeater",
        icon: "refreshCw",
        hint: "多 Tab 请求编辑重放（append-only 版本历史，W3-D4/D8.3）",
      },
    ],
  },
];

/** 按序号排列的工作区（防御式排序：数据文件内乱序也不影响展示顺序）。 */
export const WORKSPACES_BY_ORDER: readonly Workspace[] = [...WORKSPACES].sort(
  (a, b) => a.order - b.order,
);

/**
 * tab 的稳定编号（如 "5.1"）：workspace 序号 + tab 在工作区内的序号。
 * 编号只取显式 order 字段：将来按角色/能力过滤 tab 后编号也不漂移，
 * 口头「去 5.1」始终成立（参照实现的 T2 纪律）。
 */
export function tabNumberOf(workspace: Workspace, tab: WorkspaceNavItem): string {
  return `${workspace.order}.${tab.order}`;
}

/** 功能地图的计数输入（来自 /api/overview 的真实计数，总览页传入）。 */
export interface FeatureMapCounts {
  knowledge: number;
  tools: number;
  runs: number;
}

export interface FeatureMapBadge {
  label: string;
  value: number;
}

export interface FeatureMapTab {
  /** 稳定编号（tabNumberOf），如 "5.1"。 */
  number: string;
  label: string;
  to: string;
  hint: string;
}

export interface FeatureMapWorkspace {
  number: string;
  id: WorkspaceId;
  label: string;
  hint: string;
  entry: string;
  badges: FeatureMapBadge[];
  tabs: FeatureMapTab[];
}

/**
 * 从工作区数据推导功能地图模型：编号工作区卡 → tab 芯片（一句话 hint）+ 计数徽章。
 * 徽章只标在有真实数据来源的工作区上（knowledge/tools/runs 计数均来自
 * /api/overview 的既有字段），宁缺毋滥不猜语义。
 */
export function buildFeatureMap(counts: FeatureMapCounts): FeatureMapWorkspace[] {
  return WORKSPACES_BY_ORDER.map((workspace) => ({
    number: String(workspace.order),
    id: workspace.id,
    label: workspace.label,
    hint: workspace.hint,
    entry: workspace.entry,
    badges:
      workspace.id === "dashboard"
        ? [
            { label: "知识条目", value: counts.knowledge },
            { label: "工具", value: counts.tools },
            { label: "运行", value: counts.runs },
          ]
        : workspace.id === "knowledge"
          ? [{ label: "知识条目", value: counts.knowledge }]
          : workspace.id === "tools"
            ? [{ label: "工具", value: counts.tools }]
            : workspace.id === "runs"
              ? [{ label: "运行", value: counts.runs }]
              : [],
    tabs: workspace.tabs.map((tab) => ({
      number: tabNumberOf(workspace, tab),
      label: tab.label,
      to: tab.to,
      hint: tab.hint,
    })),
  }));
}
