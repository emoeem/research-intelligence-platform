/** ECharts 图表封装（直连 echarts/core，无第三方绑定层）。
 *  演进记录：P5-2 采用参照实现的 echarts-for-react/esm/core 绑定模式；P5-4
 *  浏览器实测（1440x900 手核）发现其内部容器在 init 时测得 width 0、svg
 *  从未生成（size-sensor 测量链在 Vite dev 预打包下不可靠），改为本封装直连
 *  echarts/core：挂载后测量容器 → init → setOption，ResizeObserver 跟随
 *  容器 resize，卸载 dispose。按需注册在 ./echarts-install.ts（静态具名
 *  import 保证 tree-shake；动态 import namespace 属性访问会把全家桶拉回，
 *  本仓实测 1085 kB，拆后 561 kB）。 */

import { useEffect, useRef, useState, type CSSProperties } from "react";
import type { EChartsType } from "echarts/core";
import type { EChartsReactProps } from "./echarts-install-types";

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

export interface EChartProps {
  option: unknown;
  style?: CSSProperties;
  className?: string;
  notMerge?: boolean;
  opts?: { renderer?: "canvas" | "svg"; width?: number | "auto"; height?: number | "auto" };
  onEvents?: Record<string, (params: unknown) => void>;
}

export function EChart({ option, style, className, notMerge, opts, onEvents }: EChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<EChartsType | null>(null);
  // option/onEvents 经 ref 读取：init 只做一次，更新走 setOption，不重建实例。
  const optionRef = useRef(option);
  optionRef.current = option;
  const onEventsRef = useRef(onEvents);
  onEventsRef.current = onEvents;
  const reducedMotion = usePrefersReducedMotion();

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return undefined;
    let disposed = false;
    let observer: ResizeObserver | null = null;
    // dev 可观测点：init 链状态落 dataset，浏览器手核时可直接读。
    if (import.meta.env.DEV) container.dataset.echartStage = "effect-start";

    (async () => {
      const { ensureInstalled } = await import("./echarts-install");
      const echarts = ensureInstalled();
      if (disposed) return;
      if (import.meta.env.DEV) container.dataset.echartStage = "installed";
      // 不等 rAF（后台/非绘制标签页里 rAF 永不触发，会挂死 init 链）：
      // 直接 init，尺寸为 0 时由下方 ResizeObserver 在容器获得真实尺寸后
      // chart.resize() 补齐（echarts 标准模式）。
      if (disposed || !container.isConnected) return;
      if (import.meta.env.DEV) container.dataset.echartStage = "initializing";
      const chart = echarts.init(container, undefined, {
        renderer: opts?.renderer ?? "svg",
        ...opts,
      });
      chartRef.current = chart;
      const events = onEventsRef.current ?? {};
      for (const [name, handler] of Object.entries(events)) {
        chart.on(name, (params: unknown) => handler(params));
      }
      const base = optionRef.current;
      const merged =
        reducedMotion && base && typeof base === "object" && !Array.isArray(base)
          ? { ...(base as Record<string, unknown>), animation: false }
          : base;
      chart.setOption(merged as Parameters<typeof chart.setOption>[0], notMerge ?? false);
      if (import.meta.env.DEV) container.dataset.echartStage = "ready";
      observer = new ResizeObserver(() => chart.resize());
      observer.observe(container);
    })().catch((error: unknown) => {
      console.error("[EChart] init failed:", error);
      if (import.meta.env.DEV && container.isConnected) {
        container.dataset.echartStage = "failed";
        container.dataset.echartError = error instanceof Error ? error.message : String(error);
      }
    });

    return () => {
      disposed = true;
      observer?.disconnect();
      chartRef.current?.dispose();
      chartRef.current = null;
    };
    // init 一次；opts.renderer 变化属重建语义，交由调用方用 key 重挂。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    // 实例已在时的 option 更新（reduced motion 翻转也走这里）。
    const base = option;
    const merged =
      reducedMotion && base && typeof base === "object" && !Array.isArray(base)
        ? { ...(base as Record<string, unknown>), animation: false }
        : base;
    chartRef.current?.setOption(
      merged as Parameters<typeof chartRef.current.setOption>[0],
      notMerge ?? false,
    );
  }, [option, notMerge, reducedMotion]);

  return <div ref={containerRef} className={className} style={style} />;
}

export type { EChartsReactProps };
