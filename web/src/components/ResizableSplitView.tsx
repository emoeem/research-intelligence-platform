import { useEffect, useId, useMemo, useState, type ReactNode } from "react";
import {
  ResizablePanel,
  ResizablePanelGroup,
  ResizablePanelSeparator,
} from "./ResizablePanelGroup";
import { useUiText } from "../i18n";

type SplitLayout = Record<string, number>;

function readLayout(
  key: string,
  firstId: string,
  secondId: string,
  firstCollapsible: boolean,
  secondCollapsible: boolean,
  firstMaxSize: number,
  secondMaxSize: number,
): SplitLayout | undefined {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return undefined;
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return undefined;
    const envelope = parsed as { version?: unknown; layout?: unknown };
    if (envelope.version !== 1 || !envelope.layout || typeof envelope.layout !== "object")
      return undefined;
    const layout = envelope.layout as Record<string, unknown>;
    const first = layout[firstId];
    const second = layout[secondId];
    if (typeof first !== "number" || typeof second !== "number") return undefined;
    if (!Number.isFinite(first) || !Number.isFinite(second)) return undefined;
    const firstValid = first === 0 ? firstCollapsible : first >= 25 && first <= firstMaxSize;
    const secondValid = second === 0 ? secondCollapsible : second >= 25 && second <= secondMaxSize;
    if (!firstValid || !secondValid) return undefined;
    if (Math.abs(first + second - 100) > 1.5) return undefined;
    return { [firstId]: first, [secondId]: second };
  } catch {
    return undefined;
  }
}

export function ResizableSplitView({
  first,
  second,
  firstLabel,
  secondLabel,
  className,
  layoutKey,
  firstMinSize = 25,
  firstMaxSize = 75,
  secondMinSize = 25,
  secondMaxSize = 75,
  firstCollapsible = false,
  secondCollapsible = false,
}: {
  first: ReactNode;
  second: ReactNode;
  firstLabel: string;
  secondLabel: string;
  className?: string;
  layoutKey?: string;
  firstMinSize?: number;
  firstMaxSize?: number;
  secondMinSize?: number;
  secondMaxSize?: number;
  firstCollapsible?: boolean;
  secondCollapsible?: boolean;
}) {
  const rawId = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const firstId = `split-${rawId}-first`;
  const secondId = `split-${rawId}-second`;
  const [orientation, setOrientation] = useState<"horizontal" | "vertical">(() =>
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(max-width: 767px)").matches
      ? "vertical"
      : "horizontal",
  );
  const [resetVersion, setResetVersion] = useState(0);
  const [mobilePanel, setMobilePanel] = useState<"first" | "second">("first");
  const storageKey = useMemo(
    () => `rip_split_v1:${layoutKey ?? rawId}:${orientation}`,
    [layoutKey, orientation, rawId],
  );
  const [defaultLayout, setDefaultLayout] = useState<SplitLayout>();
  const firstMaxAllowed = secondCollapsible ? 100 : firstMaxSize;
  const secondMaxAllowed = firstCollapsible ? 100 : secondMaxSize;

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return undefined;
    const query = window.matchMedia("(max-width: 767px)");
    const update = () => setOrientation(query.matches ? "vertical" : "horizontal");
    update();
    query.addEventListener?.("change", update);
    return () => query.removeEventListener?.("change", update);
  }, []);

  useEffect(() => {
    setDefaultLayout(
      readLayout(
        storageKey,
        firstId,
        secondId,
        firstCollapsible,
        secondCollapsible,
        firstMaxAllowed,
        secondMaxAllowed,
      ),
    );
  }, [
    storageKey,
    firstId,
    secondId,
    resetVersion,
    firstCollapsible,
    secondCollapsible,
    firstMaxAllowed,
    secondMaxAllowed,
  ]);

  const handleLayoutChanged = (layout: SplitLayout) => {
    const first = layout[firstId];
    const second = layout[secondId];
    if (
      typeof first !== "number" ||
      typeof second !== "number" ||
      !Number.isFinite(first) ||
      !Number.isFinite(second) ||
      (first === 0 ? !firstCollapsible : first < 25 || first > firstMaxAllowed) ||
      (second === 0 ? !secondCollapsible : second < 25 || second > secondMaxAllowed) ||
      Math.abs(first + second - 100) > 1.5
    )
      return;
    try {
      localStorage.setItem(storageKey, JSON.stringify({ version: 1, orientation, layout }));
    } catch {
      // The current split layout remains usable without local persistence.
    }
  };

  const resetLayout = () => {
    try {
      localStorage.removeItem(storageKey);
    } catch {
      // Reset still remounts to the default layout.
    }
    setDefaultLayout(undefined);
    setResetVersion((version) => version + 1);
  };

  const ui = useUiText();

  const selectMobilePanel = (next: "first" | "second") => setMobilePanel(next);

  return orientation === "vertical" ? (
    <div className={`resizable-split-mobile ${className ?? ""}`}>
      <div
        role="tablist"
        aria-label={ui("分栏面板")}
        className="resizable-split-mobile-tabs"
        onKeyDown={(event) => {
          if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
            event.preventDefault();
            const next = mobilePanel === "first" ? "second" : "first";
            selectMobilePanel(next);
            document.getElementById(`${rawId}-mobile-tab-${next}`)?.focus();
          } else if (event.key === "Home" || event.key === "End") {
            event.preventDefault();
            const next = event.key === "Home" ? "first" : "second";
            selectMobilePanel(next);
            document.getElementById(`${rawId}-mobile-tab-${next}`)?.focus();
          }
        }}
      >
        <button
          id={`${rawId}-mobile-tab-first`}
          type="button"
          role="tab"
          aria-selected={mobilePanel === "first"}
          aria-controls={`${rawId}-mobile-panel-first`}
          tabIndex={mobilePanel === "first" ? 0 : -1}
          onClick={() => selectMobilePanel("first")}
        >
          {ui(firstLabel)}
        </button>
        <button
          id={`${rawId}-mobile-tab-second`}
          type="button"
          role="tab"
          aria-selected={mobilePanel === "second"}
          aria-controls={`${rawId}-mobile-panel-second`}
          tabIndex={mobilePanel === "second" ? 0 : -1}
          onClick={() => selectMobilePanel("second")}
        >
          {ui(secondLabel)}
        </button>
      </div>
      <div
        id={`${rawId}-mobile-panel-first`}
        role="tabpanel"
        aria-labelledby={`${rawId}-mobile-tab-first`}
        hidden={mobilePanel !== "first"}
        className="resizable-split-mobile-panel"
      >
        {first}
      </div>
      <div
        id={`${rawId}-mobile-panel-second`}
        role="tabpanel"
        aria-labelledby={`${rawId}-mobile-tab-second`}
        hidden={mobilePanel !== "second"}
        className="resizable-split-mobile-panel"
      >
        {second}
      </div>
    </div>
  ) : (
    <div className="resizable-split-container">
      <button type="button" className="resizable-split-reset" onClick={resetLayout}>
        {ui("恢复默认布局")}
      </button>
      <ResizablePanelGroup
        key={resetVersion}
        className={`resizable-split-view ${className ?? ""}`}
        orientation="horizontal"
        defaultLayout={defaultLayout ?? { [firstId]: 50, [secondId]: 50 }}
        onLayoutChanged={handleLayoutChanged}
        resizeTargetMinimumSize={{ coarse: 28, fine: 10 }}
      >
        <ResizablePanel
          id={firstId}
          defaultSize="50%"
          minSize={`${Math.max(firstMinSize, 30)}%`}
          maxSize={`${secondCollapsible ? 100 : firstMaxSize}%`}
          collapsible={firstCollapsible}
          collapsedSize="0%"
        >
          <div className="resizable-split-panel" aria-label={firstLabel}>
            {first}
          </div>
        </ResizablePanel>
        <ResizablePanelSeparator
          aria-label={`${firstLabel} / ${secondLabel} panel divider`}
          className="resizable-split-separator"
        />
        <ResizablePanel
          id={secondId}
          defaultSize="50%"
          minSize={`${Math.max(secondMinSize, 30)}%`}
          maxSize={`${firstCollapsible ? 100 : secondMaxSize}%`}
          collapsible={secondCollapsible}
          collapsedSize="0%"
        >
          <div className="resizable-split-panel" aria-label={secondLabel}>
            {second}
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
