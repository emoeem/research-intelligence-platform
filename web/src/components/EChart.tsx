/** ECharts 图表封装：双层 lazy + 按需注册。整体沿自参照实现
 *  admin/src/components/EChart.tsx（UI 复用盘点 §3.9），零业务依赖；echarts 6
 *  + echarts-for-react 3 的版本组合与参照仓一致（esm/core 互操作已在其
 *  Rolldown 构建下实测，升级依赖时需重验）。 */

import { Suspense, lazy, useEffect, useState, type ComponentType, type CSSProperties } from "react";
import type { EChartsReactProps } from "echarts-for-react/esm/types";

/**
 * ECharts 是最重的前端依赖，而只有少数页面渲染图表。这里有两层懒加载：
 *
 * 1. 路由级：本封装把库推迟到第一次真正挂载图表时才动态 import，登录页与
 *    列表页完全不为此付费。
 * 2. 按需注册：import 走 `echarts/core`，只注册本项目用到的组件（柱图/饼图、
 *    grid、tooltip、legend、双 renderer），而不是全家桶。整族图表
 *    （line/scatter/map 等）被 tree-shake 掉，异步 chunk 只有全家桶的三分之一。
 *
 * 新图表类型必须加进下面的 `use([...])` 列表，否则图表静默渲染空白。
 * AxisPointerComponent 支撑柱图的 axis tooltip 触发。
 */
const ReactECharts = lazy(async () => {
  // 注册链拆在 ./echarts-install.ts：install 的静态具名 import 必须放在一个
  // 被动态 import 的独立模块里，tree-shake 才能按具名收窄；在本组件里直接
  // `await import("echarts/charts")` 再取 namespace 属性会把全家桶整段拉回
  // （本仓实测 vendor-echarts 1085 kB，拆后见 build 输出）。深挖
  // `lib/chart/*/install` 子路径则是 echarts 5 时代的写法，v6 的 exports
  // 映射已挡住它（无类型声明，TS2307 实测）。
  const [{ default: EChartsReactCore }, { ensureInstalled }] = await Promise.all([
    // 必须用 `esm/core` 入口，不是 `lib/core`：lib 构建是 CJS 且带 __esModule，
    // 在 rolldown interop 下动态 import 的 namespace default 可能返回整个
    // exports 对象而不是组件类，图表一挂载就以 React error #130 崩溃
    // （element type invalid: got object，参照实现实测教训）。
    import("echarts-for-react/esm/core"),
    import("./echarts-install"),
  ]);
  const echartsCore = ensureInstalled();
  // `/esm/core` 入口要显式传 echarts namespace（默认入口自己绑全家桶）。
  // 把带注册的 `echarts/core` 递进去，按需构建才能真正画图。
  const BoundCore = (props: EChartsReactProps) => (
    <EChartsReactCore {...props} echarts={echartsCore} />
  );
  return { default: BoundCore as unknown as ComponentType<Record<string, unknown>> };
});

export interface EChartProps {
  option: unknown;
  style?: CSSProperties;
  className?: string;
  theme?: string | Record<string, unknown>;
  notMerge?: boolean;
  lazyUpdate?: boolean;
  showLoading?: boolean;
  loadingOption?: unknown;
  opts?: { renderer?: "canvas" | "svg"; width?: number | "auto"; height?: number | "auto" };
  onEvents?: Record<string, (params: unknown, chart: unknown) => void>;
}

/** 与图表同高的骨架占位，避免布局跳动。 */
function ChartFallback({ style }: { style?: CSSProperties }) {
  return (
    <div
      aria-hidden="true"
      className="animate-pulse rounded-control bg-surface-2"
      style={{ height: 180, ...style }}
    />
  );
}

/**
 * ECharts 的饼图/环图默认有约 1s 的入场动画。向系统申请了减少动态效果的用户
 * 不想要这段运动；任何在动画中途采样 DOM 的东西（截图、视觉回归）也会把它读成
 * 「图表坏了，画了一半的环」。同一开关同时解决两者，所以这里强制关闭而不是
 * 交给每个调用方自己传。
 */
function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState(
    () =>
      typeof window !== "undefined" &&
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true,
  );
  useEffect(() => {
    const query = window.matchMedia?.("(prefers-reduced-motion: reduce)");
    if (!query) return undefined;
    const onChange = () => setReduced(query.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);
  return reduced;
}

export function EChart(props: EChartProps) {
  const reducedMotion = usePrefersReducedMotion();
  const base = props.option;
  const option =
    reducedMotion && base && typeof base === "object" && !Array.isArray(base)
      ? { ...(base as Record<string, unknown>), animation: false }
      : base;
  return (
    <Suspense fallback={<ChartFallback style={props.style} />}>
      <ReactECharts {...(props as unknown as Record<string, unknown>)} option={option} />
    </Suspense>
  );
}

export default EChart;
