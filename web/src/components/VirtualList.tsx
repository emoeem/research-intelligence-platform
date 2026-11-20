import { useCallback, useEffect, useRef, useState } from "react";

/**
 * 自研窗口化虚拟滚动列表（WSW-3-ADR W3-D5：零新 npm 依赖——否决
 * @tanstack/react-virtual，依赖审计周期 > 本批收益；PLAN :404 虚拟滚动纪律）。
 *
 * 形态：固定行高 + scrollTop 换算 + 上下 overscan 各 5 行。断言面「仅视口
 * 渲染」自研列表可测：DOM 行节点数 ≤ 视口窗口 + overscan（e2e §四.3.2）。
 *
 * 用法：`<VirtualList itemCount={n} rowHeight={h} renderItem={(i) => …} />`；
 * 容器高度自适应父级（height: 100%），外层给固定高即可。
 */

const OVERSCAN = 5;

export function VirtualList(props: {
  itemCount: number;
  rowHeight: number;
  className?: string;
  testId?: string;
  renderItem: (index: number) => React.ReactNode;
  /** 滚动位置变化回调（测试/联动面，可选）。 */
  onScrollOffset?: (offset: number) => void;
}) {
  const { itemCount, rowHeight, renderItem, className, testId, onScrollOffset } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(600);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setViewportHeight(Math.max(entry.contentRect.height, 1));
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const handleScroll = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;
    setScrollTop(el.scrollTop);
    onScrollOffset?.(el.scrollTop);
  }, [onScrollOffset]);

  const firstVisible = Math.floor(scrollTop / rowHeight);
  const visibleCount = Math.ceil(viewportHeight / rowHeight);
  const start = Math.max(0, firstVisible - OVERSCAN);
  const end = Math.min(itemCount, firstVisible + visibleCount + OVERSCAN);
  const indices: number[] = [];
  for (let i = start; i < end; i += 1) indices.push(i);

  return (
    <div
      ref={containerRef}
      className={className}
      data-testid={testId}
      onScroll={handleScroll}
      style={{ overflowY: "auto", height: "100%", position: "relative" }}
    >
      {/* 总高占位：滚动条按全量行数呈现（窗口化不改变滚动距离语义）。 */}
      <div style={{ height: itemCount * rowHeight, position: "relative" }}>
        <div
          style={{
            position: "absolute",
            top: start * rowHeight,
            left: 0,
            right: 0,
          }}
        >
          {indices.map((index) => (
            <div key={index} style={{ height: rowHeight }}>
              {renderItem(index)}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
