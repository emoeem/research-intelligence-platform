import { Link } from "react-router";
import { PageHeader } from "../components/ui";

/**
 * 工作区占位页（P7-1 共用）：壳层升级批只为十一工作区信息架构落导航与路由，
 * 未实施批次的页面先给诚实占位（标题 + 规划要点 + 填充批次标注），
 * 不放假数据、不猜语义。对应批次落页后直接替换本组件。
 */

interface WorkspacePlaceholderProps {
  title: string;
  /** 工作区稳定编号（如 "7"），与 workspaces.ts 的 order 坐标一致。 */
  workspaceNumber: string;
  /** 规划要点（来自 2.0 计划 §10.3/§10.4 的页面设计，一行一条）。 */
  planItems: readonly string[];
  /** 填充批次标注（如「P7-7」）。 */
  fillsIn: string;
  /** 可选的当前可用替代入口。 */
  related?: ReadonlyArray<{ to: string; label: string }>;
}

export function WorkspacePlaceholder({
  title,
  workspaceNumber,
  planItems,
  fillsIn,
  related = [],
}: WorkspacePlaceholderProps) {
  return (
    <div>
      <PageHeader
        title={title}
        actions={<span className="badge badge-ghost">工作区 {workspaceNumber} · 占位</span>}
      />
      <div className="placeholder-panel">
        <p className="placeholder-note">
          本工作区是壳层升级批（P7-1）落的信息架构占位页，页面功能由{" "}
          <strong data-testid="fills-in">{fillsIn}</strong> 批次实施。 以下规划要点来自 2.0 计划
          §10.3/§10.4：
        </p>
        <ul className="placeholder-plan">
          {planItems.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
        {related.length > 0 ? (
          <div className="placeholder-related">
            <span className="placeholder-related-label">当前可用入口：</span>
            {related.map((item) => (
              <Link key={item.to} to={item.to} className="placeholder-link">
                {item.label}
              </Link>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}
