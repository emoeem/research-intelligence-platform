import type { ComponentProps } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";

/** Single v4 integration boundary for every application split pane. */
export function ResizablePanelGroup(props: ComponentProps<typeof Group>) {
  return <Group {...props} />;
}
export function ResizablePanel(props: ComponentProps<typeof Panel>) {
  return <Panel {...props} />;
}
export function ResizablePanelSeparator(props: ComponentProps<typeof Separator>) {
  return <Separator {...props} />;
}
