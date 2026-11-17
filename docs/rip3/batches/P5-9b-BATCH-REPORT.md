# P5-9b 批次报告：Evidence 与 Coverage 页面（检索端点 ×3 + 两新页 + 路由 18 + 派发面模板装载修复）

> 批次：P5-9 子批二（Evidence 与 Coverage 页面）。规格源 = Phase 5 计划 §4.7 / §4.8 / §7 P5-9 行 + HANDOVER-P5-WIP.md 第四节 + rip3-p5-finish P5_9B_BRIEF；接续 = P5-9a（dd0860ae / 942d453c，CI 双绿）。

## A. 本批完成内容

1. **Evidence rp_server 检索端点 ×3**（P5-7 G3 遗留落地）：GET `/api/research/evidence?plan=<vp>|item=<vi>`（列表，域二选一）、GET `/api/research/evidence/<re-id>`（单条详情）、GET `/api/research/coverage?plan=<vp>`（七维矩阵）。鉴权沿用既有 GET 面 require_auth + AUTH_ENABLED（写端点 Bearer 强制先例，本批端点全读面）；错误格式沿既有 ApiError(400, code, message) 惯例；实现走 rp-verify 子进程通道（`_rp_verify_json`，本进程零直连 SQL，P5-2 惯例）。
2. **rp-verify CLI 扩面**：`evidence-list` 由项域扩为计划域或项域二选一（`--plan` / `--item`，同给或缺一 exit 2 不做静默默认域）；新增 `evidence-get --id <re-…>` 单条回读；store 增 `list_evidence_by_plan`（ix_research_evidence_plan 索引面）与 `get_evidence`（re- 形状守卫）。
3. **UI Evidence 页**（/research/evidence，第七工作区第 6 tab）：计划选择 → 证据列表（8 条双帧实测）→ 详情（SHA256 / evidence_type 词表徽标 / 采集时间 / 来源跳转：plan → 验证计划页、tool_run → Run 详情、trace → Trace 页）+ Raw/Pretty 双视图（Pretty 格式化 JSON，Raw 紧凑序列化原文）+ 大 payload 截断展示（8000 字符口径 + 明示）+ §4.7 诚实提示（原始快照 append-only 不因知识更新覆盖、脱敏以采集侧为准本页不二次改写）。
4. **UI Coverage 矩阵页**（/research/coverage，第 7 tab）：coverage-show 数据直通渲染七维行（中英双语标签）× 五态列（未测试与失败逐列独立，不用百分比掩盖）+ 分母明示行（计划项共 N / 已判定 N）+ 盲区 badge（uncovered / unjudged 两形态分开）+ 盲区词表框（无命中不等于无漏洞提示）。
5. **暂停/恢复/失败重试入口核对**：ResearchPlansPage 控制面 P5-6 已落地（executing → pause/cancel、paused → resume/cancel、终态不渲染按钮），本批 e2e 断言终态计划零按钮（06 截图）；失败重试 = execute-plan 幂等恢复（重跑只派发未判定项）与 018 retry 面（workflow-runs retry 端点既有），本批零增量如实登记。
6. **路由钉值同步**：workspaces.ts research 工作区 tabs 5 → 7（order 6 证据 / 7 覆盖矩阵），路由总数 16 → 18，workspaces.test.ts 钉值同步（App.tsx 两 lazy 路由同批）。
7. **派发面缺陷修复（实拍发现，附带交付）**：execute_nuclei 模板落位后缺 `-t` 旗标，nuclei 恒 FTL "no templates provided"（真实执行不可达 NOT_REPRODUCED，P5-9c 本地靶场 CONFIRMED 前置），详见 B 节。
8. **浏览器端到端**：新脚本 web/scripts/rip3_p5_9b_browser_e2e.mjs，16 断言全过 + 7 截图入库（I 节）。

## B. 架构变化和复用说明

### 派发面缺陷修复实录（本批实拍发现，坏消息单列）

P5-9b 实拍种子链跑 execute-plan（never-match 模板 + network none）四项全 INCONCLUSIVE，result_reason = `container exited 1: [FTL] Could not run nuclei: no templates provided for scan`。根因两层：

1. **provider.rs 缺 `-t` 旗标**：wrapper.sh 把 /task/in/template.yaml 装入 /task/out/nuclei-templates（镜像侧正确，手动容器验证 cp 成功），但 execute_nuclei 的 entrypoint 参数面只有 `-no-color -u -rate-limit -timeout`，nuclei 不读默认目录，无 `-t` 即 FTL。修复 = template_path 落位成功后 entrypoint 追加 `-t /task/out/nuclei-templates`（crates/verification-engine/src/provider.rs，模板缺省派发不加旗标不静默装空目录）。回放四项全 NOT_REPRODUCED（`clean run (exit 0): template loaded, zero matches`）。
2. **e2e 模板自身缺 author 字段**：nuclei 3.11.1 目录装载要求 info 含 author/description，缺者被目录静默过滤（-validate 静默通过但 -tl 零列出）。修复 = e2e 模板补 author + description（容器内 -tl 单文件列出验证）。此为调用面模板规格教训（见 K 节），非平台缺陷。

历史对账：P5-6 报告的 NOT_REPRODUCED 实拍为 evaluator 契约测试矩阵（六形态正负例），非真实 nuclei 执行；P5-7 实拍 evidence 双帧落档不依赖执行成功（失败现场帧也落档不静默），故该缺陷未暴露。本批修复使真实执行链首次贯通到五态终态。

### 复用矩阵（本批零重建）

| 能力 | 复用物 | 增量 |
| --- | --- | --- |
| rp-verify 子进程通道 | `_rp_verify_json` + ApiError 错误格式（P5-2） | 三个 handler 复用 |
| 鉴权 | require_auth + AUTH_ENABLED + 审计四件套（既有 GET/POST 面） | 零（全读端点） |
| Evidence 存储 | 021 research_evidence + store list_evidence（P5-7） | list_evidence_by_plan / get_evidence 两方法（同款 SQL 形状） |
| Coverage 推导 | coverage.rs coverage_matrix（P5-8） | 零（coverage-show 子进程直通） |
| UI 骨架 | useApi / EmptyPanel / ErrorPanel / SkeletonPanel / FilterSearch（既有组件） | 零 |
| 类型与词表 | routes/research/shared.tsx 惯例 | Evidence/Coverage 两段追加 |
| 暂停恢复控制 | ResearchPlansPage + POST 控制端点（P5-6 形状三） | 零（核对 + e2e 断言） |
| 浏览器验收 | rip3_p5_browser_e2e.mjs 模板（probeChromium/newPage/pass 骨架） | 新脚本同骨架 |

## C. 修改文件清单

| 文件 | 变化 |
| --- | --- |
| crates/verification-engine/src/store.rs | list_evidence_by_plan + get_evidence（021 既有表零迁移） |
| crates/verification-engine/src/bin/rp-verify.rs | evidence-list 计划域/项域二选一 + evidence-get 新子命令 + usage 同步 |
| crates/verification-engine/src/provider.rs | execute_nuclei 模板装载 `-t` 旗标（缺陷修复） |
| crates/verification-engine/tests/store.rs | evidence_plan_scope_listing_and_detail_get（计划域列表 + 单条回读 + 守卫负例） |
| apps/server/rp_server.py | handle_research_evidence / handle_research_evidence_detail / handle_research_coverage + API_ROUTES 注册 + GET 动态路径段 |
| web/src/routes/research/shared.tsx | ResearchEvidenceRow/List + CoverageCell/MatrixResponse + EVIDENCE_TYPE_TONES + COVERAGE_DIMENSIONS/LABELS |
| web/src/routes/research/ResearchEvidencePage.tsx | 新文件（证据档页） |
| web/src/routes/research/ResearchCoveragePage.tsx | 新文件（覆盖矩阵页） |
| web/src/App.tsx | 两 lazy 路由（research/evidence、research/coverage） |
| web/src/workspaces.ts | research tabs order 6/7（路由总数 16 → 18） |
| web/tests/workspaces.test.ts | 路由钉值 18 + 路径清单同步 |
| web/scripts/rip3_p5_9b_browser_e2e.mjs | 新文件（浏览器端到端 16 断言） |
| docs/rip3/batches/browser-e2e/p5-9b/*.png | 7 张截图入库 |
| docs/rip3/batches/P5-9b-BATCH-REPORT.md | 新文件（本报告） |

工作树用户 WSW 素材 4 项（M docs/README.md、M RIP3-PHASE5-...PLAN.md、?? WEB-SECURITY-WORKBENCH-PLAN.md、?? docs/ui-ux/）不提交不还原，commit 显式 add 本批文件清单。

## D. 数据库迁移、Schema 和兼容性变化

零迁移。SQLite 链头保持 022，PG 链头保持 V0006。evidence 计划域列表走 021 既有索引 ix_research_evidence_plan；evidence-get 走主键点查。API 面纯新增（既有端点零改动），web 路由纯新增（既有 16 路由零改动），compat/契约面零变化。

## E. 测试命令及真实结果（2026-11-18 实测）

### E1 定向测试

```
$ cargo test --offline -p verification-engine
lib 42 passed / provider 2 / store 9（含新 evidence_plan_scope_listing_and_detail_get）0 failed
$ cargo clippy --offline -p verification-engine --all-targets -- -D warnings
零告警
$ cd web && npm run build && npm run test
build 543ms；vitest 19 files / 124 passed（路由钉值 18 同步后全绿）
```

### E2 端点冒烟（实拍库 /tmp/p5-9b-e2e/verify.db，rp_server 7801 + token 文件）

```
$ curl -H "Authorization: Bearer $TOKEN" "http://127.0.0.1:7801/api/research/evidence?plan=vp-20261117-205207-399-03d6"
{"plan_id":"vp-…-03d6","items":[8 条双帧（provider_result + normalized_result ×4 项）…]}
$ curl … "http://127.0.0.1:7801/api/research/evidence/re-20261117-205208-965-02a3"
{"content_hash":"135a7bdd…","evidence_type":"provider_result",…}（单条详情）
$ curl … "http://127.0.0.1:7801/api/research/coverage?plan=vp-…-03d6"
{"cells":[七维五态分计数],"blind_spots":["identity_permissions","file_processing","data_flows"]}
负例：无 scope 参数 → 400 bad_evidence_scope；非 re- id → 400 bad_evidence_id
```

### E3 execute-plan 修复回放（v3 计划 vp-20261117-205207-399-03d6，4 项四维）

```
$ rp-verify execute-plan --plan vp-…-03d6 --template /tmp/p5-9b-e2e/template.yaml --network none
vi-…-551-0284 → NOT_REPRODUCED（clean run (exit 0): template loaded, zero matches under current conditions）
vi-…-708-012d → NOT_REPRODUCED / vi-…-874-00ad → NOT_REPRODUCED / vi-…-023-038b → NOT_REPRODUCED
$ rp-verify evidence-list --plan vp-…-03d6
8 条（re-20261117-205208-965-02a3 provider_result hash 135a7bdd… trace tr-exec-…-1794948728335429 三面贯通）
$ rp-verify coverage-show --plan vp-…-03d6
四维各 1 项 not_reproduced；盲区 3（identity_permissions / file_processing / data_flows uncovered）
```

附带预算面实录（v1 计划 vp-…-0069，max_requests 15）：首项派发消费后 runtime 预算复核将后续 2 项跳过（budget_skipped=2，NOT_TESTED + budget_exhausted 留痕）——P5-6 运行期预算面在真实链路复现，与本批修复无关。

### E4 CI

本批 commit push 后 `gh run watch` 收口，红则修复批再进下一批（P5-7 dbb582d1 先例）；终态记入 HANDOVER 与 P5-9c 开工核对。

## F. 性能数据

本批无新增基准项。端点为 rp-verify 子进程单次调用（列表毫秒级，8 行实测 <100ms）；coverage-show 为 O(七维×项数) 纯函数推导（P5-8 既有）。无性能门禁面变化。

## G. 未解决问题与风险

1. §4.7 完整范围（HTTP Request/Response 对照、Headers/Cookies 分区、截图 Artifact 预览）随 WSW 流量面批次；本批 payload 是 nuclei JSON 快照（无截图 artifact 面），如实不编造。
2. §4.8 行级明细（点开单元格显示 Task/Item/Evidence 与未覆盖原因）随数据面补齐批次；本批按维度聚合呈现 + 分母明示。
3. Coverage 页表头五态列在窄屏下拥挤（th 文字相邻无间隙视觉），390 窄屏无横向溢出（overflow 0px）但列头紧凑，视觉细化留页面族批。
4. egress_allowlist 派发面真实目标实拍（P5-5 G1）与 finding-link 正例 RTIS 写面实拍（P5-7 G1）随 P5-9c 本地靶场，本批如实未做。
5. plans 列表含 v1（INCONCLUSIVE + budget_exhausted）与 v3（NOT_REPRODUCED）两计划，浏览器验收以 v3 为数据源；v1 留作预算面实录样本。

## H. 下一批依赖和明确的非目标

P5-9c（本地靶场实拍与端到端验收）依赖：本批真实执行链贯通（`-t` 修复后本地靶场 CONFIRMED 可达）+ Evidence/Coverage 页在位（六场景 F 证据闭环演练）+ P5-7 G1 / P5-5 G1 遗留实拍位。非目标：不接 ZAP、不动 runtime-gateway 内核、不做 Report Intelligence 与知识回流、不做 Proxy/MITM。

## I. UI 截图与浏览器验收证据

脚本 web/scripts/rip3_p5_9b_browser_e2e.mjs **16/16 断言全过**（1600 宽屏 ×3 场景 + 终态计划 + 390 窄屏 ×2 页），截图入库 docs/rip3/batches/browser-e2e/p5-9b/：

| 截图 | 断言 |
| --- | --- |
| 01-evidence-empty-state.png | 空态诚实提示（未选计划不伪造数据） |
| 02-evidence-detail-raw.png / 03-evidence-detail-pretty.png | 列表 8 行 + 详情 SHA256/六 ID 锚 + Raw/Pretty 切换 |
| 04-coverage-empty-state.png / 05-coverage-matrix.png | 七维行 + 五态列分开 + 盲区 uncovered 三维 + 分母明示 |
| 06-plans-terminal-state.png | 终态计划零操作按钮（不渲染编造操作） |
| 07-coverage-390.png | 390 窄屏无横向溢出（两页 overflow 0px） |

人工抽检（Read 视觉复核）：03 详情页（列表/详情双栏 + 六 ID 锚 + Pretty JSON）与 05 矩阵页（七维双语行 + 盲区框）渲染质量符合既有视觉体系（dark 主题 token 沿用）。

## J. RTIS 对账和数据安全回归结果

本批零 RTIS 接触（无 rt.py / rtis MCP 调用）；commit 前 `git diff integrations/rtis/` 为空已核对。POC 正文零复制（evidence payload 为本机 e2e 快照，无 RTIS 条目内容）。生产 migrations/platform*.db 零触碰（实拍库 /tmp/p5-9b-e2e/verify.db）。实拍目标 127.0.0.1:9443 loopback 本机面（network none），零外联。

## K. 本批教训实录

1. **nuclei 目录装载静默过滤**：nuclei 3.11.1 对缺 author/description 的模板 `-validate` 静默通过但目录装载过滤出局（-tl 零列出 → FTL no templates provided）。调用面模板规格 = info 必含 name/author/severity/description，验收用 `-tl`（列表数）不用 `-validate`（静默面）。
2. **装载链路缺陷的最短定位路径**：容器 FTL 先看宿主侧 ws in/out 目录（模板 cp 是否到位）再跑容器内 `-tl`/`-validate` 二分——本例 cp 成功 + validate 静默 + tl 零列出，把根因从"旗标缺失"二分到"模板被过滤"再合流（两因并存：旗标缺 + 模板缺 author）。
3. **RP_DB_PATH 逐命令携带**（P5-8 教训复现）：本批首次 plan-create 漏带 env 落内存库（plan_id vp-…-0069 生成了但库即弃），后续每命令 export 重跑——症状即时可见（下一命令 not_found），如实留档。
