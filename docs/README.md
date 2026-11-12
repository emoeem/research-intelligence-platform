# docs 索引：什么时候看哪份

> research-platform 文档索引。新文档必须登记进本文件（见 DOC-STANDARD.md 提交前检查清单）。

| 项       | 值                   |
| -------- | -------------------- |
| 状态     | 生效中               |
| 日期     | 2026-11-13           |

## 日常看

| 想知道什么         | 看哪份                                                        |
| ------------------ | ------------------------------------------------------------- |
| 现在整体状态、跨会话接手 | [SESSION-HANDOFF.md](SESSION-HANDOFF.md)                     |
| 下一步做什么、阶段依赖 | [phase0/PHASE-ROADMAP.md](phase0/PHASE-ROADMAP.md)           |
| 每阶段交付了什么   | [releases/](releases/) 下按日期取最新                         |
| 文档怎么写才算合格 | [DOC-STANDARD.md](DOC-STANDARD.md)                            |
| 怎么给平台加新工具（武器库扩展） | [guides/ADDING-A-TOOL.md](guides/ADDING-A-TOOL.md)      |

## 目标与设计

| 主题             | 看哪份                                                                 |
| ---------------- | ---------------------------------------------------------------------- |
| 2.0 总体计划     | [RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md](RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md) |
| 工具执行隔离拓扑 | [phase0/TOOL-RUNTIME-ADR.md](phase0/TOOL-RUNTIME-ADR.md)               |
| 工具协议         | [phase0/TOOL-CONTRACT-DRAFT.md](phase0/TOOL-CONTRACT-DRAFT.md)         |
| 插件协议         | [phase0/PLUGIN-CONTRACT-DRAFT.md](phase0/PLUGIN-CONTRACT-DRAFT.md)     |
| 上下文分层       | [phase0/CONTEXT-BROKER-DESIGN.md](phase0/CONTEXT-BROKER-DESIGN.md)     |
| 安全知识源接入计划 | [RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md](RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md) |
| SK-1 摄取接线裁决 | [adr/SK1-INGEST-ADR.md](adr/SK1-INGEST-ADR.md)（指针：[phase6/SK1-INGEST-ADR.md](phase6/SK1-INGEST-ADR.md)） |
| 现状架构盘点     | [phase0/CURRENT-ARCHITECTURE-INVENTORY.md](phase0/CURRENT-ARCHITECTURE-INVENTORY.md) |
| 可登记工具清单   | [phase0/TOOL-INVENTORY.md](phase0/TOOL-INVENTORY.md)                   |
| 前端复用矩阵     | [phase0/TELEGRAM-ADMIN-UI-REUSE.md](phase0/TELEGRAM-ADMIN-UI-REUSE.md) |

## 阶段工作目录

| 阶段   | 目录           | 状态 |
| ------ | -------------- | ---- |
| Phase 0 | docs/phase0/   | 完成（commit cc9dcfb） |
| Phase 1 | docs/phase1/   | 完成（commit 962cfa9） |
| Phase 2 | docs/phase2/   | 完成（commit 352b3df，收口批次 P2-8 见 [releases/RELEASE_2026-11-10_PHASE2.md](releases/RELEASE_2026-11-10_PHASE2.md)） |
| Phase 3 | docs/phase3/   | 完成（批次 P3-0 到 P3-6，收口记录见 [releases/RELEASE_2026-11-10_PHASE3.md](releases/RELEASE_2026-11-10_PHASE3.md) 与 [phase3/PHASE3-ACCEPTANCE.md](phase3/PHASE3-ACCEPTANCE.md)；设计冻结 [phase3/PHASE3-DESIGN.md](phase3/PHASE3-DESIGN.md)） |
| Phase 4 | docs/phase4/   | 完成（批次 P4-0 到 P4-6，收口记录见 [releases/RELEASE_2026-11-10_PHASE4.md](releases/RELEASE_2026-11-10_PHASE4.md) 与 [phase4/PHASE4-ACCEPTANCE.md](phase4/PHASE4-ACCEPTANCE.md)；设计冻结 [phase4/PHASE4-DESIGN.md](phase4/PHASE4-DESIGN.md)，实施差异 [phase4/PHASE4-IMPLEMENTATION-DIFF.md](phase4/PHASE4-IMPLEMENTATION-DIFF.md)） |
| Phase 5 | docs/phase5/   | 完成（commit c0a4284，批次 P5-1 到 P5-10，收口记录见 [releases/RELEASE_2026-11-11_PHASE5.md](releases/RELEASE_2026-11-11_PHASE5.md) 与 [phase5/PHASE5-ACCEPTANCE.md](phase5/PHASE5-ACCEPTANCE.md)；阶段计划与偏差登记 [phase5/PHASE5-PLAN.md](phase5/PHASE5-PLAN.md)） |
| Phase 6 | docs/phase6/   | 完成（commit 49b9377，批次 P6-1 到 P6-9 含武器库扩展路径，收口入库记录见 [releases/RELEASE_2026-11-12_PHASE6.md](releases/RELEASE_2026-11-12_PHASE6.md) 与 [phase6/PHASE6-ACCEPTANCE.md](phase6/PHASE6-ACCEPTANCE.md)；批次实施快照 [releases/RELEASE_2026-11-11_PHASE6.md](releases/RELEASE_2026-11-11_PHASE6.md)，浏览器实测证据 [phase6/browser-e2e/](phase6/browser-e2e/)；SK-1 通用摄取接线收尾 [releases/RELEASE_2026-11-12_SK1-INGEST.md](releases/RELEASE_2026-11-12_SK1-INGEST.md)；SK-2 三源知识来源接入 [releases/RELEASE_2026-11-12_SK2-SOURCES.md](releases/RELEASE_2026-11-12_SK2-SOURCES.md) 与 [phase6/SK2-SOURCES-NOTES.md](phase6/SK2-SOURCES-NOTES.md)；SK-3 安全工具目录与静态审查工具 [releases/RELEASE_2026-11-12_SK3-TOOLS.md](releases/RELEASE_2026-11-12_SK3-TOOLS.md) 与 [phase6/SK3-TOOLS-NOTES.md](phase6/SK3-TOOLS-NOTES.md)；SK-1 到 SK-3 三批收口总账（V-9 门禁修复 + 独立复测截图 + RTIS 对账复跑） [releases/RELEASE_2026-11-12_SK-SECURITY-SOURCES.md](releases/RELEASE_2026-11-12_SK-SECURITY-SOURCES.md)） |
| Phase 7 | docs/phase7/   | 完成（P7-1 到 P7-7 七批：壳层升级 / Agent Engine 最小执行闭环 / 三类 Memory / 运行控制九类护栏 / trace 统一关联与跨栈查询 / 研究能力契约化 / Agent Studio 页与轨迹读取面，阶段总账见 [releases/RELEASE_2026-11-13_PHASE7.md](releases/RELEASE_2026-11-13_PHASE7.md)（批次汇总 / 五门禁数字 / trace 关联说明 / 截图证据链 / 已知取舍 / 回滚方式），分批 RELEASE P7-2 到 P7-7 见 [releases/](releases/)（P7-5 [releases/RELEASE_2026-11-13_PHASE7-P7-5.md](releases/RELEASE_2026-11-13_PHASE7-P7-5.md)，P7-6 [releases/RELEASE_2026-11-13_PHASE7-P7-6.md](releases/RELEASE_2026-11-13_PHASE7-P7-6.md) 与设计稿 [phase7/RESEARCH-CAPABILITY-CONTRACTS.md](phase7/RESEARCH-CAPABILITY-CONTRACTS.md)，P7-7 [releases/RELEASE_2026-11-13_PHASE7-P7-7.md](releases/RELEASE_2026-11-13_PHASE7-P7-7.md)），浏览器实测证据链 [phase7/browser-e2e/](phase7/browser-e2e/)（01 P7-1 壳层 / 02 P7-7 批次自验 / 03 阶段级独立验收）；阶段级五门禁入库当日复测全绿：cargo test --offline --workspace 510 passed 0 failed（94 目标）、clippy -D warnings 0 error、worker unittest 211 OK 跳 11、web typecheck/build/vitest 100 用例/prettier/视觉门 55 用例（PW_CHROMIUM 用户缓存 Chrome for Testing 151，零系统级安装）全过、selftest auth 11/11 + phase7_trace 33/33 + phase7_agents 50/50 全过（生产库零写核对一致）；入库推送 commit 短哈希按仓库回填惯例补记（先例 05ed5ee/f02d9b3）） |
