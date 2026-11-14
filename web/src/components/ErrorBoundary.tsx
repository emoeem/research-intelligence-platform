/** 路由级错误边界：class 组件 + scope 命名 + reset/reload 双出口。
 *  整体沿自参照实现 admin/src/components/ErrorBoundary.tsx（UI 复用盘点 §3.4），
 *  零业务依赖；P5-3 壳层按 location.pathname re-key 挂载，单页渲染错误不炸整个壳。
 *  PROD 下隐藏 message 与 stack，避免把内部信息暴露给访客。 */

import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  scope?: string;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error(
      `[ErrorBoundary${this.props.scope ? `:${this.props.scope}` : ""}]`,
      error,
      info.componentStack,
    );
  }

  private handleReset = (): void => {
    this.setState({ error: null });
  };

  private handleReload = (): void => {
    window.location.reload();
  };

  render(): ReactNode {
    if (this.state.error) {
      const error = this.state.error;
      return (
        <div className="grid min-h-dvh place-items-center px-5 py-10">
          <div className="w-full max-w-md rounded-surface border border-edge bg-base-100 p-6 text-center shadow-lg">
            <div className="mx-auto mb-4 grid h-14 w-14 place-items-center rounded-surface bg-error/10 text-error">
              <svg
                width="28"
                height="28"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2.2"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
                <line x1="12" y1="9" x2="12" y2="13" />
                <line x1="12" y1="17" x2="12.01" y2="17" />
              </svg>
            </div>
            <h2 className="text-lg font-bold tracking-tight">页面出错了</h2>
            <p className="mt-1 text-sm text-muted">
              {import.meta.env.PROD
                ? "出现了一个意外错误，请刷新页面重试。"
                : error.message || error.name || "未知错误"}
            </p>
            {!import.meta.env.PROD && error.stack ? (
              <pre className="mt-3 max-h-32 overflow-auto rounded-control bg-base-200 p-3 text-left text-[11px] leading-5 text-muted">
                {error.stack}
              </pre>
            ) : null}
            <div className="mt-5 flex justify-center gap-2">
              <button type="button" className="btn" onClick={this.handleReset}>
                重试
              </button>
              <button type="button" className="btn btn-primary" onClick={this.handleReload}>
                刷新页面
              </button>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
