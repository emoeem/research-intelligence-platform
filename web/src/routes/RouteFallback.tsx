/** 路由级懒加载骨架：路由 chunk 下载期间的占位（机制沿自参照实现 App 的
 *  App.tsx 的 RouteFallback：aria-busy 标注 + 主题 token 面色骨架块）。 */
export function RouteFallback() {
  return (
    <div aria-busy="true" aria-label="页面加载中" className="space-y-3 p-4">
      <div className="h-7 w-40 animate-pulse rounded-control bg-surface-2" />
      <div className="h-28 animate-pulse rounded-surface bg-surface-2" />
      <div className="h-28 animate-pulse rounded-surface bg-surface-2" />
    </div>
  );
}
