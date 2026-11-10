/** 数据获取 hook：@tanstack/react-query useQuery 封装（P5 复用纪律整改：
 *  QueryClientProvider 此前空转，本 hook 是 src 内唯一的 react-query 消费面）。
 *  对外形状与原手写 useApi 完全一致：{ data, error, loading, retry }，页面
 *  三态消费方式不变；手写 fetch/abort/reloadKey 机制已由 react-query 取代。
 *
 *  行为对齐原实现（hooks.ts 旧版语义，页面行为保持一致）：
 *  - 查询键 = 端点+参数（path 字符串本身就是原取数键）；
 *  - staleTime = 0：每次挂载都重取（原实现组件本地态无跨挂载缓存）；
 *  - retry = false：无自动重试（原实现只有手动 retry 回调）；
 *  - 窗口聚焦/网络恢复不触发重取（原实现无此监听）；
 *  - 路径切换时旧数据留屏（keepPreviousData，原 SWR「旧数据留屏」语义）；
 *  - 身份变更事件（rip:identity）令全部在屏查询失效重取（原 reloadKey 语义）；
 *  - 非 ApiError 的抛出统一包成 ApiError(0, ...)（原 catch 分支同口径）。 */

import { useEffect } from "react";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { apiGet, ApiError } from "./api";

/** 身份变更事件：令牌刷新或重新登录后派发，所有在屏查询重取。 */
export const IDENTITY_EVENT = "rip:identity";

export function notifyIdentityChanged(): void {
  window.dispatchEvent(new Event(IDENTITY_EVENT));
}

/** 取数器签名：path + abort 信号，返回解析后的数据；由调用方注入。 */
export type Fetcher<T> = (path: string, signal: AbortSignal) => Promise<T>;

const defaultFetcher: Fetcher<unknown> = (path, signal) => apiGet(path, { signal });

export interface UseApiOptions<T> {
  /** 注入的取数器；不传则用默认 apiGet。传内联函数不会触发重取（重取只看
   *  queryKey，fetcher 不进依赖）。 */
  fetcher?: Fetcher<T>;
}

export function useApi<T>(path: string | null, options: UseApiOptions<T> = {}) {
  const fetcher = options.fetcher ?? (defaultFetcher as Fetcher<T>);
  const queryClient = useQueryClient();

  // 身份变更 → 全部在屏查询失效重取；react-query 对同一 key 的并发失效自动
  // 去重（原实现每个挂载点各起一次请求，行为收敛为每 key 一次）。
  useEffect(() => {
    const onIdentity = () => {
      void queryClient.invalidateQueries();
    };
    window.addEventListener(IDENTITY_EVENT, onIdentity);
    return () => window.removeEventListener(IDENTITY_EVENT, onIdentity);
  }, [queryClient]);

  const query = useQuery<T, ApiError>({
    queryKey: ["api", path],
    enabled: path !== null,
    // path 为 null 时 queryFn 不会执行，?? "" 仅为类型收窄。
    queryFn: async ({ signal }) => {
      try {
        return await fetcher(path ?? "", signal);
      } catch (requestError) {
        // 取消导致的失败由 react-query 自行丢弃（in-flight 换 key/卸载），
        // 这里只负责把非 ApiError 统一成带构造的 ApiError。
        throw requestError instanceof ApiError
          ? requestError
          : new ApiError(0, String(requestError));
      }
    },
    placeholderData: keepPreviousData,
    staleTime: 0,
    retry: false,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });

  return {
    /** 上一次成功载荷；路径切换期由 keepPreviousData 留屏。 */
    data: query.data ?? null,
    error: query.error ?? null,
    /** 有在途请求即 loading：首取、换路径、retry、失效重取全部覆盖（原
     *  实现 loading 的四个来源一致）。 */
    loading: query.isFetching,
    retry: () => {
      void query.refetch();
    },
  };
}
