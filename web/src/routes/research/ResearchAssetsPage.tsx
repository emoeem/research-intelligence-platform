/** Research Assets（P5-1 骨架版，Phase 5 §4.5.D）：资产域（Asset/Endpoint/
 *  Technology Profile）随 P5-2 领域契约与 P5-5 Provider 批次落地；本版只渲染
 *  真实空态与来源说明，不伪造资产行、不凭域名推测技术栈（规格 §4.5.D 硬性
 *  要求：资产信息必须来自真实发现或导入记录）。 */

import type { ReactNode } from "react";
import { useApi } from "../../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../../components/ui";

/** 资产域空态说明（诚实口径：哪一表/哪一端点、何时出现）。 */
function AssetsPending(): ReactNode {
  return (
    <section className="rounded-surface border border-edge bg-base-100 p-5">
      <h2 className="text-base font-semibold">资产域尚未落库</h2>
      <ul className="mt-3 list-disc space-y-1.5 pl-5 text-sm text-muted">
        <li>资产身份（Host/Scheme/Port/URL 规范化）与端点域随 P5-2 验证契约批次定义。</li>
        <li>资产观测（发现来源、技术画像证据）随 P5-5 Provider 批次与本地靶场执行产生。</li>
        <li>本页届时改为 /api/research/assets 真实列表（服务端分页 + 过滤），不沿用占位。</li>
      </ul>
    </section>
  );
}

export function ResearchAssetsPage() {
  // 任务面计数作为资产相关性的真实锚（目标字段 target 来自 RTIS 任务镜像），
  // 用于说明「资产数据将来自既有任务的 target 与未来发现面」。
  const tasks = useApi<{ total?: number }>("/api/tasks?limit=1");

  return (
    <div>
      <PageHeader
        title="资产"
        actions={<span className="badge badge-ghost">资产域：随 P5-2/P5-5 交付</span>}
      />
      {tasks.loading ? (
        <SkeletonPanel lines={4} />
      ) : tasks.error !== null ? (
        <ErrorPanel error={tasks.error} onRetry={tasks.retry} />
      ) : (
        <>
          <p className="mb-4 text-sm text-muted">
            既有研究任务（RTIS 镜像）共 {tasks.data?.total ?? 0} 条，其 target 字段是
            现阶段唯一的目标事实来源；结构化资产身份与画像随验证契约批次建立。
          </p>
          <EmptyPanel text="暂无结构化资产记录（资产域未落库，见下方说明）。" />
          <div className="mt-4">
            <AssetsPending />
          </div>
        </>
      )}
    </div>
  );
}
