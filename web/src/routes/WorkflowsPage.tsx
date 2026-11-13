import { lazy } from "react";

/** P8-4 bundle 探针（开工前实测，实施后即替换为实页）：懒加载 @xyflow/react
 *  空画布 + 官方样式表，测量 vendor-xyflow 分组的体积影响。 */
const ProbeCanvas = lazy(async () => {
  await import("@xyflow/react/dist/style.css");
  const { ReactFlow } = await import("@xyflow/react");
  function Probe() {
    return <div style={{ width: 600, height: 400 }}><ReactFlow nodes={[]} edges={[]} /></div>;
  }
  return { default: Probe };
});

export function WorkflowsPage() {
  return (
    <div className="space-y-4">
      <h2 className="text-sm font-semibold">Workflow Studio</h2>
      <ProbeCanvas />
    </div>
  );
}
