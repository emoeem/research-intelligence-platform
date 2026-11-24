import { useId, useState, type ReactNode } from "react";
import {
  ResizablePanel,
  ResizablePanelGroup,
  ResizablePanelSeparator,
} from "./ResizablePanelGroup";

function readSize(key: string, fallback: number): number {
  try {
    const value = Number(localStorage.getItem(key));
    return Number.isFinite(value) && value >= 20 && value <= 80 ? value : fallback;
  } catch {
    return fallback;
  }
}

export function PersistentShellSplit({
  first,
  second,
  orientation,
  storageKey,
  firstSize = 72,
  firstMin = 35,
  secondMin = 20,
  separatorLabel,
  className,
}: {
  first: ReactNode;
  second: ReactNode;
  orientation: "horizontal" | "vertical";
  storageKey: string;
  firstSize?: number;
  firstMin?: number;
  secondMin?: number;
  separatorLabel: string;
  className?: string;
}) {
  const id = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const firstId = `shell-${id}-primary`;
  const secondId = `shell-${id}-secondary`;
  const [initialSize] = useState(() => readSize(storageKey, firstSize));
  const [fallbackSize, setFallbackSize] = useState(initialSize);
  const saveFallback = (next: number) => {
    const bounded = Math.max(20, Math.min(80, next));
    setFallbackSize(bounded);
    try {
      localStorage.setItem(storageKey, String(bounded));
    } catch {
      /* optional persistence */
    }
  };
  if (typeof ResizeObserver === "undefined") {
    const firstStyle =
      orientation === "horizontal" ? { width: fallbackSize + "%" } : { height: fallbackSize + "%" };
    const secondStyle =
      orientation === "horizontal"
        ? { width: 100 - fallbackSize + "%" }
        : { height: 100 - fallbackSize + "%" };
    return (
      <div
        className={`persistent-shell-split persistent-shell-fallback ${orientation} ${className ?? ""}`}
      >
        <div className="persistent-shell-pane" style={firstStyle}>
          {first}
        </div>
        <button
          type="button"
          role="separator"
          aria-orientation={orientation}
          aria-label={separatorLabel}
          aria-valuemin={20}
          aria-valuemax={80}
          aria-valuenow={fallbackSize}
          tabIndex={0}
          className="persistent-shell-separator"
          onKeyDown={(event) => {
            if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
              event.preventDefault();
              saveFallback(fallbackSize - 2);
            }
            if (event.key === "ArrowRight" || event.key === "ArrowDown") {
              event.preventDefault();
              saveFallback(fallbackSize + 2);
            }
          }}
        />
        <div className="persistent-shell-pane" style={secondStyle}>
          {second}
        </div>
      </div>
    );
  }

  return (
    <ResizablePanelGroup
      className={`persistent-shell-split ${className ?? ""}`}
      orientation={orientation}
      defaultLayout={{ [firstId]: initialSize, [secondId]: 100 - initialSize }}
      onLayoutChanged={(layout) => {
        const next = layout[firstId];
        if (typeof next !== "number" || !Number.isFinite(next)) return;
        try {
          localStorage.setItem(storageKey, String(Math.max(20, Math.min(80, next))));
        } catch {
          // Layout remains usable if storage is unavailable.
        }
      }}
      resizeTargetMinimumSize={{ coarse: 28, fine: 10 }}
    >
      <ResizablePanel
        id={firstId}
        defaultSize={`${initialSize}%`}
        minSize={`${firstMin}%`}
        maxSize="80%"
      >
        <div className="persistent-shell-pane">{first}</div>
      </ResizablePanel>
      <ResizablePanelSeparator aria-label={separatorLabel} className="persistent-shell-separator" />
      <ResizablePanel
        id={secondId}
        defaultSize={`${100 - initialSize}%`}
        minSize={`${secondMin}%`}
        maxSize="80%"
      >
        <div className="persistent-shell-pane">{second}</div>
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}
