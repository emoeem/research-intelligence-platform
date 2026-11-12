/**
 * 右侧 Inspector（通用组件，P7-1，计划 §10.2）：与数据无关的属性面板。
 *
 * 布局与数据模型分离：本组件不 import workspaces.ts、不发请求。展示什么
 * （当前对象属性、关联知识/工具、运行配置）由调用方以 sections 传入，
 * 每个 section 是 id + 标题 + 已渲染好的 body 节点。折叠行为由 props.open
 * 控制（关闭即不渲染，展开入口在顶部栏，单一控制源）。
 */

import type { ReactNode } from "react";

export interface InspectorSection {
  id: string;
  title: string;
  body: ReactNode;
}

interface InspectorPanelProps {
  open: boolean;
  sections: readonly InspectorSection[];
  /** aria-label（如「Inspector」）。 */
  label: string;
}

export function InspectorPanel({ open, sections, label }: InspectorPanelProps) {
  if (!open || sections.length === 0) return null;
  return (
    <aside className="inspector" aria-label={label}>
      <h2 className="panel-title">{label}</h2>
      <div className="inspector-body">
        {sections.map((section) => (
          <section key={section.id} className="inspector-section" aria-label={section.title}>
            <h3 className="inspector-section-title">{section.title}</h3>
            {section.body}
          </section>
        ))}
      </div>
    </aside>
  );
}
