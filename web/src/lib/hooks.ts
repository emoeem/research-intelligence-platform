/** 数据获取 hook：SWR（旧数据留屏）+ abort 竞态取消 + retry + 身份变更事件重载。
 *  模式沿自参照实现 admin/src/hooks.ts（UI 复用盘点 §3.5 解耦切点）：
 *  fetcher 改为参数注入（默认走 lib/api 的 apiGet），身份事件按 RIP 会话模型
 *  重定义（rip:identity，由令牌刷新/登录面在 P5-4 后派发）。 */

import { useCallback, useEffect, useRef, useState } from "react";
import { apiGet, ApiError } from "./api";

/** 身份变更事件：令牌刷新或重新登录后派发，所有在屏 useApi 重取。 */
export const IDENTITY_EVENT = "rip:identity";

export function notifyIdentityChanged(): void {
  window.dispatchEvent(new Event(IDENTITY_EVENT));
}

/** 取数器签名：path + abort 信号，返回解析后的数据；由调用方注入。 */
export type Fetcher<T> = (path: string, signal: AbortSignal) => Promise<T>;

const defaultFetcher: Fetcher<unknown> = (path, signal) => apiGet(path, { signal });

export interface UseApiOptions<T> {
  /** 注入的取数器；不传则用默认 apiGet。传内联函数不会触发重取（ref 保稳）。 */
  fetcher?: Fetcher<T>;
}

export function useApi<T>(path: string | null, options: UseApiOptions<T> = {}) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [loading, setLoading] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);

  // fetcher 存 ref：调用方每渲染传新函数时不能把请求打挂（effect 依赖只看 path/reloadKey）。
  const fetcherRef = useRef<Fetcher<T> | null>(options.fetcher ?? null);
  fetcherRef.current = options.fetcher ?? null;

  useEffect(() => {
    if (!path) return undefined;
    const controller = new AbortController();
    let cancelled = false;
    setLoading(true);
    setError(null);
    // Stale-while-revalidate：上一个载荷留在屏上。清空它（旧行为）会在每次
    // 防抖按键、翻页与筛选时把整个列表换回骨架屏，既闪又跳滚动位置。
    // abort 竞态：卸载/换路径时直接取消在途请求，配合 cancelled 双保险。
    const fetcher = fetcherRef.current ?? (defaultFetcher as Fetcher<T>);
    fetcher(path, controller.signal)
      .then((result) => {
        if (!cancelled) setData(result);
      })
      .catch((requestError: unknown) => {
        // 取消导致的失败不算错误：组件已经不在乎这次响应了。
        if (cancelled || controller.signal.aborted) return;
        setError(
          requestError instanceof ApiError ? requestError : new ApiError(0, String(requestError)),
        );
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [path, reloadKey]);

  useEffect(() => {
    const onIdentity = () => setReloadKey((key) => key + 1);
    window.addEventListener(IDENTITY_EVENT, onIdentity);
    return () => window.removeEventListener(IDENTITY_EVENT, onIdentity);
  }, []);

  const retry = useCallback(() => setReloadKey((key) => key + 1), []);

  return { data, error, loading, retry };
}
