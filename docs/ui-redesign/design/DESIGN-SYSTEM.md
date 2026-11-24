# RIP UI Design System 规格

> 状态：M1 设计冻结候选。Token 数值未在本阶段修改。
> 设计来源：现有 `web/src/styles/token.css`、`web/src/theme.ts`、`docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md`。

## 一、设计目标

专业、克制、信息密集的研究与安全工作台。视觉使用石墨深色表面、细边框、低装饰、明确层级。强调色沿用当前 RIP Token，不在原型中悄悄更换品牌色。

## 二、颜色 Token

生产颜色的单一来源仍为 `web/src/styles/token.css`。禁止页面硬编码新 HEX 或 OKLCH。设计语义映射：

| 语义 | 现有变量 | 用途 |
|---|---|---|
| 主背景 | `--color-base-200` | 应用底层 |
| 表面 | `--color-base-100` | Sidebar、Topbar、主面板 |
| 更深表面 | `--color-base-300` | 内嵌区和层次分区 |
| 主文本 | `--color-base-content` | 标题与正文 |
| 主强调 | `--color-primary` | 当前选择、主操作、关键焦点 |
| 辅助强调 | `--color-secondary` | 次级强调和产品图标 |
| 状态 | success / warning / error / info | 状态标签和反馈 |
| 边框 | `--edge` / `--edge-soft` | 细分隔线 |
| Sidebar | `--sidebar` | 左侧导航背景 |
| 次级表面 | `--surface-2` | Hover、选中行、浮层 |
| 次级文字 | `--muted` | 时间、说明、辅助字段 |

Dark / Light 主题必须使用同一语义角色，禁止只在一个主题中可见。

## 三、字体和排版

沿用现有字体栈：`--font-sans` 与 `--font-mono`。英文界面优先 Inter 风格的系统字体回退，中文按系统字体正常回退，不假设每台设备安装同一中文字体。代码、URL、HTTP Header、JSON、Trace ID 和时间戳使用等宽字体。

排版层级建议：
- Page title：20-24px，Semibold。
- Section title：14-16px，Semibold。
- 正文：13-14px。
- 表格：12-13px，关键数据不可低于 12px。
- 元数据：11-12px，仅辅助信息，必须有足够对比度。
- 编辑器：12-13px 等宽。

## 四、间距、圆角、密度

基于 4px 网格，主要间距 4/8/12/16/24/32px。按钮、输入框沿用 `--radius-control`，面板沿用 `--radius-surface`。不增加不必要阴影，不给每张卡片独立颜色。工作台数据表格应比普通设置页更紧凑，但行高和可读性不能牺牲。

## 五、图标与动效

统一使用已安装 Lucide React。图标只承担识别，不用 Emoji 代替导航图标。动画只用于状态切换、面板展开和短反馈。遵循 prefers-reduced-motion，不使用大面积渐变、发光和霓虹。

## 六、主题与可访问性

保留 `rip-dark` 和 `rip-light`，使用现有首帧防闪烁机制。主题改变必须验证页面、图表、编辑器、下拉、Dialog、表格、选中态与焦点态。状态不得只靠颜色传达，必须带文本、图标或 aria-label。

## 七、设计冻结与变更规则

M1 原型使用现有 token。只有在 Figma 可视复核和全站对照后，才能以 ADR 修改基础色。任何基础 Token 变更必须重新检查所有工作区、双主题和截图快照，不允许单页私自改颜色。
