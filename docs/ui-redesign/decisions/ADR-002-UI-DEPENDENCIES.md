# ADR-002：统一 UI 组件与成熟依赖栈

状态：M1 设计裁决候选，M3 经 Spike 决定最终采用。
日期：2026-11-24

## 决策

保留 React 19、Tailwind CSS 4、daisyUI 5、RIP Dark/Light、TanStack Query、ECharts、Lucide 和 XYFlow。候选成熟库为 TanStack Table v9、TanStack Virtual、react-resizable-panels v4、CodeMirror 6。Radix primitives 仅按需引入。

## 不采用

不引入另一套完整设计系统、组件库或主题。暂不引入 Monaco。不要把 shadcn/ui、Radix、daisyUI 三套视觉组件叠在一起。

## 条件

任何新增包必须记录当前版本、License、Peer Dependencies、依赖树、安全公告、bundle 影响和 API 兼容性。M3 在安装前做 React 19/TypeScript 7/Vite 8/Tailwind 4 实际 Spike。

## 版本核对基线

2026-11-24 本机 npm registry 实测：
- @tanstack/react-table 9.3.1, MIT, React >=18。
- @tanstack/react-virtual 3.14.28, MIT, React/React DOM 支持到 19。
- react-resizable-panels 4.24.1, MIT, React/React DOM 18/19。
- @codemirror/view 6.43.15, MIT。
- @codemirror/state 6.7.6, MIT。
- codemirror 6.0.2, MIT。
- @radix-ui/react-tooltip 1.4.1, MIT, React 16.8 到 19。

版本不是永久钉值。安装前必须重新确认 registry、lockfile 和官方文档；所有版本需 exact pin，不能用 latest 模糊安装。

## 例外

如 Spike 证明候选库与现有数据契约/无障碍/浏览器环境不兼容，需新增例外 ADR，列出替代库和测试依据，不允许直接手写通用组件。
