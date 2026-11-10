/** UI primitives：Modal / PageHeader / SkeletonPanel / EmptyPanel / ErrorPanel /
 *  StatusBadge。Modal、PageHeader、SkeletonPanel、EmptyPanel 四个组件按参照实现
 *  admin/src/components/ui.tsx 平移（UI 复用盘点 §3.1：零业务依赖）；ErrorPanel
 *  的 401 文案去 Telegram 化（RIP 是 Bearer 令牌模型）；StatusBadge 泛型化，
 *  label map 与 tone map 由调用方注入，不再绑定问卷状态类型。 */

import { useEffect, useRef, type ReactNode } from "react";
import { AlertTriangle, Inbox, RotateCw, X } from "lucide-react";
import { ApiError } from "../lib/api";
import { trapFocusWithin } from "./focus-trap";

export interface StatusBadgeProps {
  status: string;
  /** 状态到展示文案的映射（调用方注入）；未知状态直接显示原文。 */
  labels?: Record<string, string>;
  /** 状态到徽标色调类的映射（daisyUI badge 修饰类，如 badge-error）。 */
  tones?: Record<string, string>;
}

export function StatusBadge({ status, labels = {}, tones = {} }: StatusBadgeProps) {
  const label = labels[status] ?? (status || "-");
  const tone = tones[status] ?? "badge-ghost";
  return <span className={`badge ${tone}`}>{label}</span>;
}

export function SkeletonPanel({ lines = 4 }: { lines?: number }) {
  const widths = ["w-2/5", "w-11/12", "w-3/4", "w-5/6"];
  return (
    <section className="rounded-surface border border-edge bg-base-100 p-5">
      <div className="skeleton mb-4 h-5 w-2/5" />
      {Array.from({ length: lines }, (_, index) => (
        <div
          key={index}
          className={`skeleton my-3 h-3.5 ${widths[index % widths.length] ?? "w-full"}`}
        />
      ))}
    </section>
  );
}

export function ErrorPanel({ error, onRetry }: { error: ApiError; onRetry: () => void }) {
  const title =
    error.status === 401
      ? "登录状态已过期"
      : error.status === 403
        ? "403 无权访问"
        : error.status === 404
          ? "数据不存在"
          : "加载失败";
  const hint =
    error.status === 401
      ? "访问令牌缺失或已失效，请重新配置访问凭据。"
      : error.message && error.message !== title
        ? error.message
        : "";
  return (
    <section className="rounded-surface border border-edge bg-base-100">
      <div className="flex flex-col items-center gap-3 py-6 text-center">
        <span className="grid h-12 w-12 place-items-center rounded-full bg-error/10 text-error">
          <AlertTriangle className="h-6 w-6" aria-hidden="true" />
        </span>
        <div>
          <h2 className="text-base font-semibold">{title}</h2>
          {hint ? <p className="mt-1 text-sm text-base-content/60">{hint}</p> : null}
        </div>
        <button type="button" className="btn btn-sm" onClick={onRetry}>
          <RotateCw className="h-4 w-4" aria-hidden="true" />
          重试
        </button>
      </div>
    </section>
  );
}

export function EmptyPanel({
  text,
  actionLabel,
  onAction,
  icon,
}: {
  text: string;
  actionLabel?: string;
  onAction?: () => void;
  icon?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-3 rounded-surface border border-edge bg-base-100 py-10 text-center text-sm text-base-content/60">
      <span className="grid h-12 w-12 place-items-center rounded-full bg-base-200 text-base-content/50">
        {icon ?? <Inbox className="h-6 w-6" aria-hidden="true" />}
      </span>
      <div>{text}</div>
      {actionLabel && onAction ? (
        <button type="button" className="btn btn-sm mt-1" onClick={onAction}>
          {actionLabel}
        </button>
      ) : null}
    </div>
  );
}

/**
 * 所有弹层都该用的唯一 Modal。
 *
 * 参照实现此前有四种弹窗并存（两种还依赖构建产物里从不生成的 daisyUI
 * modal-* 类），只有一种响应 Escape 或锁定背景滚动。Modal 把这些行为集中
 * 收口：Escape 关闭、打开期间锁 body 滚动、可聚焦面板打开时接焦点、遮罩
 * 是真 button（可键盘与读屏触达）。参照实现为 Telegram WebView 软键盘做的
 * visualViewport 让位 props 属平台专属，RIP 不带。
 */
export function Modal({
  open,
  onClose,
  title,
  children,
  footer,
  size = "md",
}: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  size?: "sm" | "md" | "lg" | "xl";
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  const restoreFocusRef = useRef<HTMLElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    if (!open) return undefined;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    // 关闭后把焦点还给打开弹窗的按钮（键盘用户不该被丢到页面顶部）。
    restoreFocusRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onCloseRef.current();
    };
    window.addEventListener("keydown", onKeyDown);
    // Move focus into the dialog so Tab does not continue through the page
    // behind it.
    panelRef.current?.focus();
    return () => {
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", onKeyDown);
      restoreFocusRef.current?.focus?.();
    };
  }, [open]);

  if (!open) return null;
  const width =
    size === "sm"
      ? "max-w-sm"
      : size === "lg"
        ? "max-w-2xl"
        : size === "xl"
          ? "max-w-4xl"
          : "max-w-lg";
  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/40 p-4 backdrop-blur-sm"
      role="presentation"
    >
      <button
        type="button"
        className="absolute inset-0 cursor-default"
        aria-label="关闭弹窗"
        onClick={onClose}
      />
      <div
        ref={panelRef}
        className={`relative w-full ${width} rounded-surface border border-edge bg-base-100 shadow-2xl`}
        role="dialog"
        aria-modal="true"
        aria-label={typeof title === "string" ? title : undefined}
        tabIndex={-1}
        // Tab 焦点循环留在面板内（见 components/focus-trap.ts）。
        onKeyDown={(event) => {
          const panel = panelRef.current;
          if (panel) trapFocusWithin(panel, event.nativeEvent);
        }}
      >
        <header className="flex items-center justify-between gap-3 border-b border-edge px-5 py-3">
          <h2 className="text-base font-semibold">{title}</h2>
          <button
            type="button"
            className="btn btn-circle btn-ghost btn-sm"
            onClick={onClose}
            aria-label="关闭"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </header>
        <div className="max-h-[70vh] overflow-auto px-5 py-4">{children}</div>
        {footer ? <footer className="border-t border-edge-soft px-5 py-3">{footer}</footer> : null}
      </div>
    </div>
  );
}

export function PageHeader({ title, actions }: { title: ReactNode; actions?: ReactNode }) {
  return (
    <header className="mb-6 flex items-center justify-between gap-3">
      <h1 className="text-xl font-bold tracking-tight">{title}</h1>
      {actions ? <div className="flex items-center gap-2">{actions}</div> : null}
    </header>
  );
}
