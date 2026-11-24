import { useCallback, useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

/**
 * Windowed list shared by high-volume workspaces.
 * Fixed-height rows keep measurement deterministic and preserve the existing
 * cursor-pagination callback contract used by HTTP History.
 */
export function VirtualList(props: {
  itemCount: number;
  rowHeight: number;
  className?: string;
  testId?: string;
  renderItem: (index: number) => React.ReactNode;
  onScrollOffset?: (offset: number) => void;
  itemKey?: (index: number) => string | number;
  scrollToEndKey?: string | number;
}) {
  const {
    itemCount,
    rowHeight,
    renderItem,
    className,
    testId,
    onScrollOffset,
    itemKey,
    scrollToEndKey,
  } = props;
  const parentRef = useRef<HTMLDivElement | null>(null);
  const virtualizer = useVirtualizer({
    count: itemCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => rowHeight,
    initialRect: { width: 1024, height: rowHeight * 10 },
    overscan: 8,
    useFlushSync: false,
    getItemKey: (index) => itemKey?.(index) ?? index,
  });

  useEffect(() => {
    if (scrollToEndKey === undefined || itemCount === 0) return;
    virtualizer.scrollToIndex(itemCount - 1, { align: "end" });
  }, [itemCount, scrollToEndKey, virtualizer]);

  const handleScroll = useCallback(() => {
    const offset = parentRef.current?.scrollTop ?? 0;
    onScrollOffset?.(offset);
  }, [onScrollOffset]);

  return (
    <div
      ref={parentRef}
      className={className}
      data-testid={testId}
      onScroll={handleScroll}
      style={{ overflowY: "auto", height: "100%", position: "relative" }}
    >
      <div style={{ height: virtualizer.getTotalSize(), width: "100%", position: "relative" }}>
        {virtualizer.getVirtualItems().map((item) => (
          <div
            key={item.key}
            data-index={item.index}
            ref={virtualizer.measureElement}
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              width: "100%",
              height: item.size,
              transform: `translateY(${item.start}px)`,
            }}
          >
            {renderItem(item.index)}
          </div>
        ))}
      </div>
    </div>
  );
}
