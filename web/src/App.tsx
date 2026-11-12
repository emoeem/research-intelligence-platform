import { Suspense, lazy, useState, type ReactNode } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Layout } from "./components/Layout";
import { RouteFallback } from "./routes/RouteFallback";

/**
 * 一级路由全部走动态 import：初始包只带壳层 + 当前路由（计划 §10.6 的
 * route-level lazy 要求，模式沿自参照实现 App 的 lazy + lazyElement）。
 * 页面由 P5-5 到 P5-8 逐页补全；壳层见 components/Layout.tsx。
 */
const OverviewPage = lazy(() =>
  import("./routes/OverviewPage").then((m) => ({ default: m.OverviewPage })),
);
const ProjectsPage = lazy(() =>
  import("./routes/ProjectsPage").then((m) => ({ default: m.ProjectsPage })),
);
const KnowledgePage = lazy(() =>
  import("./routes/KnowledgePage").then((m) => ({ default: m.KnowledgePage })),
);
const ToolCenterPage = lazy(() =>
  import("./routes/ToolCenterPage").then((m) => ({ default: m.ToolCenterPage })),
);
const ToolDetailPage = lazy(() =>
  import("./routes/ToolDetailPage").then((m) => ({ default: m.ToolDetailPage })),
);
const RunsPage = lazy(() => import("./routes/RunsPage").then((m) => ({ default: m.RunsPage })));
const RunDetailPage = lazy(() =>
  import("./routes/RunDetailPage").then((m) => ({ default: m.RunDetailPage })),
);
// P7-5：trace 统一关联链查询页（/traces?id=，六 ID 任一入参）。
const TracePage = lazy(() => import("./routes/TracePage").then((m) => ({ default: m.TracePage })));
const RuntimePage = lazy(() =>
  import("./routes/RuntimePage").then((m) => ({ default: m.RuntimePage })),
);
const PluginsPage = lazy(() =>
  import("./routes/PluginsPage").then((m) => ({ default: m.PluginsPage })),
);
const PluginDetailPage = lazy(() =>
  import("./routes/PluginDetailPage").then((m) => ({ default: m.PluginDetailPage })),
);
// P7-1 新增四区（Research/Settings 占位页后续批次填充，Agents 已由 P7-7
// 落实页三区，Workflows Phase 8 填充）；既有 10 页路由路径不变。
const ResearchPage = lazy(() =>
  import("./routes/ResearchPage").then((m) => ({ default: m.ResearchPage })),
);
const AgentsPage = lazy(() =>
  import("./routes/AgentsPage").then((m) => ({ default: m.AgentsPage })),
);
const WorkflowsPage = lazy(() =>
  import("./routes/WorkflowsPage").then((m) => ({ default: m.WorkflowsPage })),
);
const SettingsPage = lazy(() =>
  import("./routes/SettingsPage").then((m) => ({ default: m.SettingsPage })),
);

function lazyElement(node: ReactNode) {
  return <Suspense fallback={<RouteFallback />}>{node}</Suspense>;
}

/**
 * QueryClient 默认值对齐原手写 useApi 行为（lib/hooks.ts 旧版语义）：
 * 无自动重试（原实现只有手动 retry）、窗口聚焦/网络恢复不重取、
 * staleTime 0（挂载即重取）。lib/hooks.ts 是唯一消费面（P5 复用纪律整改）。
 */
function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
        staleTime: 0,
      },
    },
  });
}

export function App() {
  // client 随 App 挂载创建（useState 惰性初始化，重渲染不重建）：生产单次
  // 挂载与模块级单例等价；测试反复挂载 App 时各自独立缓存，互不串扰。
  const [queryClient] = useState(createQueryClient);
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          <Route element={<Layout />}>
            <Route index element={lazyElement(<OverviewPage />)} />
            <Route path="projects" element={lazyElement(<ProjectsPage />)} />
            <Route path="knowledge" element={lazyElement(<KnowledgePage />)} />
            <Route path="tools" element={lazyElement(<ToolCenterPage />)} />
            <Route path="tools/:toolId" element={lazyElement(<ToolDetailPage />)} />
            <Route path="runs" element={lazyElement(<RunsPage />)} />
            <Route path="runs/:runId" element={lazyElement(<RunDetailPage />)} />
            <Route path="traces" element={lazyElement(<TracePage />)} />
            <Route path="runtime" element={lazyElement(<RuntimePage />)} />
            <Route path="plugins" element={lazyElement(<PluginsPage />)} />
            <Route path="plugins/:pluginId" element={lazyElement(<PluginDetailPage />)} />
            <Route path="research" element={lazyElement(<ResearchPage />)} />
            <Route path="agents" element={lazyElement(<AgentsPage />)} />
            <Route path="workflows" element={lazyElement(<WorkflowsPage />)} />
            <Route path="settings" element={lazyElement(<SettingsPage />)} />
          </Route>
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
