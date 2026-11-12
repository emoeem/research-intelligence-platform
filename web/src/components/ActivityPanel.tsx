/**
 * 底部 Activity 面板（通用组件，P7-1，计划 §10.2）：与数据无关的活动面板。
 *
 * 布局与数据模型分离：本组件不 import workspaces.ts、不发请求。任务队列、
 * Tool Runs、日志、输出、状态等页签由调用方以 tabs 传入（id + 标题 +
 * 已渲染好的 body 节点），活跃态与切换回调也由 props 控制。折叠行为由
 * props.open 控制（关闭即不渲染，展开入口在顶部栏，单一控制源）。
 */

import type { ReactNode } from "react";

export interface ActivityTab {
  id: string;
  label: string;
  /** 已渲染好的面板内容。 */
  body: ReactNode;
}

interface ActivityPanelProps {
  open: boolean;
  tabs: readonly ActivityTab[];
  activeId: string;
  onSelect: (id: string) => void;
  /** aria-label（如「Activity 面板」）。 */
  label: string;
}

export function ActivityPanel({ open, tabs, activeId, onSelect, label }: ActivityPanelProps) {
  if (!open || tabs.length === 0) return null;
  const active = tabs.find((tab) => tab.id === activeId) ?? tabs[0]!;
  return (
    <section className="activity" aria-label={label}>
      <div className="activity-tabs" role="tablist" aria-label={`${label}页签`}>
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={tab.id === active.id}
            className={`activity-tab${tab.id === active.id ? " is-active" : ""}`}
            onClick={() => onSelect(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <div className="activity-body" role="tabpanel" aria-label={active.label}>
        {active.body}
      </div>
    </section>
  );
}
