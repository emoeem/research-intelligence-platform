import { Suspense, lazy, type ReactNode } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Layout } from "./components/Layout";
import { RouteFallback } from "./routes/RouteFallback";

/**
 * 六个一级路由全部走动态 import：初始包只带壳层 + 当前路由（计划 §10.6 的
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
const RunsPage = lazy(() => import("./routes/RunsPage").then((m) => ({ default: m.RunsPage })));
const RuntimePage = lazy(() =>
  import("./routes/RuntimePage").then((m) => ({ default: m.RuntimePage })),
);

function lazyElement(node: ReactNode) {
  return <Suspense fallback={<RouteFallback />}>{node}</Suspense>;
}

const queryClient = new QueryClient();

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          <Route element={<Layout />}>
            <Route index element={lazyElement(<OverviewPage />)} />
            <Route path="projects" element={lazyElement(<ProjectsPage />)} />
            <Route path="knowledge" element={lazyElement(<KnowledgePage />)} />
            <Route path="tools" element={lazyElement(<ToolCenterPage />)} />
            <Route path="runs" element={lazyElement(<RunsPage />)} />
            <Route path="runtime" element={lazyElement(<RuntimePage />)} />
          </Route>
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
