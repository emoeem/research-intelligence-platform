# RIP 2.0 交接与续作指南（给新的 ZCode 项目）

> 本文件是跨项目交接的唯一入口。新 ZCode 会话从这里开始，按序读完即可接手全部剩余工作，不依赖原会话的任何记忆。

| 项       | 值                                                             |
| -------- | -------------------------------------------------------------- |
| 状态     | 生效中                                                         |
| 日期     | 2026-11-12（最后实质更新）                                     |
| 适用版本 | git `ace3c53` 及之后                                           |
| 相关     | docs/SESSION-HANDOFF.md（十四节授权脉络）、docs/handoff/workflow-script-RIP2-2026-11-12.dwf.ts（活体工作流脚本快照） |

## 一、项目是什么、做到哪了

Research Intelligence Platform（RIP）：研究智能平台，Rust Core（领域模型/SQLite/检索/治理）+ Python Workers（解析/Embedding/评估）+ React Web 工作台 + Podman 工具执行面 + 插件系统。最终目标：用户日常研究工作台 + AI Agent 执行基础设施 + 对外 MCP/REST/CLI。工程根：`/home/emo/code/pensting/research-platform`（独立 git 仓库，origin 为 SSH `git@github.com:emoeem/research-intelligence-platform.git`，仓库公开是用户决定）。

已完成并入库（全部经五道门禁 + 独立验收 + RELEASE 更新记录）：

| 阶段 | 内容 | 关键提交 |
|---|---|---|
| Phase 0 | 只读审计与架构冻结，八份交付物 + PHASE-ROADMAP | cc9dcfb |
| Phase 1 | Tool Contract 三份 v1 Schema、tool-registry/tool-engine、28 工具清单 | 962cfa9 |
| Phase 2 | podman_host 容器执行面、runtime-gateway 心跳/恢复、007 迁移 | f64ee24 |
| Phase 3 | Core Utilities + Web Research 两 Tool Pack 端到端、出站 allowlist | c9e7b23 |
| Phase 4 | Context Broker L0-L3、意图打分、预算缓存、008 迁移 | ce13424 |
| Phase 5 | Web Workbench 五页面 + ⌘K 接真实 API、react-query 数据层、浏览器实测 | 6217ca8 |
| Phase 6 | Plugin Registry/Runtime/009 迁移/rip-data 布局/五类来源/Catalog/Plugin Center 页/ADDING-A-TOOL 指南与脚手架 | ace3c53 |

进行中（原会话活体工作流 dwfrun-5f804a20，若已停则由接手方继续）：

- **SK 扩展（安全知识源接入）**：SK-1 通用摄取接线实施中；SK-2 三源插件（Awesome-Hacking/SecSkills/tongui 课程，规格见 docs/RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md，固定 Commit 在其 §3）；SK-3 抽取与静态审查工具。规格与验收清单 = 该计划 §8/§10，RTIS 15,155 条对账零回归是硬红线。
- **Phase 7（Agent Engine + WebUI 壳层升级）**：首批 = VS Code 式五区壳层 + workspaces 数据模型（Workspace/Project/Agent/Workflow 概念分离）；后续含 trace 关联（七类 ID 贯穿）、研究能力契约化（七能力 Schema+mock+测试）、Agent Studio 页。

## 二、新会话 30 秒接手清单

1. 用 ZCode 打开项目 `/home/emo/code/pensting/research-platform`（本文件在仓库内，AGENTS.md 是代理入口）。
2. 按序读：`docs/SESSION-HANDOFF.md`（十四节，授权与全部用户硬性要求）→ `docs/phase0/PHASE-ROADMAP.md`（冻结批次与验收命令）→ `git log --oneline -15`。
3. 核对工作树：`git status -sb`。若有未提交改动 = 原工作流在跑批次时的半成品，先 `git add -A && git commit -m "WIP 备份：接手前快照" && git push origin main`（SSH 失败重试 2 到 3 次，间歇断连是已知特性）。
4. 选择接手方式（三选一）：
   - **a. 原会话仍在跑**：什么都不做，等它推进；新项目只做观察（git log 与 docs/releases/ 是真相源）。
   - **b. 原会话已死，重建工作流**：用 CreateWorkflow 以 `path: docs/handoff/workflow-script-RIP2-2026-11-12-RESUME.dwf.ts` 提交（2026-11-13 起该脚本为跨机免编辑版：SK 段与 Phase 7 批次 1 到 4 已改核对性说明，P7-5 规格按被停运行规划员输出钉死且迁移编号已修正为 015，只规划 P7-6/P7-7；直接提交即可，无需再改）。到达尾洞（Phase 8/9/10）时按洞内 prompt 指引写填充。
   - **c. 纯手动推进**：每个批次的规格、路径、自验命令都在 PHASE-ROADMAP 与 docs/phaseN/PHASEN-PLAN.md 里，按队列逐批实施，每批过五道门禁 + 独立验收 + RELEASE 入库。
5. 推进纪律：阶段验收不过不进下一阶段；连续两次同一方法无新信息就换方法。

> 接手记录（2026-11-12 03:2x）：原运行 dwfrun-5f804a20 于 03:17 因 owning session closed 中断（spent 452M tokens），已按 b 案由新会话重建：重建脚本为 docs/handoff/workflow-script-RIP2-2026-11-12-RESUME.dwf.ts（Phase 5/6 已完成段改为只读核对，SK-1 半成品 WIP 备份 fec5a3f 已标注进规划与实施 ask，门禁命令按运行 cwd 即仓库根修正，仓库 core.sshCommand 已指向宿主密钥 /home/emo/.ssh/id_ed25519）。后续接手方优先用 RESUME 版脚本，原快照仅作历史参照。
>
> 接手记录（2026-11-13 用户暂停点）：重建运行 dwfrun-954913b0-9e59-48cd-a2af-1202026edf95 已完成 SK 扩展收尾（队列第 1 项完成：SK-1 到 SK-3，commit 2bf8e9c 入库推送）与 Phase 7 批次 1 到 4 半成品（五区壳层、agent-engine 最小执行闭环、013 三类 Memory、014 运行控制九类护栏，WIP 备份 7effba9 已推送，各自 RELEASE 见 docs/releases/RELEASE_*_PHASE7-P7-2/3/4.md）。运行因 1005 配额停过一次，续跑（缓存重放）后用户手动停。剩余：P7-5（trace 统一关联与跨栈查询，实施中被停）、P7-6、P7-7（含 Agent Studio 页）与 Phase 7 门禁/独立验收/入库三站，之后 Phase 8、9、10 与收尾闭环（第三节队列第 2 到 6 项）。恢复首选 ResumeWorkflowRun dwfrun-954913b0-9e59-48cd-a2af-1202026edf95（已完成步骤走缓存）；如需重建（运行丢失时），用 RESUME 版脚本并把 SK 段与 P7-1 到 P7-4 改为对照 2bf8e9c 与 7effba9 的核对性说明，另注意本机浏览器二进制口径：视觉门禁用 PW_CHROMIUM 指向宿主 Chrome for Testing 153（路径见 SESSION-HANDOFF 第三节 Phase 7 行）。
>
> 接手记录（2026-11-13 迁机跨机续作）：工作全部在 origin/main（最新 9639689），journal 不跨机（ResumeWorkflowRun 只在本机有效，原运行在新机器不可达），新机器直接走 b 案重建。RESUME 脚本已改写为跨机免编辑版（SK 与 P7-1 到 P7-4 为核对性说明、P7-5 钉死含迁移编号 015 修正、浏览器二进制口径机器无关化），新机器无需再改脚本。新机器开工三步：1) 配好 GitHub SSH 密钥后 git clone git@github.com:emoeem/research-intelligence-platform.git；2) 用 ZCode 打开 clone 出的仓库根目录（工作目录必须是仓库根）；3) 第一句话输入「读 docs/HANDOVER-RIP2-CONTINUATION.md 并按第二节执行」，核对员会先做环境自检（cargo/python3/node 版本、vendor 离线构建、web npm ci、push dry-run），缺什么如实报在 VerifyReport gaps 里。

## 三、剩余任务队列（完成即按序执行）

| 序 | 任务 | 验收基准 |
|---|---|---|
| 1 | SK-1/2/3 安全知识源扩展收尾 | 计划 §10 十五条清单 + RTIS 对账 + 浏览器实测三源展示 |
| 2 | Phase 7 Agent Engine + 壳层升级 | 2.0 计划 §14 Phase 7 + SESSION-HANDOFF §十（trace/契约）+ §十一（五区布局） |
| 3 | Phase 8 Workflow Studio | ROADMAP Phase 8 + §十（持久化/checkpoint/恢复，参考 LangGraph/Temporal 设计不替换 Rust 自研）+ 画布浏览器实测 |
| 4 | Phase 9 MCP/REST/CLI | ROADMAP Phase 9 + §十（correlation id 对外、MCP 与 Tool Center 同源） |
| 5 | Phase 10 生态与发布 | ROADMAP Phase 10 + §十（backup/restore 实现、FTS 重建校验、保留归档、许可证逐包清单、axe 对比度、视觉基线）+ autoresearch 性能基准 |
| 6 | 收尾审阅与实战校验闭环 | SESSION-HANDOFF §十二 + §十四（见第五节） |

## 四、贯穿纪律（用户硬性要求，违反即返工）

1. **复用优先**：通用逻辑一律先用成熟且仍在维护的开源库或仓库既有实现；Rust 新依赖走增量 cargo vendor 保 `--offline`；验收把「有库却手写」当不通过项。
2. **浏览器实测先行**：凡 UI 阶段必须用 browser-use 技能（browser-use:control-browser / web-gui-tester）驱动真实浏览器逐页逐功能点实测并留截图，不以构建通过替代。
3. **telegram-bot 文档先行**：复用其前端（/home/emo/code/telegram-bot）前必读 admin/README.md、admin/DEPENDENCIES.md、docs/HANDOVER.md（2026-11-08 最新）、docs/UPGRADE_ROADMAP_2026-10-28.md；editor/ 结论以 HANDOVER 为准。
4. **RELEASE 纪律**：每阶段入库按 docs/DOC-STANDARD.md 写 docs/releases/RELEASE_日期_主题.md 并登记 docs/README.md；数字实测写来源；坏消息单列。
5. **红线**：不在宿主机安装系统级依赖（npm 包只进 web/）；不用 sudo；不写生产 migrations/platform*.db；RTIS 生产行为零改动；文档中文禁用 em/en dash；不擅自 reset/clean/checkout/rebase。
6. **门禁五道**（必须从仓库根执行，vendor 配置按 CWD 发现）：`cargo test --offline --workspace`、`cargo clippy --offline --workspace --all-targets -- -D warnings`、`python3 -m unittest discover -s workers/python/tests`、`cd web && npm run build`、`cd web && npm run test`；另有 `bash apps/server/selftest_auth.sh`（11 项鉴权回归）。

## 五、完成后闭环（Phase 10 之后，循环到生产就绪）

按 SESSION-HANDOFF §十二与 §十四顺序执行，每轮产出问题清单，问 ChatGPT 后优化修复入库，直到 2.0 计划 §17 全绿：

1. **ChatGPT 全面审阅**：browser-use 打开 chatgpt.com（复用用户已登录会话），fill 提问后必须点「发送」按钮（Enter 无效），轮询「停止」按钮消失即完成，对话 region innerText 回收全文；给它仓库地址加 ROADMAP 与 releases 清单；会话「仓库快速审阅」可续问。
2. **GitHub 知识库与武器库补齐**：三源插件 Manifest/catalog 元数据/checksums 入库（课程原文不批量镜像、*.db 不入库）；Tool Pack Containerfile/Manifest 与首批安全工具（静态分析类先行）。
3. **实战校验三面**（每面跑 RIP 端到端：建任务 → Context 检索 → Tool 派发 → Evidence → 报告）：本地 podman 靶场（vulhub/DVWA 类）；在线授权靶场；公益 SRC（补天等授权项目，低并发、间隔不小于 1s、只打授权范围，漏洞细节只提交平台不进公开仓库）。
4. 每轮后回第 1 步。

## 六、运行时环境事实

- **配额**：start-plan 与 individual 两个 bigmodel 账户轮换用；1308=5 小时窗口满（等重置自动恢复）、1005=池尽（需充值或切账户）、1310=周/月尽。停跑后先 WIP 备份再等/切。
- **推送**：origin 已整体 SSH；SSH 间歇 Connection closed 重试即通；gh keyring token 对本仓库无写权、HTTPS 凭据助手脚本缺失，均不可用。
- **动态工作流机制**：尾洞（hole）到达时运行暂停等填充；填充 = 写该洞函数体返回 FinalReport 或链到下一个尾洞；运行期修订用 AmendWorkflow 传 path（旧运行若报 missing_boundaries 则无法带缓存，只能重建）；完成通知带 FinalReport。
- **成本教训**：常驻实施员上下文滚动极贵（Phase 6 单代理 3.4 亿）；用「每批新实施员 + 上一批交接摘要」模式。
- **验收立场**：验收员挑毛病不放行；fuzzy 通过视为未通过；未验证的如实标注。

## 七、已知遗留（清账候选，非阻塞）

rtis.py 自测 4 失败（T2/T4/T11/T13，非平台引入）；M8_READINESS 已知限制（evidence 抽样 4/26、research_task 核心字段映射）；license 轴数据源未接；egress_proxy JSON 通道重复键未拒绝（YAML 通道已拒绝）；视觉回归 Playwright 基线未建；README 与 M8_READINESS 文档状态冲突（收尾审阅时统一）。
