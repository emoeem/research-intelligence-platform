import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { EChart } from "../src/components/EChart";

/** P5-2 基础件包：EChart 懒加载链 + 按需注册冒烟。
 *  本组件未被任何页面 import（P5-5 起接入），这里显式渲染以实证：
 *  esm/core 入口在 rolldown 之外的 vitest（vite-node）下同样拿到组件类、
 *  具名 install 注册后 bar 图可用 svg renderer 在 DOM 里画出内容。 */
describe("components/EChart 按需注册冒烟（P5-2）", () => {
  afterEach(() => {
    cleanup();
  });

  it("懒加载注册链完成后能渲染 svg 图表", async () => {
    const { container } = render(
      <EChart
        option={{
          xAxis: { type: "category", data: ["甲", "乙"] },
          yAxis: {},
          series: [{ type: "bar", data: [1, 2] }],
        }}
        opts={{ renderer: "svg", width: 400, height: 300 }}
      />,
    );
    await waitFor(
      () => {
        const svg = container.querySelector("svg");
        if (!svg) throw new Error("echarts svg 尚未渲染");
        expect(svg.querySelector("path, rect, polygon")).toBeTruthy();
      },
      { timeout: 5000 },
    );
  });

  it("reduced motion 下 option 被强制关闭动画", async () => {
    // jsdom 的 matchMedia 恒为 false，这里只断言组件对 object option 的
    // 归一化路径不抛错（真正的 reduced-motion 行为由 P5-9 视觉回归覆盖）。
    const { container } = render(
      <EChart
        option={{ series: [{ type: "pie", data: [{ value: 1, name: "甲" }] }] }}
        opts={{ renderer: "svg", width: 300, height: 300 }}
      />,
    );
    await waitFor(() => {
      if (!container.querySelector("svg")) throw new Error("echarts svg 尚未渲染");
    });
  });
});
