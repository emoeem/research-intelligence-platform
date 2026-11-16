# RIP3 交接与续作指南（跨会话唯一入口）

> 本文件是 RIP3 续作的唯一交接入口。新会话从这里开始，按序读完即可接手全部剩余工作。
> 2.0 的底账（M0 到 Phase 10）见 docs/HANDOVER-RIP2-CONTINUATION.md 与
> docs/SESSION-HANDOFF.md（已收口，仅作背景）；RIP3 一切以本文件与 docs/rip3/ 为准。

| 项 | 值 |
| --- | --- |
| 状态 | 生效中（2026-11-17，Phase 4 收口：P4-1 至 P4-4 四批全绿 + RELEASE_2026-11-17_RIP3-PHASE4.md；下一阶段 Phase 5 P5-0 只读审计开批） |
| 计划书 | docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md（修订版 R2 + 修订记录段，唯一权威） |
| 适用版本 | git 41bf3359 及之后（Phase 1 收口 aabc3894） |
| CI | 主分支全绿基线（最新 run 见 GitHub Actions） |
| 相关 | docs/rip3/00-ARCHITECTURE-AUDIT.md（文档序首篇）；docs/releases/RELEASE_2026-11-15_RIP3-PHASE1.md |
| 自动化收官 | `.zcode/workflows/rip3-finish.dwf.ts`（动态工作流保存式脚本：Phase 3→8 + §17/D8 收官自查逐阶段编排，批次自动入库 + 编排面五道门禁复测 + HEAD CI 确认 + 受阻处置站）；启动 = 在有动态工作流工具面的会话（仓库根）`CreateWorkflow saved: { name: "rip3-finish" }` 或以 path 直提该文件；编译诊断按 §13 改文件后 path 重提 |

## 一、项目现状（30 秒版）

RIP 2.0 十阶段全部交付收口（底账见 HANDOVER-RIP2-CONTINUATION.md，队列 1 到 7 全勾销）。
RIP3 = 平台架构升级（Document Intelligence + Knowledge Governance + Hybrid Search +
Workflow/Report Intelligence），计划书修订版 R2 经用户评审定稿，其生效条件（2.0 收口）
已满足。当前进度：

| 阶段 | 状态 | 关键提交/产物 |
| --- | --- | --- |
| Phase 0 架构审计与四冻结 | 完成（afe6f5b，用户评审「通过」） | 00-ARCHITECTURE-AUDIT.md（A-G 八字段登记表 + 三关键问题实证）；四冻结物 03/04/12/13；ADR-0001/0002；R2 修订记录段 |
| Phase 1 文档域与存储 | 完成（c4c76a9 + 41bf3359 + aabc3894，CI 全绿） | rip-pg17 镜像族 + crates/intel-db + migrations_pg PG-0001/0002 + rp-doc CLI + REST 通道 + /documents 页 + 备份双轨演练；RELEASE_2026-11-15_RIP3-PHASE1.md |
| Phase 2 文档智能 | 完成（4139ed58 + P2-2 收口批，CI 全绿） | 批次报告 docs/rip3/batches/P2-1-BATCH-REPORT.md 与 P2-2-BATCH-REPORT.md；阶段总账 docs/releases/RELEASE_2026-11-15_RIP3-PHASE2.md；实施面 docs/rip3/05-DOCUMENT-INTELLIGENCE.md；遗留见批次报告 G 节（Parallel 分片调度随 Phase 8 / 解析状态卡随页面族批 / docling 误判观察面） |
| Phase 3 知识治理 | 完成（P3-1 到 P3-4 四批，CI 全绿） | 阶段总账 docs/releases/RELEASE_2026-11-15_RIP3-PHASE3.md；批次报告 docs/rip3/batches/P3-1 到 P3-4；D1-D5 五级去重 + Relations 十型 + 质量分六权重 + 审核工作流（018 Human 复用）+ 批量审核面；rp-kc CLI；容器族 +rip-gov/+rip-embed；遗留见批次报告 G 节（D4 阈值校准随 Phase 4 / evidence_level 导入侧接线随 Phase 4/7） |

用户三项已生效裁决（执行代理不得重开）：

1. **数据库**：情报核（Document/Chunk/Vector/KnowledgeCard/Relation/Finding/Report/
   治理面）= PostgreSQL 17 + pgvector 0.8 + pg_jieba + pg_search，绞杀者路线
   （ADR-0001），新增域永不落 SQLite；运营核（tool_run/agent_run/workflow_run/plugin/
   runtime_node/audit）暂留 SQLite（020+ 编号），Phase 8 基准后再定移植。
2. **成熟库优先**（AGENTS.md 硬约束）：通用能力先评估成熟维护库；vendor 不在不是拒绝理由。
3. **许可证只记录不做淘汰门槛**（能力优先，2026-11-14 裁决）：PyMuPDF（AGPL）与
   ParadeDB pg_search（AGPL）按能力转正选型，AGPL 组件在容器/服务侧使用并记录。

另有继承裁决：实战校验三面暂缓（项目完成后由用户启动）；ChatGPT 审阅步骤删除暂缓；
窗口重排 5 窗已确认（见 00-ARCHITECTURE-AUDIT 第六节）。

## 二、新会话 30 秒接手清单

1. 用 ZCode 打开仓库根（本文件在仓库内；AGENTS.md 是代理入口）。
2. 按序读（全部有 path:line 或命令级证据，禁止只读文档就下结论）：
   a. docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md（R2 + 修订记录段，第零部分执行边界
      与第二十部分硬性边界先读）；
   b. docs/rip3/00-ARCHITECTURE-AUDIT.md（现状底账 + 三关键问题实证 + 迁移编号基线）；
   c. 四冻结物：03-DOMAIN-MODEL.md（实体/RTIS 五面映射 v1）、04-DOCUMENT-IR-SCHEMA.md
      （IR v1）、12-STORAGE-AND-MIGRATION.md（存储/CAS/迁移纪律）、
      13-SECURITY-AND-PRIVACY.md（安全边界/数据通道）；
   d. docs/releases/RELEASE_2026-11-15_RIP3-PHASE1.md + docs/rip3/batches/ 两批次报告；
   e. git log --oneline -20 与 git status -sb。
3. 核对工作树：若有未提交改动 = 上一批次半成品，先按
   `git add -A && git commit -m "WIP 备份：接手前快照" && git push origin main`
   收口（SSH 断连重试 2 到 3 次即通，与本仓历史一致）。
4. 判定接手方式：
   - a. 原会话仍在跑（本机）：不动，等它推进；git log 与 docs/rip3/batches/ 是真相源。
   - b. RIP3 全部 Phase 收口：转 rip3 完成后的闭环事项（实战校验三面等用户启动）。
   - c. Phase 2 到 8 在途且原会话已死：直接按第六节队列表手动推进（本会话即 d 案
     手动全程，动态工作流工具不在会话工具面）；每批按 R2 §17.6 A-H 格式出批次报告。
   - d. 纯手动推进口径：每批规格 = R2 对应阶段行 + 四冻结物 + 本文件第七节坑位清单；
     批次报告落 docs/rip3/batches/PN-M-BATCH-REPORT.md。
5. 推进纪律：每批五道门禁 + selftest + 阶段收口（RELEASE 登记 + push + CI 全绿硬门槛，
   红不进下一阶段）；RTIS 对账零回归是每阶段红线；测试失败不得删测试或降标准。

## 三、剩余任务队列（完成即按序执行）

| 序 | 任务 | 验收基准（R2 第十四部分原文为判据源） |
| --- | --- | --- |
| 1 | Phase 2 文档智能 | rip-parser 容器族（Docling/PyMuPDF/Tesseract 模型烘焙离线 + digest 台账）；Parser Registry + Provider 接口；解析管线 = workflow 定义（九态状态机映射既有八类节点）；Document IR v1 落库（04 号冻结物 Schema）；解析质量检查；OCR 面。验收：PDF/EPUB/MD/DOCX/PPTX/XLSX/HTML/TXT/图片/ZIP 统一入库；结构正确；页码可定位；失败可重试；解析器版本可记录；network=none |
| 2 | Phase 3 知识治理 | D1 sha256 / D2 规范化哈希 / D3 datasketch MinHash+LSH worker（容器）/ D4 语义候选（pgvector，禁自动合并）/ D5 CONTRADICTS 进审核；KnowledgeRelation 10 型；质量分六权重 + 七类风险标记；审核工作流（复用 018 控制面 + Human 节点）；RTIS 对账零回归（15155 口径） |
| 3 | Phase 4 混合检索 | 统一 Search API（PG 面：pg_search BM25 + pgvector + JSONB 过滤）；RRF/MMR 融合在 Rust 检索层（跨 SQLite 知识面与 PG 文档面）；reranker worker（bge-reranker ONNX）；tokenizers 接入替换 est_tokens；缓存升级；Benchmark（RTIS 语料播种，Recall@K/MRR/NDCG） |
| 4 | Phase 5 工作流与安全研究 | workflow 模板机制（现无模板能力，审计实证）；渗透/SRC 十三步模板；自适应研究引擎（覆盖矩阵七维/假设生成/五态纪律 CONFIRMED 等）；画布增强；Findings 经 RTIS 接口落库 |
| 5 | Phase 6 报告智能 | Report Domain/IR（事实引用 RTIS）；typst PDF 渲染（字体烘焙离线）；DOCX/HTML/MD/JSON 渲染器（统一 IR 单源）；Diagram Engine（Mermaid + typst cetz/fletcher）；通俗+技术双解释；导出质检 |
| 6 | Phase 7 知识回流 | Finding 到候选 KnowledgeCard 流水线（复用 k_proposal 模式）；Evidence 溯源（六 ID）；上游更新监测（复用 source_sync） |
| 7 | Phase 8 规模测试 | 1k/10k/100k 文档 + 百万 chunk 合成语料；导入/索引/检索/内存/存储/并发/迁移/重建/备份基准报告；决定运营核是否移植 PG |
| 8 | 现状接续（独立待办，不占窗口） | RTIS 侧清账（rtis.py 自测 10 过 7 败 + license/maintenance 轴，RTIS 侧批次）；实战校验三面（用户门控，暂缓中） |

## 四、贯穿纪律（用户硬性要求，违反即返工）

1. **继承优先（R2 原则 G 底线）**：trace 六 ID、Tool Contract 信封、workflow-engine、
   Context Broker L0-L3、podman 执行面、audit correlation、source_sync、RTIS 接口
   八项已有能力必须复用，禁止重建。
2. **RTIS 红线**：零直写生产库（一切读写走 rt.py CLI / rtis MCP 工具面）；零行为变更；
   漏洞细节不进 3.0 库与公开仓库；15155 对账零回归。
3. **新增域永不落 SQLite**：情报核一切新表走 migrations_pg（refinery，PG-0003 起）；
   SQLite 020+ 仅运营面小迁移。
4. **成熟库优先**：引入前检索官方 SDK 与成熟维护库；Rust 新依赖增量 vendor 保
   --offline；实施时钉版本 + OSV 复查；许可证只记录不淘汰（用户裁决）。
5. **门禁五道**（仓库根执行）：cargo test --offline --workspace、cargo clippy
   --offline --workspace --all-targets -- -D warnings、python3 -m unittest discover
   -s workers/python/tests、cd web && npm run build、cd web && npm run test；
   另 selftest_auth.sh 11 项与各阶段 selftest。push 后 GitHub CI 全绿为硬门槛。
6. **浏览器实测**：凡 UI 批次必须真实浏览器逐功能点实测并截图入库
   （docs/rip3/batches/browser-e2e/ 或 web/scripts/ 脚本 + 截图）。
7. **文档纪律**：中文禁用 em/en dash（Write 工具有 dash guard 会拦截）；数字实测写
   来源；坏消息单列；每批按 R2 §17.6 A-H 格式出批次报告。
8. **红线**：不在宿主机安装系统级依赖（npm 只进 web/，Python 重依赖只进容器镜像）；
   不用 sudo；不写生产 migrations/platform*.db（指纹红线）；不擅自 reset/clean/
   checkout/rebase；手动会话 commit/push 需用户明确要求（自动化批次按既定自动入库）。

## 五、运行时环境事实（本机口径，跨机需自适配）

- **podman**：distrobox 会话无 podman 二进制；shim 挂 PATH 调宿主
  （`mkdir -p /tmp/podman-shim && printf '#!/bin/sh\nexec distrobox-host-exec podman "$@"\n' > /tmp/podman-shim/podman && chmod +x /tmp/podman-shim/podman`，
  然后 export PATH="/tmp/podman-shim:$PATH"）；shim 在 /tmp 会被清，重建即可。
- **rip-pg17 容器**：`podman run -d --name rip-pg17 -e POSTGRES_PASSWORD=postgres
  -p 127.0.0.1:5432:5432 -v "$PWD/rip-data/pg17:/var/lib/postgresql/data"
  localhost/rip-pg17:latest`（digest 钉死引用见 containerfiles/README.md 台账）；
  init 引导需等 "PostgreSQL init process complete"（约 15s）再 psql。
- **DSN/对象目录**：RP_PG_DSN（缺省 postgres://postgres:postgres@127.0.0.1:5432/postgres）；
  RIP_OBJECT_STORE_DIR（缺省 rip-data/object-store）；rip-data/ 整体 gitignore。
- **端口**：7800 被生产 rp_server 常驻 systemd 服务占用（别动）；测试 rp_server 用
  7801+；vite dev 需 RP_API_TARGET=http://127.0.0.1:7801 指向测试服务，监听 [::1]
  （curl 用 localhost 或 [::1]，127.0.0.1 会 refused）。
- **浏览器**：playwright chromium 用户缓存（npx playwright install chromium，
  chromium-1254 与既有基线容差内）；浏览器文件上传用 playwright setInputFiles
  （web/scripts/rip3_p1_browser_e2e.mjs 是可复用模板）；IAB file chooser 不支持。
- **网络**：SSH 推送间歇 Connection closed 重试即通；api.github.com 与 github.com 443
  会整段间歇不可达（gh/WebFetch EOF/超时），CI 状态改走 webReader 抓 actions 页面。
- **vendor 增量**：cargo vendor 直连 crates.io 是设计行为绕不开（镜像受限环境）；
  口径 = 移开 .cargo/config.toml 联网 cargo fetch，然后手工增量 vendor（registry
  缓存解包 + .cargo-checksum.json 生成），**必须清理新目录嵌套 .gitignore 并同步
  checksum + 三方一致性核对（checksum/磁盘/git 零缺失），否则 CI checksum 校验炸
  （CI #63 实录，见 P1-2 报告 E2 节）**。
- **守卫与坑位**：跑 rtis.py test 会向 integrations/rtis/adapter_audit.jsonl（跟踪态）
  追加审计行触发 B5 守卫，跑后 git status 核对并还原；cargo/vitest/视觉门并发跑有
  负载 flake（时序敏感断言），收口复测静场串行；管道 tail 吃退出码，门禁用输出文件 +
  显式 echo；Option<&String>.as_deref() 得 Option<&String>（用 .map(|s| s.as_str())）；
  PG 判插入不用 xmax=0（FOR UPDATE 两步法，document_repo.rs 同款）。
- **配额**：bigmodel 双账户轮换；1308=5 小时窗满等重置、1005=池尽、1310=周月尽；
  停跑先 WIP 备份再等/切。

## 六、已知遗留（清账候选，非阻塞）

1. CI 无 PG service：intel-db 集成测试 CI 侧跳过（V-9 口径），本地真容器实跑为准；
   PG service job 随 Phase 4 批评估。
2. pg_search BM25 索引建演练未做（V0002 尚无全文/向量列，Phase 4 批）。
3. /documents 页未纳入视觉门矩阵（Phase 5/6 页面族统一扩展）。
4. vendor 手工增量口径（依赖缓存在位；跨机全量重跑用 UPGRADE.md 增量 vendor 流程）。
5. RTIS 侧：rtis.py 自测 10 过 7 败（非平台引入）+ license/maintenance 轴数据源未接，
   留 RTIS 批次。
6. M8_READINESS 限制 1/2（evidence 抽样 4/26、research_task 核心字段映射，环境面）。
7. 服务端插件来源拉取完整 confirm 后受理链路需真实上游可达（P10-2 已声明）。

## 七、Phase 5 开批速查（P5-0 只读审计先于一切实现批次）

- 补充计划 = docs/rip3/RIP3-PHASE5-WEB-WORKBENCH-AND-KNOWLEDGE-DRIVEN-
  VERIFICATION-PLAN.md（P5-0 至 P5-9 批次表 + 硬性架构边界 + UI 参考图清单）；
  任务书落 docs/rip3/batches/P5-0-TASK.md；范围 = P5-0 只读审计（不写业务
  代码）：
  1. Web 侧：ResearchPage 占位现状 / Layout-TabArea-InspectorPanel-
     ActivityPanel-Theme-Token-API 封装审计；Projects/Runs/Tools/Knowledge/
     Workflow 页面可复用组件矩阵。
  2. 后端侧：ResearchTask/Finding/Evidence/Asset/Trace 领域模型现状盘点；
     RTIS POC Search/Read/Use/Add 接口面（rt.py + MCP）；Tool Engine/
     Runtime Gateway/Workflow Engine 执行与取消能力。
  3. Provider 面：Nuclei/ZAP 既有 Manifest 与容器镜像状态；许可/维护/离线
     能力记录（成熟库优先纪律）。
  4. 产物：页面复用矩阵 + 后端能力复用矩阵 + 差距清单 + 新 DTO/Schema/
     迁移/Provider 接口 ADR 草稿 + P5-1 设计冻结提案。
- 参考图已落位：docs/rip3/ui-reference/yakit/{home,http-history,web-fuzzer}
  .png（B 组）；A 组（用户当前 UI 截图两张）待用户附上落 current/。
- P4-4 坑位实录（本批，详见 P4-4-BENCHMARK.md G 节）：tool-engine 容器
  stdout 并发读（大输出 >64KB 管道死锁）；MMR 只能在 0-1 相关分上做（RRF
  分上做 = 多样性压倒相关性）；检索结果缓存键必须含 query（漏了 = 多查询
  共用一键）；RTIS poc search 中文零命中（组件名用英文，kingdee 有命中金蝶
  没有）；batch-review approve 前必须逐卡 score（地板闸跳过面）；qrel 显式
  声明不用检索结果自判。

## 八、完成后闭环（Phase 8 之后）

R2 §17 最终验收 + 实战校验三面（用户门控，暂缓中）+ 版本命名决策 D8（2.x Phase 11+
或 3.0，按兼容性评估）。每轮循环到 §17 全绿为止；ChatGPT 步骤维持删除暂缓。

## 九、2026-11-15 手动会话实录（Phase 2 收口 至 P4-2，d 案全程）

| 批次 | 提交 | CI run | 结果 |
|---|---|---|---|
| P2-2 收口批 | 0179f2bc | 43709329153 | success 2m51s（Phase 2 收口） |
| P2-2 CI 回填 | 71c1f9a5 | 43709613596 | success 2m49s |
| P3-1 | e4dd5fe6 + 02e2e9f8 + 91232650 | 43713355070 | success 3m16s（红二连：unused import + const 断言，实录） |
| P3-2 | 2d46766c + 2d36b575 | 43714615323 | success 2m49s（mcp-server 负载 flake，同树重跑） |
| P3-3 | 97168499 | 43715885192 | success 3m0s |
| P3-4 + Phase 3 收口 | 83da42f3 | 43716976430 | success 3m19s |
| P4-1 | 29585fe7 + c412be88 + 7486797e + 19645d02 | 43719162660 | success 2m52s（CI 红三连实录：重复 cfg + mcp flake 加固 + needless return） |
| P4-2 | ee9ee79c | 43720100522 | success 3m12s |
| 收官工作流 | daaa56fe | 43710907819 | success 2m47s（先于上述批次入库） |

本会话交付摘要：Phase 2/3 全收口 + Phase 4 双批；三容器族（rip-parser 0.1.2 /
rip-gov 0.1.0 / rip-embed 0.1.0）+ migrations_pg V0003-V0006 + rp-kc/rp-doc
parse/parse-status/approve/batch-review/search + rp-search-unified + 审核工作流
wf-review-card-v1 两分支 + 解析管线 wf-parse-pipeline-v1 两分支 + selftest
phase2 22/22 / phase3 19/19 + 统一检索三模式 + 跨库归并双通道；收官自动化
工作流 .zcode/workflows/rip3-finish.dwf.ts（有动态工作流工具面的会话启动）。
门禁全程：workspace 707 passed / clippy 零告警 / unittest 232 / web 124 /
selftest 全集。D4 阈值实测数据点：同义改写对 0.8358（语义面独立召回，D3 零
召回）、无关句 0.325、阈值区间 0.83-0.92 随 P4-4 校准。

## 十、2026-11-15 手动会话实录（P4-3，d 案全程，WIP 快照接手）

| 批次 | 提交 | CI run | 结果 |
|---|---|---|---|
| P4-3 | e8682ba4 | 43729534846 | failure（vendor/tokenizers 18 文件被根 .gitignore models/ 规则误伤未入库，CI #63 同族实录） |
| P4-3 CI 修复 | da46e7e7 | 43729823719 | success 4 job 全绿（.gitignore 锚定 /models/ + 补 add 18 文件） |
| P4-3 docs 回填 | c7043a4e | 43730193861 | success（E2/第十节回填，P4-3 收口） |
| P4-3 前置 | 6a3ad9a9 | 43724657601 | success 3m18s（WIP 快照） |

P4-3 交付摘要：rerank 全链收口（rerank_search unimplemented! → hybrid+正文
ANY 批读+容器排序；rerank_hits 8KiB UTF-8 安全截断；rp-kc --mode rerank）+
tokenizers 接入（context-broker tokenizer.rs；EMBEDDING_TOKENIZER_DIR 优先，
缺省仓库模型，双缺席回落；est_tokens 单点切换；vendor 增量 tokenizers
0.21.4+fancy-regex 闭包 39 crate）+ 缓存升级（intel-db search_cache.rs 同构
cache.rs:117-207；query-vector + retrieval 双缓存；治理态指纹失效）+
selftest_phase4_search.sh。实测：rerank 容器分离 0.99969/0.68741 vs 3.7e-05；
CLI 前后对比 hybrid 0.0328/0.0323/0.0159 → rerank 0.9993/0.5041/0.0；缓存
命中 1.493s→0.237s；calibration 2/2（公式偏差<5% 达标）。集成 2/2 / context-
broker 66/0 / intel-db lib 28/0。

## 十一、2026-11-17 手动会话实录（P4-4，d 案全程，Phase 4 收口）

| 批次 | 提交 | CI run | 结果 |
|---|---|---|---|
| P4-4 | 4e41c53e | 43904610803 | success（Phase 4 收口，HEAD CI 全绿） |

P4-4 交付摘要：语料播种（RTIS verified 133 元数据面脱敏 + 合成 8 = ws-bench
141 卡全 APPROVED + 向量齐；POC 正文零复制）+ qrel 显式 10 查询（六形态）+
bench.rs 指标纯函数（单测钉值）+ rp-kc bench 子命令 + 四模式基准（hybrid
0.5812/0.9167/0.8212 全面超单通道；rerank MRR 0.95；数据见 P4-4-BENCHMARK.md）
+ JSONB 集成测试转正 + --sqlite-filters CLI 解析（fail-closed）+ 三平台级
bug 修复（tool-engine 管道死锁 / MMR 尺度错位 / 缓存键漏 query，详见
P4-4-BENCHMARK.md G 节）+ RELEASE_2026-11-17_RIP3-PHASE4.md（Phase 4 收口）。
Yakit UI 参考图落 docs/rip3/ui-reference/yakit/（Phase 5 P5-1 前置素材）。
