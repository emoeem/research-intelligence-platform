import { PagePlaceholder } from "./PagePlaceholder";

/** Runtime 页占位：VM 在线状态、心跳、Podman 版本与镜像 digest（Phase 2）由 P5-7 批次实现。 */
export function RuntimePage() {
  return (
    <PagePlaceholder
      title="运行时"
      description="VM 在线状态、心跳与容器运行环境信息。"
      batch="P5-7"
    />
  );
}
