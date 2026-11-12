/**
 * 中央多标签工作区（通用组件，P7-1）：与数据无关的文档标签条。
 *
 * 布局与数据模型分离：本组件不 import workspaces.ts、不发请求、不知道
 * RIP 的路由语义。tab 由调用方（Layout）组装：固定 tab（工作区着陆页，
 * pinned 标记）、会话 tab（对象详情/深链接，可关闭）与活跃态全部由 props
 * 传入，回调（onSelect/onClose）也由调用方接导航。
 *
 * 结构约束：标签主体与关闭按钮是兄弟节点（button 不可嵌套 button，
 * 否则 HTML 非法且 axe 报 button-name 相关违规）。pinned tab 无关闭钮。
 */

import type { ReactNode } from "react";

export interface TabAreaTab {
  /** 唯一 id（RIP 里即路径），活跃判定与回调都按 id 走。 */
  id: string;
  label: string;
  /** 已渲染好的图标节点（通用组件不做图标键映射）。 */
  icon?: ReactNode;
  /** 固定 tab：不渲染关闭钮。 */
  pinned?: boolean;
  /** 稳定编号徽标（如 "5.1"，来自 workspaces.ts 的 order 坐标）。 */
  number?: string;
  /** tooltip（缺省用 label）。 */
  title?: string;
}

interface TabAreaProps {
  tabs: readonly TabAreaTab[];
  /** 活跃 tab id（null 表示当前路由不属于任何已开 tab，无高亮）。 */
  activeId: string | null;
  onSelect: (id: string) => void;
  /** 关闭回调（会话 tab 关闭钮调用；固定 tab 永不调用）。 */
  onClose?: (id: string) => void;
  /** aria-label（如「工作区标签页」）。 */
  label: string;
}

export function TabArea({ tabs, activeId, onSelect, onClose, label }: TabAreaProps) {
  if (tabs.length === 0) return null;
  return (
    <nav aria-label={label} className="tabarea">
      <ul className="tabarea-list" role="list">
        {tabs.map((tab) => {
          const active = tab.id === activeId;
          return (
            <li key={tab.id} className={`tabarea-tab${active ? " is-active" : ""}`}>
              <button
                type="button"
                className="tabarea-btn"
                title={tab.title ?? tab.label}
                aria-current={active ? "page" : undefined}
                onClick={() => onSelect(tab.id)}
              >
                {tab.icon ? <span className="tabarea-icon">{tab.icon}</span> : null}
                {tab.number ? <span className="tabarea-no">{tab.number}</span> : null}
                <span className="tabarea-label">{tab.label}</span>
              </button>
              {tab.pinned ? (
                <span className="tabarea-pin" aria-hidden="true" title="固定标签">
                  <PinGlyph />
                </span>
              ) : onClose ? (
                <button
                  type="button"
                  className="tabarea-close"
                  aria-label={`关闭标签 ${tab.label}`}
                  title={`关闭标签 ${tab.label}`}
                  onClick={(event) => {
                    event.stopPropagation();
                    onClose(tab.id);
                  }}
                >
                  <CloseGlyph />
                </button>
              ) : null}
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

/** 内联小图标（避免通用组件依赖 lucide-react，保持零依赖可测）。 */
function PinGlyph(): ReactNode {
  return (
    <svg
      viewBox="0 0 24 24"
      width="12"
      height="12"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 17v5" />
      <path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1z" />
    </svg>
  );
}

function CloseGlyph(): ReactNode {
  return (
    <svg
      viewBox="0 0 24 24"
      width="12"
      height="12"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </svg>
  );
}
