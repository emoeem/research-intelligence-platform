import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, cleanup, within } from "@testing-library/react";
import { DataTable, type DataColumn } from "../src/components/data";
import { FilterBuilder, type FilterRule } from "../src/components/data/FilterBuilder";
import { VirtualTable } from "../src/components/data/VirtualTable";
import { Pagination } from "../src/components/data/Pagination";
import { DiffViewer } from "../src/components/editor/DiffViewer";
import { MessageViews } from "../src/components/MessageViews";
import { MessageEditor } from "../src/components/editor/MessageEditor";
import { JsonViewer } from "../src/components/editor/JsonViewer";
import { LogViewer, type LogEntry } from "../src/components/feedback/LogViewer";
import {
  LoadingState,
  SuccessState,
  EmptyState,
  ErrorState,
  OfflineState,
  PartialDataState,
  ReadOnlyState,
  UnauthorizedState,
} from "../src/components/feedback/StatePanels";

afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.restoreAllMocks();
});

const originalRect = HTMLElement.prototype.getBoundingClientRect;
function mockVirtualViewport() {
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.dataset.testid === "virtual-table-scroll" ||
      this.dataset.testid === "log-viewer-list"
      ? 420
      : 0;
  });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.dataset.testid === "virtual-table-scroll" ||
      this.dataset.testid === "log-viewer-list"
      ? 800
      : 0;
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
    this: HTMLElement,
  ) {
    if (
      this.dataset.testid === "virtual-table-scroll" ||
      this.dataset.testid === "log-viewer-list"
    ) {
      return {
        x: 0,
        y: 0,
        top: 0,
        left: 0,
        right: 800,
        bottom: 420,
        width: 800,
        height: 420,
        toJSON: () => ({}),
      } as DOMRect;
    }
    return originalRect.call(this);
  });
}

type Row = { id: string; name: string; score: number };
const rows: Row[] = Array.from({ length: 1000 }, (_, index) => ({
  id: `row-${index}`,
  name: `Item ${index}`,
  score: index,
}));

describe("M3 DataTable", () => {
  const columns: DataColumn<Row>[] = [
    { id: "name", label: "名称", width: "220px", sortValue: (row) => row.name },
    { id: "score", label: "分数", width: "100px", sortValue: (row) => row.score },
  ];
  it("sorts rows and exposes semantic headers", () => {
    render(
      <DataTable
        columns={columns}
        rows={rows.slice(0, 3)}
        rowKey={(row) => row.id}
        caption="测试列表"
        emptyText="无数据"
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "分数 排序" }));
    const body = screen.getByRole("table").querySelector("tbody");
    const scores = Array.from(body?.querySelectorAll("tr") ?? []).map(
      (row) => row.cells[1]?.textContent,
    );
    expect(scores).toEqual(["2", "1", "0"]);
    expect(screen.getByRole("columnheader", { name: /分数/ })).toBeTruthy();
  });
  it("supports controlled row selection without triggering row activation", () => {
    const onSelectionChange = vi.fn();
    const onRowActivate = vi.fn();
    render(
      <DataTable
        columns={columns}
        rows={rows.slice(0, 2)}
        rowKey={(row) => row.id}
        caption="可选择列表"
        emptyText="无数据"
        selectedRowIds={[]}
        onSelectionChange={onSelectionChange}
        onRowActivate={onRowActivate}
      />,
    );
    fireEvent.click(screen.getByRole("checkbox", { name: "选择行 row-0" }));
    expect(onSelectionChange).toHaveBeenCalledWith(["row-0"]);
    expect(onRowActivate).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("checkbox", { name: "选择全部行" }));
    expect(onSelectionChange).toHaveBeenLastCalledWith(["row-0", "row-1"]);
  });

  it("persists validated column layout and restores defaults", () => {
    localStorage.setItem(
      "rip_data_table_v1:tasks",
      JSON.stringify({
        version: 1,
        columnSizing: { name: 310, score: 120, injected: 9999 },
        columnVisibility: { score: false, injected: false },
      }),
    );
    const { rerender } = render(
      <DataTable
        columns={columns}
        rows={rows.slice(0, 2)}
        rowKey={(row) => row.id}
        caption="任务"
        layoutKey="tasks"
        emptyText="无数据"
      />,
    );
    expect(screen.queryByRole("columnheader", { name: /分数/ })).toBeNull();
    expect(screen.getByRole("columnheader", { name: /名称/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "重置列布局" }));
    expect(screen.getByRole("columnheader", { name: /分数/ })).toBeTruthy();
    expect(localStorage.getItem("rip_data_table_v1:tasks")).toBeNull();
    rerender(
      <DataTable
        columns={columns}
        rows={[]}
        rowKey={(row) => row.id}
        caption="任务"
        layoutKey="tasks"
        emptyText="没有记录"
      />,
    );
    expect(screen.getByText("没有记录")).toBeTruthy();
  });
});

describe("M3 FilterBuilder", () => {
  it("adds, edits and removes typed filter clauses", () => {
    const onChange = vi.fn<(rules: FilterRule[]) => void>();
    render(
      <FilterBuilder
        fields={[
          {
            id: "state",
            label: "状态",
            type: "select",
            options: [{ value: "open", label: "Open" }],
          },
          { id: "title", label: "标题", type: "text" },
        ]}
        rules={[]}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "添加条件" }));
    expect(onChange).toHaveBeenCalledWith([
      expect.objectContaining({ field: "state", operator: "equals", value: "" }),
    ]);
    const rule: FilterRule = { id: "rule-1", field: "title", operator: "contains", value: "alpha" };
    cleanup();
    const changed = vi.fn();
    render(
      <FilterBuilder
        fields={[{ id: "title", label: "标题", type: "text" }]}
        rules={[rule]}
        onChange={changed}
      />,
    );
    fireEvent.change(screen.getByLabelText("标题筛选值"), { target: { value: "beta" } });
    expect(changed).toHaveBeenCalledWith([{ ...rule, value: "beta" }]);
    fireEvent.click(screen.getByRole("button", { name: "删除标题条件" }));
    expect(changed).toHaveBeenLastCalledWith([]);
  });
});

describe("M3 VirtualTable", () => {
  it("renders a bounded window with stable row keys and supports keyboard activation", () => {
    const onActivate = vi.fn();
    render(
      <VirtualTable
        rows={rows}
        columns={[{ id: "name", label: "名称" }]}
        rowKey={(row) => row.id}
        label="一千条记录"
        height={240}
        rowHeight={30}
        onRowActivate={onActivate}
      />,
    );
    const grid = screen.getByRole("grid", { name: "一千条记录" });
    expect(grid.getAttribute("aria-rowcount")).toBe("1001");
    expect(within(grid).getAllByTestId("virtual-table-row").length).toBeLessThan(40);
    grid.focus();
    fireEvent.keyDown(grid, { key: "End" });
    fireEvent.keyDown(grid, { key: "Enter" });
    expect(onActivate).toHaveBeenCalledWith(rows[999]);
  });
});

describe("M3 DiffViewer", () => {
  it("marks inserted/deleted lines and can hide unchanged lines", () => {
    render(<DiffViewer before={"same\nold"} after={"same\nnew\nextra"} />);
    expect(screen.getByText("old")).toBeTruthy();
    expect(screen.getByText("new")).toBeTruthy();
    expect(screen.getByText("extra")).toBeTruthy();
    fireEvent.click(screen.getByRole("checkbox", { name: "显示未变化行" }));
    expect(screen.queryByText("same")).toBeNull();
  });
  it("protects against quadratic work on huge inputs", () => {
    render(
      <DiffViewer
        before={Array(1200).fill("a").join("\n")}
        after={Array(1200).fill("b").join("\n")}
      />,
    );
    expect(screen.getByRole("status").textContent).toContain("过大");
  });
});

describe("M3 LogViewer", () => {
  const entries: LogEntry[] = [
    { id: "1", level: "info", source: "worker", message: "started" },
    { id: "2", level: "error", source: "worker", message: "connection failed" },
  ];
  it("filters by level and text, and labels cached offline records", () => {
    mockVirtualViewport();
    render(<LogViewer entries={entries} offline />);
    expect(screen.getByText(/日志源离线/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText("日志级别"), { target: { value: "error" } });
    expect(screen.getByText("connection failed")).toBeTruthy();
    expect(screen.queryByText("started")).toBeNull();
    fireEvent.change(screen.getByRole("searchbox", { name: "搜索日志" }), {
      target: { value: "missing" },
    });
    expect(screen.getByText("暂无日志")).toBeTruthy();
  });
});

describe("M3 feedback states", () => {
  it("exposes unauthorized, read-only, partial and offline states to assistive technology", () => {
    render(
      <>
        <LoadingState />
        <SuccessState label="比较完成" />
        <EmptyState />
        <ErrorState />
        <UnauthorizedState />
        <ReadOnlyState />
        <PartialDataState />
        <OfflineState />
      </>,
    );
    expect(
      screen.getAllByRole("alert").some((element) => element.textContent?.includes("无权访问")),
    ).toBe(true);
    expect(screen.getByText("正在加载")).toBeTruthy();
    expect(screen.getByText("比较完成")).toBeTruthy();
    expect(screen.getByText("暂无记录")).toBeTruthy();
    expect(
      screen.getAllByRole("alert").some((element) => element.textContent?.includes("请求失败")),
    ).toBe(true);
    expect(screen.getByText("只读模式")).toBeTruthy();
    expect(screen.getByText("部分数据可用")).toBeTruthy();
    expect(screen.getByText("服务暂时不可用")).toBeTruthy();
  });
});

describe("M3 MessageViewer / MessageEditor", () => {
  it("uses keyboard-operable tabs and renders supplied original body bytes in Hex", () => {
    render(
      <MessageViews
        rawBody="A"
        rawBytes={new Uint8Array([0x41, 0xff])}
        testIdPrefix="m3-message"
      />,
    );
    const raw = screen.getByRole("tab", { name: "Raw" });
    raw.focus();
    fireEvent.keyDown(screen.getByRole("tablist"), { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Pretty" }).getAttribute("aria-selected")).toBe("true");
    fireEvent.click(screen.getByRole("tab", { name: "Hex" }));
    expect(screen.getByTestId("m3-message-hex").textContent).toContain("41 ff");
    expect(screen.getByRole("tabpanel").getAttribute("aria-labelledby")).toContain("hex");
  });

  it("renders a read-only CodeMirror editor without changing the source value", () => {
    render(
      <MessageEditor
        value='{"safe":true}'
        onChange={vi.fn()}
        language="json"
        readOnly
        label="只读消息编辑器"
      />,
    );
    expect(screen.getAllByLabelText("只读消息编辑器").length).toBeGreaterThan(0);
  });
});

describe("M3 cursor pagination", () => {
  it("uses next-cursor semantics without inventing page numbers", () => {
    const onNext = vi.fn();
    const { rerender } = render(
      <Pagination loadedCount={100} totalCount={240} hasNext onNext={onNext} />,
    );
    expect(screen.getByText("已加载 100 / 240")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "下一页" }));
    expect(onNext).toHaveBeenCalledOnce();
    rerender(<Pagination loadedCount={240} totalCount={240} hasNext={false} onNext={onNext} />);
    expect(screen.queryByRole("button", { name: "下一页" })).toBeNull();
  });
});

describe("M3 JsonViewer", () => {
  it("renders structured JSON and enforces the configured depth/size limits", () => {
    const value = { name: "alpha", nested: { enabled: true, count: 3 } };
    const { rerender } = render(<JsonViewer value={value} />);
    expect(screen.getByText('"alpha"')).toBeTruthy();
    expect(screen.getByText("enabled")).toBeTruthy();
    rerender(<JsonViewer value={value} maxDepth={0} />);
    expect(screen.getByText(/嵌套深度上限/)).toBeTruthy();
    rerender(<JsonViewer value={{ payload: "x".repeat(100) }} maxBytes={10} />);
    expect(screen.getByRole("status").textContent).toContain("过大");
  });
  it("reports non-serializable cyclic objects without crashing", () => {
    const value: Record<string, unknown> = {};
    value.self = value;
    render(<JsonViewer value={value} />);
    expect(screen.getByRole("alert").textContent).toContain("无法序列化");
  });
});
