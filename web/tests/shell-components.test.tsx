import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ActivityPanel, type ActivityTab } from "../src/components/ActivityPanel";
import { InspectorPanel, type InspectorSection } from "../src/components/InspectorPanel";
import { TabArea, type TabAreaTab } from "../src/components/TabArea";

/** P7-1：TabArea / InspectorPanel / ActivityPanel 是与数据无关的通用组件。
 *  这里只用假数据驱动三个组件，验证布局与数据模型分离：组件不认识 RIP 的
 *  workspaces.ts，一切 tab/分区/页签内容都来自 props。 */

const TABS: TabAreaTab[] = [
  { id: "/a", label: "固定甲", pinned: true, number: "1.1", title: "固定甲提示" },
  { id: "/b", label: "会话乙" },
  { id: "/c", label: "会话丙" },
];

afterEach(() => {
  document.body.innerHTML = "";
});

describe("TabArea（通用多标签条）", () => {
  it("渲染标签与编号徽标，活跃 tab 带 aria-current", () => {
    render(
      <TabArea
        tabs={TABS}
        activeId="/b"
        onSelect={vi.fn()}
        onClose={vi.fn()}
        label="工作区标签页"
      />,
    );
    const nav = screen.getByRole("navigation", { name: "工作区标签页" });
    expect(nav.textContent).toContain("1.1");
    expect(nav.textContent).toContain("会话乙");
    const activeBtn = screen.getByRole("button", { name: "会话乙" });
    expect(activeBtn.getAttribute("aria-current")).toBe("page");
  });

  it("pinned 标签无关闭钮，非 pinned 标签有关闭钮", () => {
    render(
      <TabArea
        tabs={TABS}
        activeId="/b"
        onSelect={vi.fn()}
        onClose={vi.fn()}
        label="工作区标签页"
      />,
    );
    expect(screen.queryByRole("button", { name: "关闭标签 固定甲" })).toBeNull();
    expect(screen.getByRole("button", { name: "关闭标签 会话乙" })).toBeTruthy();
  });

  it("onSelect / onClose 回调带 tab id", () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    render(
      <TabArea
        tabs={TABS}
        activeId="/b"
        onSelect={onSelect}
        onClose={onClose}
        label="工作区标签页"
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "会话丙" }));
    expect(onSelect).toHaveBeenCalledWith("/c");
    fireEvent.click(screen.getByRole("button", { name: "关闭标签 会话乙" }));
    expect(onClose).toHaveBeenCalledWith("/b");
  });

  it("没有 tabs 时整体不渲染", () => {
    const { container } = render(
      <TabArea tabs={[]} activeId={null} onSelect={vi.fn()} label="工作区标签页" />,
    );
    expect(container.querySelector(".tabarea")).toBeNull();
  });
});

describe("InspectorPanel（通用右侧面板）", () => {
  const SECTIONS: InspectorSection[] = [
    { id: "s1", title: "当前对象", body: <p>甲对象属性</p> },
    { id: "s2", title: "关联", body: <p>乙关联条目</p> },
  ];

  it("open 时渲染分区标题与内容", () => {
    render(<InspectorPanel open sections={SECTIONS} label="Inspector" />);
    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeTruthy();
    expect(screen.getByText("当前对象")).toBeTruthy();
    expect(screen.getByText("甲对象属性")).toBeTruthy();
    expect(screen.getByText("乙关联条目")).toBeTruthy();
  });

  it("关闭时不渲染", () => {
    const { container } = render(
      <InspectorPanel open={false} sections={SECTIONS} label="Inspector" />,
    );
    expect(container.querySelector(".inspector")).toBeNull();
  });
});

describe("ActivityPanel（通用底部面板）", () => {
  const TABS: ActivityTab[] = [
    { id: "q", label: "队列", body: <p>队列内容</p> },
    { id: "l", label: "日志", body: <p>日志内容</p> },
  ];

  it("open 时渲染页签与活跃页签内容，切换回调生效", () => {
    const onSelect = vi.fn();
    const { rerender } = render(
      <ActivityPanel open tabs={TABS} activeId="q" onSelect={onSelect} label="Activity 面板" />,
    );
    expect(screen.getByRole("region", { name: "Activity 面板" })).toBeTruthy();
    expect(screen.getByText("队列内容")).toBeTruthy();
    expect(screen.queryByText("日志内容")).toBeNull();
    fireEvent.click(screen.getByRole("tab", { name: "日志" }));
    expect(onSelect).toHaveBeenCalledWith("l");
    rerender(
      <ActivityPanel open tabs={TABS} activeId="l" onSelect={onSelect} label="Activity 面板" />,
    );
    expect(screen.getByText("日志内容")).toBeTruthy();
  });

  it("关闭时不渲染", () => {
    const { container } = render(
      <ActivityPanel
        open={false}
        tabs={TABS}
        activeId="q"
        onSelect={vi.fn()}
        label="Activity 面板"
      />,
    );
    expect(container.querySelector(".activity")).toBeNull();
  });
});
