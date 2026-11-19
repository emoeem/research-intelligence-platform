import { Link } from "react-router";
import { WorkspacePlaceholder } from "./WorkspacePlaceholder";

/** 设置工作区（占位，P7-1）：界面偏好与系统设置由后续批次实施
 *  （计划 §10.3 System）。主题切换已在图标轨底部可用，本批不重复入口。 */
export function SettingsPage() {
  return (
    <div>
      <WorkspacePlaceholder
        title="设置"
        workspaceNumber="11"
        fillsIn="后续批次"
        planItems={["界面偏好（主题、密度、语言）", "系统设置（运行时、集成、账号）"]}
        related={[
          { to: "/runtime", label: "运行时状态" },
          { to: "/settings/wsw-proxy", label: "WSW 代理状态（WSW-2）" },
          { to: "/settings/wsw-cert", label: "WSW 证书状态（WSW-2）" },
        ]}
      />
      <p className="muted">
        WSW-2 最小面：<Link to="/settings/wsw-proxy">代理状态</Link> ·{" "}
        <Link to="/settings/wsw-cert">证书状态</Link>
      </p>
    </div>
  );
}
