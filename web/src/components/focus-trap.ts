/**
 * 模态框焦点陷阱（WAI-ARIA dialog pattern）。
 *
 * 背景：Modal / DialogPanel 打开时只把焦点移进面板，没有拦 Tab，焦点走到
 * 面板最后一个可聚焦元素后会继续跑进背后的页面，键盘用户（以及读屏器）
 * 会「看不到打开的是哪个弹窗」而迷失。这里补上标准的首尾循环：
 *
 *   - Tab 在最后一个元素 → 回到第一个；
 *   - Shift+Tab 在第一个元素（或焦点已跑到面板外）→ 跳到最后一个；
 *   - 面板内没有可聚焦元素时吞掉 Tab，焦点留在面板上。
 *
 * 只处理 Tab；Escape 由调用方各自处理（Modal 走 window keydown、Dialogs 走
 * useDialogFocus）。刻意不用 focusin 把逃出去的焦点拽回来，那会与浏览器的
 * 地址栏/开发者工具切换抢焦点。
 */
const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "textarea:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

/** 只算「真的能 Tab 到」的元素：display:none / aria-hidden 的容器排除。 */
function focusableWithin(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    // getClientRects 判可见性：offsetParent 对 position:fixed 容器里的元素
    // 可能是 null，会把整个弹窗里的控件误判成隐藏。
    (element) => element.getClientRects().length > 0,
  );
}

/** 在面板的 onKeyDown 里调用：`trapFocusWithin(panel, event.nativeEvent)`。 */
export function trapFocusWithin(container: HTMLElement | null, event: KeyboardEvent): void {
  if (!container || event.key !== "Tab") return;
  const focusables = focusableWithin(container);
  if (focusables.length === 0) {
    event.preventDefault();
    container.focus();
    return;
  }
  const first = focusables[0] as HTMLElement;
  const last = focusables[focusables.length - 1] as HTMLElement;
  const active = document.activeElement as HTMLElement | null;
  const inside = active !== null && container.contains(active);

  if (event.shiftKey) {
    if (!inside || active === first) {
      event.preventDefault();
      last.focus();
    }
    return;
  }
  if (!inside || active === last) {
    event.preventDefault();
    first.focus();
  }
}
