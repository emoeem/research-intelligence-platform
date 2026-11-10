/** EChart 公共 prop 类型（与旧绑定层的调用面兼容）。独立文件避免
 *  EChart.tsx 在类型位引用 echarts-for-react 残留。 */

export interface EChartsReactProps {
  option: unknown;
  style?: import("react").CSSProperties;
  className?: string;
  notMerge?: boolean;
  opts?: { renderer?: "canvas" | "svg"; width?: number | "auto"; height?: number | "auto" };
  onEvents?: Record<string, (params: unknown) => void>;
}
