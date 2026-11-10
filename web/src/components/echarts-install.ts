/** echarts 按需注册安装模块：只被 EChart.tsx 的动态 import 拉起（双层 lazy 的
 *  第二层）。echarts 的 install 用静态具名 import 从 v6 的 typed barrel 取
 *  （lib/export/* 是纯 install 再导出、不在包 sideEffects 名单，可 tree-shake）。
 *  注意不要写成 `const charts = await import("echarts/charts"); charts.BarChart`：
 *  namespace 对象一旦被物化，打包器无法按属性收窄，全家桶会被整段拉回
 *  （本仓实测 1085 kB，改为静态具名 import 后见 build 输出）。 */

import * as echartsCore from "echarts/core";
import { BarChart, PieChart } from "echarts/charts";
import {
  AxisPointerComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from "echarts/components";
import { CanvasRenderer, SVGRenderer } from "echarts/renderers";

/** 注册本项目用到的图表族并返回 core namespace（重复调用无害）。 */
export function ensureInstalled(): typeof echartsCore {
  echartsCore.use([
    BarChart,
    PieChart,
    GridComponent,
    TooltipComponent,
    AxisPointerComponent,
    LegendComponent,
    CanvasRenderer,
    SVGRenderer,
  ]);
  return echartsCore;
}
