import type { ReactNode } from "react";
import { useUiText } from "../../i18n";
import {
  AlertTriangle,
  WifiOff,
  LockKeyhole,
  Info,
  CheckCircle2,
  LoaderCircle,
  CircleAlert,
  Inbox,
} from "lucide-react";

function Panel({
  children,
  tone = "neutral",
  role = "status",
}: {
  children: ReactNode;
  tone?: "neutral" | "warning" | "error" | "success";
  role?: "status" | "alert";
}) {
  const toneClass =
    tone === "error"
      ? "border-error/40 bg-error/5"
      : tone === "warning"
        ? "border-warning/40 bg-warning/5"
        : tone === "success"
          ? "border-success/40 bg-success/5"
          : "border-edge bg-base-100";
  return (
    <section
      className={`rounded-surface border p-4 ${toneClass}`}
      role={role}
      aria-live={role === "alert" ? "assertive" : "polite"}
    >
      {children}
    </section>
  );
}
export function LoadingState({ label = "正在加载", detail }: { label?: string; detail?: string }) {
  const ui = useUiText();
  return (
    <Panel>
      <div className="flex items-center gap-3">
        <LoaderCircle
          className="h-4 w-4 animate-spin motion-reduce:animate-none"
          aria-hidden="true"
        />
        <div>
          <p className="text-sm font-medium">{ui(label)}</p>
          {detail && <p className="text-xs text-muted">{ui(detail)}</p>}
        </div>
      </div>
    </Panel>
  );
}
export function SuccessState({ label, children }: { label: string; children?: ReactNode }) {
  const ui = useUiText();
  return (
    <Panel tone="success">
      <div className="flex items-center gap-2">
        <CheckCircle2 className="h-4 w-4" aria-hidden="true" />
        <p className="text-sm font-medium">{ui(label)}</p>
      </div>
      {children}
    </Panel>
  );
}
export function EmptyState({
  label = "暂无记录",
  detail,
  action,
}: {
  label?: string;
  detail?: string;
  action?: ReactNode;
}) {
  const ui = useUiText();
  return (
    <Panel>
      <div className="flex flex-col items-center gap-2 py-5 text-center">
        <Inbox className="h-5 w-5 text-muted" aria-hidden="true" />
        <p className="text-sm font-medium">{ui(label)}</p>
        {detail && <p className="max-w-prose text-xs text-muted">{ui(detail)}</p>}
        {action}
      </div>
    </Panel>
  );
}
export function ErrorState({
  label = "请求失败",
  detail,
  onRetry,
}: {
  label?: string;
  detail?: string;
  onRetry?: () => void;
}) {
  const ui = useUiText();
  return (
    <Panel tone="error" role="alert">
      <div className="flex items-start gap-2">
        <CircleAlert className="h-4 w-4" aria-hidden="true" />
        <div className="flex-1">
          <p className="text-sm font-medium">{ui(label)}</p>
          {detail && <p className="text-xs text-muted">{ui(detail)}</p>}
          {onRetry && (
            <button type="button" className="btn btn-xs mt-2" onClick={onRetry}>
              {ui("重试")}
            </button>
          )}
        </div>
      </div>
    </Panel>
  );
}
export function UnauthorizedState({
  label = "当前凭据无权访问此资源",
  detail,
}: {
  label?: string;
  detail?: string;
}) {
  const ui = useUiText();
  return (
    <Panel tone="warning" role="alert">
      <div className="flex gap-2">
        <LockKeyhole className="h-4 w-4" aria-hidden="true" />
        <div>
          <p className="text-sm font-medium">{ui(label)}</p>
          {detail && <p className="text-xs text-muted">{ui(detail)}</p>}
        </div>
      </div>
    </Panel>
  );
}
export function ReadOnlyState({ label = "只读模式", detail }: { label?: string; detail?: string }) {
  const ui = useUiText();
  return (
    <Panel>
      <div className="flex gap-2">
        <Info className="h-4 w-4" aria-hidden="true" />
        <div>
          <p className="text-sm font-medium">{ui(label)}</p>
          {detail && <p className="text-xs text-muted">{ui(detail)}</p>}
        </div>
      </div>
    </Panel>
  );
}
export function PartialDataState({
  label = "部分数据可用",
  detail,
}: {
  label?: string;
  detail?: string;
}) {
  const ui = useUiText();
  return (
    <Panel tone="warning">
      <div className="flex gap-2">
        <AlertTriangle className="h-4 w-4" aria-hidden="true" />
        <div>
          <p className="text-sm font-medium">{ui(label)}</p>
          {detail && <p className="text-xs text-muted">{ui(detail)}</p>}
        </div>
      </div>
    </Panel>
  );
}
export function OfflineState({
  label = "服务暂时不可用",
  detail,
  onRetry,
}: {
  label?: string;
  detail?: string;
  onRetry?: () => void;
}) {
  const ui = useUiText();
  return (
    <Panel tone="warning">
      <div className="flex items-start gap-2">
        <WifiOff className="h-4 w-4" aria-hidden="true" />
        <div className="flex-1">
          <p className="text-sm font-medium">{ui(label)}</p>
          {detail && <p className="text-xs text-muted">{ui(detail)}</p>}
          {onRetry && (
            <button type="button" className="btn btn-xs mt-2" onClick={onRetry}>
              {ui("重试")}
            </button>
          )}
        </div>
      </div>
    </Panel>
  );
}
