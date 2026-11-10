import { beforeEach, describe, expect, it, vi } from "vitest";
import { trapFocusWithin } from "../src/components/focus-trap";

/** P5-2 基础件包：焦点陷阱 Tab 首尾循环用例。
 *  jsdom 无布局引擎，getClientRects 恒为空；按真实语义打桩：带 hidden 属性的
 *  元素视为不可见，其余可聚焦。 */

function makePanel(children: "buttons" | "hidden-button" | "none"): HTMLDivElement {
  const panel = document.createElement("div");
  panel.tabIndex = -1;
  if (children === "buttons") {
    for (const label of ["甲", "乙", "丙"]) {
      const button = document.createElement("button");
      button.textContent = label;
      panel.appendChild(button);
    }
  }
  if (children === "hidden-button") {
    const button = document.createElement("button");
    button.hidden = true;
    panel.appendChild(button);
  }
  document.body.appendChild(panel);
  return panel;
}

function buttonsOf(panel: HTMLDivElement): HTMLButtonElement[] {
  return Array.from(panel.querySelectorAll("button"));
}

function tabEvent(shiftKey = false): KeyboardEvent {
  return new KeyboardEvent("keydown", { key: "Tab", shiftKey, bubbles: true, cancelable: true });
}

describe("components/focus-trap（P5-2）", () => {
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, "getClientRects").mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.hidden ? ([] as unknown as DOMRectList) : ([{}] as unknown as DOMRectList);
    });
    document.body.innerHTML = "";
  });

  it("Tab 在最后一个元素时回到第一个", () => {
    const panel = makePanel("buttons");
    const buttons = buttonsOf(panel);
    buttons[2]!.focus();
    trapFocusWithin(panel, tabEvent());
    expect(document.activeElement).toBe(buttons[0]);
  });

  it("Tab 在中间元素时正常前进（不拦截）", () => {
    const panel = makePanel("buttons");
    const buttons = buttonsOf(panel);
    buttons[0]!.focus();
    const event = tabEvent();
    trapFocusWithin(panel, event);
    expect(event.defaultPrevented).toBe(false);
    expect(document.activeElement).toBe(buttons[0]);
  });

  it("Shift+Tab 在第一个元素时跳到最后一个", () => {
    const panel = makePanel("buttons");
    const buttons = buttonsOf(panel);
    buttons[0]!.focus();
    trapFocusWithin(panel, tabEvent(true));
    expect(document.activeElement).toBe(buttons[2]);
  });

  it("焦点在面板外时，Tab 拉回首元素", () => {
    const panel = makePanel("buttons");
    const buttons = buttonsOf(panel);
    const outside = document.createElement("button");
    document.body.appendChild(outside);
    outside.focus();
    expect(panel.contains(document.activeElement)).toBe(false);
    trapFocusWithin(panel, tabEvent());
    expect(document.activeElement).toBe(buttons[0]);
  });

  it("面板内无可聚焦元素时吞掉 Tab 并聚焦面板", () => {
    const panel = makePanel("hidden-button");
    const event = tabEvent();
    trapFocusWithin(panel, event);
    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(panel);
  });

  it("非 Tab 按键直接忽略", () => {
    const panel = makePanel("buttons");
    const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
    trapFocusWithin(panel, event);
    expect(event.defaultPrevented).toBe(false);
  });
});
