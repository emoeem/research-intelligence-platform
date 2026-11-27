# RIP UI Redesign 工作目录

> 状态：M0-M7 在已批准范围内完成；M8 SPA 按用户决定保持 staged，默认入口未切换。
> 日期：2026-11-24

本目录维护 UI 重构的基线、设计系统、页面规格、ADR、浏览器原型和验收证据。

- `MASTER-PLAN.md`：M0-M8 总计划和阶段门禁。
- `M0-BASELINE.md`：代码、服务、测试和 Figma 现状。
- `M0-DEPENDENCY-AUDIT.md`：当前依赖和成熟库候选。
- `M0-CURRENT-ROUTES.md`：当前工作区与路由。
- `M0-UI-PROBLEMS.md`：问题清单和优先级。
- `M1-PROTOTYPE-ACCEPTANCE.md`：原型验收、浏览器和 axe 结果、已知限制。
- `REFERENCE-001-DASHBOARD.md`：用户提供的 Dashboard 视觉参考、真实 API 映射和不照搬项。
- `I18N-IMPLEMENTATION.md`：zh-CN/en-US/ja-JP 多语言实现、资源覆盖和验收规范。
- `design/`：设计系统、AppShell、导航、组件、响应式和无障碍规格。
- `screens/SCREEN-SPECS.md`：12 个工作区的页面设计规格。
- `decisions/`：统一入口、组件依赖、导航/面板、HTTP 消息编辑器裁决。
- `prototype/index.html`：独立本地交互原型。
- `screenshots/`：原型桌面、移动、主题截图，以及 Dashboard 参考实现截图。

## 打开浏览器原型

```bash
cd /media/emo/code/pensting/research-platform
python3 -m http.server 4178 --bind 127.0.0.1 --directory docs/ui-redesign/prototype
```

浏览器打开 `http://127.0.0.1:4178/`。原型中的记录和状态全部为演示数据。按钮只在浏览器内更新演示状态，不调用生产 API，不写数据库。

## 当前阶段结论

M0 基线门禁、M1 规格与原型已记录；M2 AppShell、M3 专业组件、M4 独立 Projects/Research Assets、M5 Web Security、M6 Agent/Workflow/Runs/Runtime 均已收口。M7 当前复跑通过 85 项 Playwright、360/360 全路由响应式 smoke、22 files / 158 Vitest、typecheck、Prettier 与 production build；用户已接受当前 UI。按 M7=reference、Figma=existing 决策，不要求像素级 Figma parity。M8 已在 7801 建立隔离 SPA staged 入口，按用户决定不切换默认根。

**视觉基准说明**：用户于 2026-11-24 提供 Dashboard 参考图，已作为 M2-M7 的视觉方向和 Dashboard 截图验收基准，见 REFERENCE-001-DASHBOARD.md。Figma 文件此前仅读取过元数据；本轮 Figma MCP Starter 调用额度已触顶，get_metadata 被拒绝，无法读取 Frame/页面层级或完成像素级对照。四张用户截图已作为 reference；未修改 Figma。该限制不阻止按已提供截图和 RIP tokens 继续实施，但人工 Figma 签收仍待额度恢复。

## 2026-11-24 阶段收口补充

- M0 复测与收口：见 `M0-CLOSEOUT.md`；原始现场细节见 `M0-RECHECK.md`。
- M1 Token 细化：`design/COLOR-TOKENS.md`、`design/TYPOGRAPHY.md`。
- M1 页面规格现按 12 个工作区拆分到 `screens/*.md`；`SCREEN-SPECS.md` 保留为总览索引。
- 验收规范：`acceptance/VISUAL-ACCEPTANCE.md`、`FUNCTIONAL-ACCEPTANCE.md`、`ACCESSIBILITY.md`、`PERFORMANCE.md`。
- 当前质量门禁复测：TypeScript、Prettier、production build、Vitest 22 files / 155 tests、Playwright 85/85 均通过（含 360 组全路由三语言双主题五视口响应式检查）。42 张既有视觉基线仍需人工审阅，不能因自动化通过而视为视觉签收。
- M0 已收口；M1 设计规格与隔离原型已补齐并按用户确认的 Dashboard 参考方向冻结。Figma 像素级核对仍单独待办，不代表 Figma 文件已修改。


- `M2-REPORT.md`：AppShell、真实对象 Inspector、Activity 数据接入、可调整持久化面板与验收结果；M2 前端实现已收口，通知与原始日志流受后端能力边界限制。


- `M3-EXECUTION-PLAN.md`：通用专业组件审计、依赖门禁、实施批次与最终门禁；M3 已 CLOSED。
- `M3-REPORT.md`：M3 组件集成、真实数据边界和最终验收结果。

- `M4-REPORT.md`：基础业务页面、独立 Projects/Research Assets 领域模型、Rust/HTTP/前端实现与验收结果。
- `M5-REPORT.md`：Web Security 工作台与 Scope/Body/信任边界。
- `M6-REPORT.md`：Agent、Workflow、Runs、Trace、Runtime 工作区。
- `M7-REPORT.md`：多语言、双主题、响应式和未完成的人工视觉签收。
- `M4-M8-STATUS.md`：M4–M8 阶段判定、自动化门禁和人工/部署验收边界。
- `M7-PERFORMANCE-REPORT.md`：生产构建本地 24 次导航性能 smoke。
- `M8-STAGED-ENTRY.md`：7801 独立 SPA staged 入口、fallback、缓存、安全头和验收记录。
- `decisions/M4-PROJECTS-ASSETS-DOMAIN.md`：Projects 与 Research Assets 的独立领域决策。
- `M8-ENTRY-MIGRATION-READINESS.md`：旧静态 UI 与新 SPA 的迁移拓扑、当前 staged 状态与最终 promotion gate。
- `migrations/031_research_projects_assets.sql`：独立 Projects / Research Assets 的加法迁移。
