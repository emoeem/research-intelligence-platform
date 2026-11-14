/** Dialog 与 Toast：useDialogs() 提供 promise 化的 confirm / alert / toast。
 *  整体沿自参照实现 admin/src/components/Dialogs.tsx（UI 复用盘点 §3.2），
 *  零业务依赖，中文默认文案保留（RIP 同为中文产品）。工程细节全部是事故
 *  教训，解耦时必须一起带走（见下方四处注释）。 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { AlertTriangle, CheckCircle2, CircleX, Info } from "lucide-react";
import { trapFocusWithin } from "./focus-trap";

export type DialogVariant = "default" | "danger";

export interface ConfirmOptions {
  title?: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  variant?: DialogVariant;
}

/**
 * 单按钮对话框，用于用户只能确认知悉的陈述（「无法保存，这是 3 条问题」）。
 * 它存在的原因：两键 confirm 会逼每个这类调用方编造一个假第二选项，这正是
 * 「返回修改 / 知道了」变成两个完全一样按钮的来源。
 */
export interface AlertOptions {
  title?: string;
  message: string;
  buttonLabel?: string;
  variant?: DialogVariant;
}

export interface ToastAction {
  label: string;
  onClick: () => void | Promise<void>;
}

export interface ToastOptions {
  message: string;
  variant?: "success" | "error" | "info";
  durationMs?: number;
  action?: ToastAction;
  countdownMs?: number;
}

interface InternalConfirmItem extends ConfirmOptions {
  id: number;
  kind: "confirm";
  resolve: (ok: boolean) => void;
}

interface InternalAlertItem extends AlertOptions {
  id: number;
  kind: "alert";
  resolve: () => void;
}

/** 单队列：alert 与 confirm 永不叠层。 */
type InternalDialogItem = InternalConfirmItem | InternalAlertItem;

interface InternalToastItem extends ToastOptions {
  id: number;
}

interface DialogsContextValue {
  confirm: (options: ConfirmOptions) => Promise<boolean>;
  alert: (options: AlertOptions) => Promise<void>;
  toast: (options: ToastOptions) => void;
}

const DialogsContext = createContext<DialogsContextValue | null>(null);

export function useDialogs(): DialogsContextValue {
  const ctx = useContext(DialogsContext);
  if (!ctx) throw new Error("useDialogs must be used within <DialogsProvider>");
  return ctx;
}

let nextId = 1;

export function DialogsProvider({ children }: { children: ReactNode }) {
  const [dialogs, setDialogs] = useState<InternalDialogItem[]>([]);
  const [toasts, setToasts] = useState<InternalToastItem[]>([]);
  const timersRef = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  const confirm = useCallback((options: ConfirmOptions): Promise<boolean> => {
    return new Promise<boolean>((resolve) => {
      setDialogs((prev) => [...prev, { id: nextId++, kind: "confirm", ...options, resolve }]);
    });
  }, []);

  const alert = useCallback((options: AlertOptions): Promise<void> => {
    return new Promise<void>((resolve) => {
      setDialogs((prev) => [...prev, { id: nextId++, kind: "alert", ...options, resolve }]);
    });
  }, []);

  const toast = useCallback((options: ToastOptions): void => {
    const duration = options.durationMs ?? (options.variant === "error" ? 5000 : 3000);
    const id = nextId++;
    setToasts((prev) => [...prev, { id, ...options }]);
    const timer = setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
      timersRef.current.delete(id);
    }, duration);
    timersRef.current.set(id, timer);
  }, []);

  // state updater 必须保持纯函数：React 可能丢弃或重放 updater（StrictMode 会
  // 双调用），把 promise resolve 放在 updater 里曾让调用方的 `await confirm()`
  // 挂起（参照实现事故，Dialogs.tsx:104-107）。
  const dialogsRef = useRef<InternalDialogItem[]>([]);
  dialogsRef.current = dialogs;

  const closeDialog = useCallback((id: number, ok: boolean) => {
    const item = dialogsRef.current.find((d) => d.id === id);
    setDialogs((prev) => prev.filter((d) => d.id !== id));
    if (!item) return;
    if (item.kind === "confirm") item.resolve(ok);
    else item.resolve();
  }, []);

  useEffect(() => {
    return () => {
      timersRef.current.forEach((t) => clearTimeout(t));
      timersRef.current.clear();
      // provider 卸载（路由切换、HMR）时，未决 dialog 若不兜底 resolve，
      // 调用方会永久挂起（参照实现事故，Dialogs.tsx:118-129）。
      dialogsRef.current.forEach((item) => {
        if (item.kind === "confirm") item.resolve(false);
        else item.resolve();
      });
    };
  }, []);

  return (
    <DialogsContext.Provider value={{ confirm, alert, toast }}>
      {children}
      <DialogStack items={dialogs} onClose={closeDialog} />
      <ToastStack items={toasts} />
    </DialogsContext.Provider>
  );
}

/**
 * Escape 关闭；打开时焦点进入弹窗，关闭时焦点回到之前的位置。两者都曾缺失：
 * 弹窗对键盘用户是纯鼠标件，关闭后焦点掉到 body 上。
 */
function useDialogFocus(id: number, onEscape: () => void) {
  const panelRef = useRef<HTMLDivElement>(null);
  const restoreRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    restoreRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    return () => restoreRef.current?.focus?.();
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onEscape();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [id, onEscape]);

  return panelRef;
}

function DialogStack({
  items,
  onClose,
}: {
  items: InternalDialogItem[];
  onClose: (id: number, ok: boolean) => void;
}) {
  if (items.length === 0) return null;
  const top = items[items.length - 1] as InternalDialogItem;
  const variant = top.variant ?? "default";
  const danger = variant === "danger";
  const isAlert = top.kind === "alert";
  const title = top.title ?? (isAlert ? "提示" : danger ? "确认操作" : "提示");
  const message = top.message;

  return (
    <>
      {items.slice(0, -1).map((c) => (
        <div key={c.id} className="pointer-events-none fixed inset-0 z-[100]" aria-hidden="true" />
      ))}
      <div className="fixed inset-0 z-[100] grid place-items-center bg-black/40 px-4 backdrop-blur-sm">
        <DialogPanel
          id={top.id}
          isAlert={isAlert}
          danger={danger}
          title={title}
          message={message}
          closeLabel={top.kind === "alert" ? (top.buttonLabel ?? "知道了") : undefined}
          confirmLabel={
            top.kind === "confirm" ? (top.confirmLabel ?? (danger ? "确认" : "确定")) : undefined
          }
          cancelLabel={top.kind === "confirm" ? (top.cancelLabel ?? "取消") : undefined}
          onClose={(ok) => onClose(top.id, ok)}
        />
      </div>
    </>
  );
}

function DialogPanel({
  id,
  isAlert,
  danger,
  title,
  message,
  confirmLabel,
  cancelLabel,
  closeLabel,
  onClose,
}: {
  id: number;
  isAlert: boolean;
  danger: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  closeLabel?: string;
  onClose: (ok: boolean) => void;
}) {
  const panelRef = useDialogFocus(id, () => onClose(false));
  const okRef = useRef<HTMLButtonElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);

  // confirm 初始焦点落在取消键（Enter 不能误触危险默认）；alert 只有一个
  // 动作，焦点给它（参照实现事故，Dialogs.tsx:227-229）。
  useEffect(() => {
    (isAlert ? okRef : cancelRef).current?.focus();
  }, [isAlert]);

  const titleId = `dialog-title-${id}`;
  const descId = `dialog-desc-${id}`;

  return (
    <div
      ref={panelRef}
      role={isAlert ? "alertdialog" : "dialog"}
      aria-modal="true"
      aria-labelledby={titleId}
      aria-describedby={descId}
      // Tab 焦点循环留在面板内（见 components/focus-trap.ts）。
      onKeyDown={(event) => trapFocusWithin(panelRef.current, event.nativeEvent)}
      className="w-full max-w-sm overflow-hidden rounded-surface border border-edge bg-base-100 shadow-2xl"
    >
      <div className="flex items-start gap-3 p-5">
        <div
          className={`grid h-10 w-10 shrink-0 place-items-center rounded-control ${
            danger ? "bg-error/10 text-error" : "bg-primary/10 text-primary"
          }`}
        >
          <AlertTriangle className="h-5 w-5" aria-hidden="true" />
        </div>
        <div className="min-w-0 flex-1">
          <h3 id={titleId} className="text-base font-bold tracking-tight">
            {title}
          </h3>
          <p id={descId} className="mt-1 whitespace-pre-wrap text-sm leading-6 text-muted">
            {message}
          </p>
        </div>
      </div>
      <div className="flex border-t border-edge-soft">
        {isAlert ? (
          <button
            ref={okRef}
            type="button"
            onClick={() => onClose(true)}
            className={`flex-1 py-3 text-sm font-bold transition-colors ${
              danger ? "text-error hover:bg-error/10" : "text-primary hover:bg-primary/10"
            }`}
          >
            {closeLabel}
          </button>
        ) : (
          <>
            <button
              ref={cancelRef}
              type="button"
              onClick={() => onClose(false)}
              className="flex-1 border-r border-edge-soft py-3 text-sm font-semibold text-muted transition-colors hover:bg-base-200"
            >
              {cancelLabel}
            </button>
            <button
              ref={okRef}
              type="button"
              onClick={() => onClose(true)}
              className={`flex-1 py-3 text-sm font-bold transition-colors ${
                danger ? "text-error hover:bg-error/10" : "text-primary hover:bg-primary/10"
              }`}
            >
              {confirmLabel}
            </button>
          </>
        )}
      </div>
    </div>
  );
}

function ToastStack({ items }: { items: InternalToastItem[] }) {
  if (items.length === 0) return null;
  return (
    <div className="pointer-events-none fixed inset-x-0 top-4 z-[101] flex flex-col items-center gap-2 px-4">
      {items.map((t) => (
        <ToastItem key={t.id} item={t} />
      ))}
    </div>
  );
}

function ToastItem({ item }: { item: InternalToastItem }) {
  const variant = item.variant ?? "info";
  const [remainingMs, setRemainingMs] = useState(item.countdownMs ?? 0);

  useEffect(() => {
    if (!item.countdownMs) return undefined;
    const startedAt = Date.now();
    const timer = setInterval(() => {
      setRemainingMs(Math.max(0, item.countdownMs! - (Date.now() - startedAt)));
    }, 250);
    return () => clearInterval(timer);
  }, [item.countdownMs]);
  const Icon = variant === "success" ? CheckCircle2 : variant === "error" ? CircleX : Info;
  // 用主题 token 而非 `dark:` 变体：dark: 跟随系统配色，应用其余部分跟随
  // 选定的 daisyUI 主题，「浅色应用 + 深色系统」时每个 toast 都会反色
  // （参照实现事故，Dialogs.tsx:330-340 注释）。
  const border =
    variant === "success"
      ? "border-s-[3px] border-s-success"
      : variant === "error"
        ? "border-s-[3px] border-s-error"
        : "border-s-[3px] border-s-info";
  const color = `border-edge bg-base-100 text-base-content ${border}`;
  const iconColor =
    variant === "success" ? "text-success" : variant === "error" ? "text-error" : "text-info";

  return (
    <div
      role="status"
      className={`pointer-events-auto flex min-w-[260px] max-w-md items-center gap-2 rounded-control border px-4 py-3 text-sm font-medium shadow-lg ${color}`}
    >
      <Icon className={`h-4 w-4 shrink-0 ${iconColor}`} aria-hidden="true" />
      <span className="min-w-0 flex-1 break-words">{item.message}</span>
      {item.action ? (
        <button
          type="button"
          className="shrink-0 rounded-control px-2.5 py-1.5 text-xs font-bold text-primary hover:bg-primary/10"
          onClick={() => void item.action?.onClick()}
        >
          {item.action.label}
          {item.countdownMs ? ` · ${Math.ceil(remainingMs / 1000)}s` : ""}
        </button>
      ) : null}
    </div>
  );
}
