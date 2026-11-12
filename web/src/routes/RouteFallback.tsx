/** 路由级懒加载骨架：路由 chunk 下载期间的占位（机制沿自参照实现 App 的
 *  App.tsx 的 RouteFallback：aria-busy 标注 + 主题 token 面色骨架块）。
 *  P7-7 修正：roleless div 不允许 aria-label（axe aria-prohibited-attr
 *  serious），改挂 role=status（隐式 aria-live=polite，aria-label 在该
 *  角色下合法）； Suspense 揭示时序下回退帧可能仍在 DOM，门禁逐页扫。 */
export function RouteFallback() {
  return (
    <div role="status" aria-busy="true" aria-label="页面加载中" className="space-y-3 p-4">
      <div className="h-7 w-40 animate-pulse rounded-control bg-surface-2" />
      <div className="h-28 animate-pulse rounded-surface bg-surface-2" />
      <div className="h-28 animate-pulse rounded-surface bg-surface-2" />
    </div>
  );
}
