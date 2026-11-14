# 更新说明 · 2026-11-14（Phase 9 阶段总账：对外能力面三栈与双通道一致性）

> Phase 9 全段交付完成后的总账。用户可感知的变化：外部 AI 第一次有了完整的平台接入面（MCP stdio 服务器 rp-mcp：目录分页发现 + 按需 Schema + rip.run_tool 执行 + 运行查询与取消 + rip:// 七 resource 与 prompt 三条剧本；REST 契约面收口：trace 透出、全局限频、访问审计、端点机器对账；CLI 帮助面 11 bin 统一口径）；MCP 与 REST 两通道的业务逻辑一致性第一次有了常驻 CI 门禁（rp-mcp-parity 载具，五个业务操作双通道同断言，ROADMAP §10.5 风险 1 的债从「靠自觉」变成「红灯阻断」）；Tool Center 页第一次露出 MCP 目录同步状态卡（目录条数 + 读时对账结果）。本文件是阶段总账，分批实施细节以各批 RELEASE 为准，不在此重写。

| 项         | 值 |
| ---        | --- |
| 状态       | 生效中（Phase 9 阶段总账；P9-4 收口批次随本提交入库，git 短哈希按仓库回填惯例由后续 docs 批次补记，先例 `05ed5ee`/`f02d9b3`） |
| 日期       | 2026-11-14 |
| 适用版本   | Phase 8 收口（`8f0446b`）及之后 |
| 阶段交付   | P9-1 MCP 目录发现面（crates/mcp-server 新 crate）→ P9-2 rip.run_tool 端到端执行通道 → P9-3 REST 契约化收口与 CLI 帮助面 → P9-4 双通道一致性载具 + Resources/Prompts + Tool Center MCP 状态卡（本批）。ROADMAP §10.3 的 P9-5（rip:// 七 resource 与 prompt 面）按批次编排并入 P9-4 交付（本批 ask 原文点名），Phase 9 至此五批内容四批全交付 |
| 数据库迁移 | 阶段累计 019_external_api_audit.sql（P9-3，external_api_audit append-only 表，rp_server 零 SQL 写纪律不动摇，表定位为审计 JSONL 流的 SQL 存储形状）；P9-4 零新增迁移；生产 platform.db 与 platform-dryrun.db 全程未应用（本批 selftest 前后指纹一致复测） |
| 自动化测试 | `cargo test --offline --workspace`：608 passed 0 failed（本批入库前全量实测；P9-3 基线 603 + rp-mcp-parity 5 载具用例）；web vitest 17 文件 115 passed（P9-3 基线 111 + mcp-smoke 4） |
| 静态检查   | `cargo clippy --offline --workspace --all-targets -- -D warnings` 0 error（本批实测）；web typecheck/build/vitest/视觉门 59 passed/prettier --check 全过（本批实测） |
| 相关       | docs/phase0/PHASE-ROADMAP.md §10、docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md §11:640-679、docs/phase9/PHASE9-DESIGN.md（§一到 §十二）、docs/phase9/REST-API-CONTRACT.md、docs/guides/CLI-REFERENCE.md、docs/guides/EXTERNAL-AI-QUICKSTART.md、分批 RELEASE 三份（P9-1/P9-2/P9-3，见「一、」表） |

## 一、批次 1 到 4 汇总

| 批次 | 内容 | 关键交付 | 记录 |
| --- | --- | --- | --- |
| P9-1 | MCP 目录发现面 | crates/mcp-server（bin rp-mcp，stdio NDJSON JSON-RPC 2.0，MCP 2024-11-05 子集）：initialize 握手鉴权（RP_MCP_TOKEN 禁止匿名，常时比较）+ tools/list 分页目录（能力面 rip.* 条目 + 注册面 tool 表 1:1 投影，与 Tool Center 同源同一读面，aux 增量合并）+ rip.get_tool_schema 按需 Schema；初始化不携带任何工具全文说明（计划 §11:658）；双 MCP 边界延续（rip.* vs redteam_*） | [RELEASE_2026-11-14_PHASE9-P9-1.md](RELEASE_2026-11-14_PHASE9-P9-1.md) |
| P9-2 | rip.run_tool 端到端执行通道 | dispatch.rs 三件事（请求整形 / tool-engine Engine 受理 D-TC-05 / 既有适配器驱动两条路径）+ Artifact + Evidence 登记（P7-2 EvidenceRegistrar 同一实现，face=rp-mcp）+ rip.get_run_status / rip.cancel_run 运行面 + rp-tool run/status/cancel 对外子命令（最小 MCP 客户端同一通道）+ T2 authorization_ref 校验保留 + 运行库 RP_RUN_DB 隔离（生产 platform.db 零写） | [RELEASE_2026-11-14_PHASE9-P9-2.md](RELEASE_2026-11-14_PHASE9-P9-2.md) |
| P9-3 | REST 契约化收口与 CLI 帮助面 | trace 六 ID + correlation_id 透出（响应体 + X-Trace-Id/X-Correlation-Id 头）+ 全局限频（RP_RATE_LIMIT_RPM，429 + Retry-After）+ 对外访问审计（RP_API_AUDIT_LOG JSONL + 019 SQL 形状）+ 子进程超时常量化 + 契约文档 docs/phase9/REST-API-CONTRACT.md 与 rp_server.py 机器对账（selftest 五重断言）+ rp-* 11 bin 帮助面统一（--help exit 0 / 误用 exit 2）+ docs/guides/CLI-REFERENCE.md | [RELEASE_2026-11-14_PHASE9-P9-3.md](RELEASE_2026-11-14_PHASE9-P9-3.md) |
| P9-4 | 双通道一致性载具 + Resources/Prompts + MCP 状态卡（本批） | ① crates/rp-mcp-parity 常驻载具（五个业务操作：列工具/查 Schema/跑工具/查 Run/查 trace，REST 真实 rp_server.py HTTP 进程 vs MCP 协议面，断言同一输出形状与同一审计链，ROADMAP §10.4 验收命令 `cargo test --offline -p rp-mcp-parity`，§10.5 风险 1 常驻 CI 门禁）+ .github/workflows/ci.yml parity job；② mcp-server resources.rs（rip:// 七 resource：静态四件 resources/list + 模板三件 resources/templates/list，rip://runs/<run_id> 与 rip.get_run_status 同一投影，rip://evidence/<id> 证据行 + Artifact 快照文本内联 64KiB 上限）+ prompts.rs（三条剧本：rip.tool_run_playbook / rip.run_audit_review / rip.knowledge_brief）+ initialize capabilities 增 resources/prompts；③ MCP 通道审计链自锚定（submit_correlated trace_id = run_id，015/018「run 即链锚」同款惯例，REST /api/traces 可反查 MCP 通道运行）；④ rp_server GET /api/mcp-catalog（目录条数 + 读时对账结果，API_ROUTES 16→17 条）+ Tool Center MCP 目录同步状态卡（web/src/routes/ToolCenterPage.tsx McpCatalogCard 四态）+ mcp-smoke 4 用例 + mcp-status.spec.ts 双模式（route-mock 三态 + 真实后端） | 本文件（§二到 §七） |

## 二、P9-4 批次实施细节（本批）

### 1. 一致性载具（crates/rp-mcp-parity）

- crate 形态：占位 lib + tests/parity.rs 五用例（op1 到 op5 与判据一一对应），`cargo test --offline -p rp-mcp-parity` 单命令全绿（0.88s 实测）。依赖全部 workspace 既有 crate（mcp-server 走 path 依赖），HTTP 客户端为 std TcpStream 薄实现（vendor 无 HTTP client crate；rp_server HTTP/1.0 + Content-Length + Connection: close，读至 EOF）。
- 通道口径：REST 通道 = 真实 rp_server.py 子进程（Bearer 鉴权 + RP_AUX_DB/RP_WORKFLOW_DB 注入 + RP_RATE_LIMIT_RPM=0 防轮询撞限频）；MCP 通道 = `McpServer::handle_line` 协议面（bin rp-mcp stdio 循环同一入口；stdio 循环本身由 mcp-server tests 与 selftest_phase9_mcp.sh 覆盖，载具不重复）。两通道指向同一临时主库（001 水位，platform.db 同款水位）+ 同一临时运行库（全量迁移 + Registry::register 登记，rp-tool register 库面同款）。
- 断言面（五操作）：
  1. 列工具：REST /api/tools 与 MCP rip.list_tools 的 total/returned/工具 id 序列（同排序）/逐条核心字段（kind/lifecycle/governance/permission_level/source_system/updated_at）逐项一致；/api/mcp-catalog catalog_count 与能力面词表 == MCP wire 面（tools/list 能力面逐名对照）；
  2. 查 Schema：REST /api/tools?tool_id= 与 rip.get_tool_schema 的 input/output Schema JSON 逐字节一致，source（aux 回退）一致；
  3. 跑工具：REST POST /api/workflow-runs（wf-p8-tool 样例受理 + 轮询终态）与 rip.run_tool 都走 tool-engine 同一受理链；REST tool 节点 output == MCP envelope result（同一引擎同一输入确定性输出，全等对照）；审计链锚各自在场（workflow run trace_id = wfr-id；MCP run 自锚定 trace_id = run-id），workspace 按通道身份区分（ws-workflow / ws-mcp）；
  4. 查 Run：两通道互读对方 run（同一运行库）：rip.get_run_status 读 REST 通道 run 得同一链锚；REST /api/runs?run_id= 读 MCP run，correlation_id / evidence 关联集 / artifact 关联与 MCP 信封引用集一致，P9-3 关联头对 MCP run 同样在场；未知 run 两通道同一「不存在」语义（404 no_run / VALIDATION）；
  5. 查 trace：REST /api/traces?id=<run_id> 对两通道 run 各自解析出链锚，MCP resources/read rip://runs/<run_id> 同链锚同 evidence 集；rip://evidence/<id> 实读证据行 + Artifact 快照文本。
- 前置守卫：python3 在位（V-9）；rp-workflow 二进制缺位时载具内 ensure_bin 按 `cargo build --offline` 自动构建（vendor 在库，CI 与新机器开箱可跑）。

### 2. Resources 与 Prompts（计划 §11:662-668 + P9-5 内容并入）

- 七 resource 落法：静态四件（rip://workspace/current、rip://knowledge/catalog、rip://plugins/catalog、rip://tools/catalog）进 resources/list；模板三件（rip://tasks/{task_id}、rip://runs/{run_id}、rip://evidence/{evidence_id}）进 resources/templates/list（MCP 2024-11-05 协议口径，不把模板伪装成具体资源）；resources/read 按 URI 分发，词表外 -32602、实体不存在 -32602、数据面不可读 -32603（reason 词表同目录读面）。
- 读面复用：rip://runs/<run_id> 与 rip.get_run_status 同一 dispatch RunChannel::run_status 投影（不建第二套读面）；rip://tools/catalog 与 rip.list_tools 注册面同一 list_registry；目录四件数据面不可读时返回 available:false + reason 的诚实空态载荷（rp_server 读面口径），不报错不造假数据。
- rip://evidence/<id>：证据行（运行库优先，主库回退，trace_id 列按库水位条件读）+ storage_ref 指向的 Artifact 快照文本内联（64 KiB 上限，超出截断并标注，按 storage_ref 路径取件）。
- prompts 三条（prompts/list + prompts/get，role=user 单 text 消息子集口径）：rip.tool_run_playbook（八步剧本，计划 §11 外部 AI 工作方式提示词化）、rip.run_audit_review（审计链复核剧本）、rip.knowledge_brief（知识简报取材剧本）；未知 prompt / 缺必填参数 -32602。
- initialize capabilities 增 `"resources": {"subscribe": false}` 与 `"prompts": {"listChanged": false}`；instructions 保持一行导航（<200 字符断言延续）。
- 插件目录读面新增 RP_PLUGIN_DB 环境变量（缺省 migrations/dev-p6.db，rp_server RIP_PLUGIN_DB 同默认；RP_PLUGIN_DB=none 显式关闭；库缺席返回诚实空态）。

### 3. MCP 通道审计链自锚定（dispatch.rs 行为演进，P9-4）

- 变更：rip.run_tool 受理由 `Engine::submit` 改为 `Engine::submit_correlated(&request, Some(run_id))`（tool_run.trace_id = run_id），登记 Registration.trace_id = Some(run_id)（artifact/evidence 行同链，015 单锚传播）。此前 MCP 通道 trace_id 全 NULL，REST /api/traces 无法反查 MCP 通道运行；自锚定后两通道运行都能经同一 trace 面回查（载具 op5 断言面）。
- 惯例依据：015/018「run 即链锚」（workflow run trace_id = run_id 先例），workflow tool 节点用同一 submit_correlated 入口；T2 校验、词表、拒绝不落行等受理语义零变化（correlated 仅在 submit 成功后 UPDATE trace_id 列）。P9-2 RELEASE 登记的「trace_id: None」快照不回改，以本文件为权威口径。

### 4. /api/mcp-catalog 与 Tool Center MCP 状态卡

- rp_server 新增 GET /api/mcp-catalog（API_ROUTES 16→17 条，契约文档 ENDPOINTS 段同步 + selftest_phase9_rest.sh 对账断言 16→17/17→18 随批演进）：catalog_count（与 MCP tools/list 注册面 total 同一读面，handle_tools 合并逻辑抽为 _tool_catalog_rows 共用实现，不建第二套合并逻辑）+ capability_tools 词表（mcp-server lib CAPABILITY_TOOLS 六条同源镜像）+ 读时对账结果（注册面投影完整性：tool_id 非空且无重复、lifecycle/governance 落词表；drift 如实列出）。rp_server 只读纪律不落状态行，「最后对账结果」= 本次读取时点对账（reconciled_at），跨通道 wire 级对账由 rp-mcp-parity CI 门禁常驻承载（端点 note 字段明示这一定位）。
- web：ToolCenterPage 新增 McpCatalogCard（独立取数面 /api/mcp-catalog，不随目录过滤重取）：对账正常 / 对账漂移 N 条（drift 逐条展示）/ 数据面不可读诚实空态 / 读取失败降级说明（明示「MCP 工具目录本身可用，仅状态卡不可用」）四态；载荷形状防御（reconciliation 缺失按暂不可读处理，不炸页面）。web/src/lib/api.ts 增 McpCatalogStatus 类型与 fetchMcpCatalog。
- 测试：web/tests/mcp-smoke.test.tsx 4 用例（vitest）；web/tests/mcp-status.spec.ts + web/playwright.mcp.config.ts 双模式 Playwright（模式 A route-mock 三态；模式 B RP_P9_E2E=1 真实后端，P6-8 同款 route 转发链路）；视觉门 /tools 页补 /api/mcp-catalog stub（55 用例全绿，marker 不变）。

## 三、五道门禁数字（本批实测）

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| 1. cargo test | `cargo test --offline --workspace` | 608 passed 0 failed（P9-3 基线 603 + parity 载具 5）；`cargo test --offline -p rp-mcp-parity` 5 passed 0 failed（0.88s，ROADMAP §10.4 验收命令原形） |
| 2. clippy | `cargo clippy --offline --workspace --all-targets -- -D warnings` | exit 0，0 error |
| 3. worker unittest | `python3 -m unittest discover -s workers/python/tests` | Ran 211，OK (skipped=11)（本批零 workers/python 触点，回归复测） |
| 4. web 门禁 | typecheck / build / vitest / 视觉门 / prettier | typecheck exit 0；build exit 0；vitest 17 文件 115 passed（基线 111 + mcp-smoke 4）；视觉门 59 passed（pages.spec 55 + workflows.spec 4，PW_CHROMIUM 指向用户缓存 chromium-1234，axe color-contrast findings 0）；prettier --check 全过 |
| 5. selftest | selftest_phase9_rest.sh / selftest_phase9_mcp.sh / selftest_auth.sh | 33/33（第 10 段对账断言随批演进 17 条路由）/ 23/23 / 11/11（SELFTEST_PORT=7825/7826，7800 常驻工作台守卫场景延续）；生产 platform*.db 指纹前后一致零写 |

浏览器实测（纪律 §九硬性要求）：真实 Chromium（用户缓存 chromium-1234，PW_CHROMIUM 注入，零系统级安装）走真实 rp_server（127.0.0.1:7832，生产主库只读 + fixture 运行库副本）逐区实测 Tool Center MCP 状态卡，截图 4 张入库 docs/phase9/browser-e2e/（状态卡特写 / 目录列表区 / 桌面整页 / 移动视口），route-mock 三态 3 张在 mock/；证据链与复跑命令见 docs/phase9/browser-e2e/README.md。

## 四、三面同源与双通道一致性速查（本批交付的对账口径）

三面同源锚点：REST、MCP、CLI 三个对外面共享同一读面与同一执行链（工具目录同一 tool 表投影，/api/tools、MCP tools/list 注册面、/api/mcp-catalog catalog_count 三处同一读面，P9-4 抽 _tool_catalog_rows 共用实现不建第二套合并逻辑；执行同一 tool-engine Engine 受理链；运行查询同一运行库投影，rip://runs/<run_id> 与 rip.get_run_status 同一 RunChannel::run_status）。下表为 REST 与 MCP 双通道逐操作对账（载具 crates/rp-mcp-parity 常驻断言），CLI 面见段末。

| 业务操作 | REST 通道 | MCP 通道 | 一致性断言落点 |
| --- | --- | --- | --- |
| 列工具 | GET /api/tools | rip.list_tools（tools/call） | rp-mcp-parity op1（total/id 序列/逐条字段）+ /api/mcp-catalog 对账 |
| 查 Schema | GET /api/tools?tool_id= | rip.get_tool_schema | op2（Schema JSON 逐字节） |
| 跑工具 | POST /api/workflow-runs（workflow 异步受理 + 轮询） | rip.run_tool（同步信封） | op3（同一 tool-engine；节点 output == envelope result；链锚各自在场） |
| 查 Run | GET /api/runs?run_id= | rip.get_run_status | op4（互读对方 run；correlation/evidence/artifact 关联一致） |
| 查 trace | GET /api/traces?id=<run_id> | resources/read rip://runs/<run_id> | op5（同一链锚 + 同一 evidence 集） |
| 目录状态 | GET /api/mcp-catalog | tools/list 能力面 + 注册面 total | op1（能力面词表 wire 对照 + catalog_count 三面一致） |

CLI 面（同源第三面）：rp-tool run/status/cancel 是最小 MCP 客户端（spawn rp-mcp 子进程一次性 stdio JSON-RPC 交换，RP_MCP_BIN 可指路），与外部 AI 走同一执行通道（P9-2，crates/tool-registry/src/bin/rp-tool.rs；该 crate 不能反向依赖 tool-engine，cargo 禁环，故 CLI 与 MCP 共用 dispatch.rs 单一实现）；rp-* 11 bin 帮助面 P9-3 统一口径（--help exit 0 / 误用 exit 2，逐 bin 参考与可复制命令见 docs/guides/CLI-REFERENCE.md，机器断言 selftest_phase9_rest.sh 第 11 段）；CLI 不另起第二套派发与接口风格。

## 五、鉴权与限频边界

依据：docs/phase9/REST-API-CONTRACT.md §三（鉴权，selftest_auth 11 项同源）、§六（限频）、§八（审计）与 P9-1/P9-2/P9-3 RELEASE。逐面边界：

| 面 | 鉴权 | 限频 | 审计 |
| --- | --- | --- | --- |
| REST（rp_server /api/*） | Authorization: Bearer，令牌优先级 RP_AUTH_TOKEN > RP_AUTH_TOKEN_FILE（缺省仓库根 .rp-workbench-token，权限非 600 启动告警），hmac.compare_digest 常时比较（rp_server.py:2713-2721）；未配置任何令牌时 GET 本地开发模式可用，全部 POST 一律 403 auth_required_for_write 不降级放行；写面目标库指向 platform*.db 一律 403 production_db_refused（红线） | RP_RATE_LIMIT_RPM 缺省 600 req/60s，服务进程级全局滑动窗口（check_rate_limit，rp_server.py:186），0 显式关闭；只计 /api/*（GET 与 POST 都计，静态资源不计），先限频后鉴权（401 也计数，保护鉴权面）；超限 429 code=rate_limited + Retry-After: 60；多线程记账加锁 | RP_API_AUDIT_LOG 指向 JSONL 时逐请求落 api_access 事件，缺省关闭零行为变化；019 external_api_audit 为审计流的 SQL 存储形状，rp_server 零 SQL 写，platform*.db 不应用 |
| MCP（bin rp-mcp stdio） | RP_MCP_TOKEN 未配置一律 -32001 拒绝匿名握手；initialize 可选出示 _meta.rp.auth 常时比较（P9-1）；双 MCP 边界延续（rip.* 能力面 vs redteam_* 工具面） | 本机 stdio 进程面无 HTTP 层，REST 全局限频只计 rp_server /api/*，两通道限频边界不交叉；能力面限定 rip.* 条目，工具全文说明不下发（计划 §11:658） | 通道审计链自锚定 trace_id = run_id（P9-4，015/018「run 即链锚」同款惯例），可经 REST /api/traces 反查 |
| CLI（rp-* 11 bin） | 本地进程面不开网络端口；rp-tool 执行类子命令的匿名禁止由 MCP 握手鉴权承载（一次性令牌经环境变量注入 rp-mcp 子进程，P9-2）；rp-plugin/rp-workflow 显式拒绝 --db 指向 platform*.db（production_db_refused） | 无网络限频面；exit 码语义统一（--help exit 0 / 误用 exit 2，P9-3） | 不落 REST 对外访问审计（audit_api_access 只覆盖 rp_server /api/* 请求） |

限频形态取舍（P9-3 已知取舍 2 同口径）：全局预算而非按 IP 分桶，按 ask 原文「全局限频」字面落地；本地单人工作台形态下按 IP 分桶收益有限且稀释保护强度。

## 六、已知取舍

1. MCP 通道用协议面（`McpServer::handle_line`）而非 spawn rp-mcp 二进制：stdio 循环是 15 行薄壳（bin/rp-mcp.rs），由 mcp-server tests 与 selftest_phase9_mcp.sh（真实二进制端到端）覆盖，载具不重复；CI 无需额外二进制编排。
2. /api/mcp-catalog 的对账是读面快照（读时对账，不落状态行）：rp_server 零 SQL 写纪律（Phase 6 静态门禁）不允许它写对账状态表；「最后对账结果」= 本次请求时点结果，常驻跨通道对账由 CI 载具承载，端点 note 字段明示这一定位。
3. rip:// 七 resource 的模板三件在 resources/templates/list 而非 resources/list：MCP 2024-11-05 协议口径（模板不是具体资源）；严格计数七件 = 4 + 3（载具与 selftest 断言口径）。
4. rip://knowledge/catalog、rip://tasks/<task_id> 读主库（ETL 注册面），rip://plugins/catalog 读 RP_PLUGIN_DB（Plugin Center 同源），三者在生产机器上主库水位（V1）决定可用性：缺表返回诚实空态载荷不报错；这是数据面水位现实，不是资源面缺陷。
5. 载具 REST 通道跑在 rp_server 进程内嵌限频缺省关闭（RP_RATE_LIMIT_RPM=0）：轮询 workflow run 在缺省 600 req/60s 预算内也可完成，显式关闭消除 CI 波动面；限频行为本身由 selftest_phase9_rest.sh 独立实例断言覆盖，两处不冲突。
6. web/tests/mcp-status.spec.ts 不进视觉主门禁（playwright.config testDir=qa/visual）：它是状态卡专项双模式 spec（独立配置 playwright.mcp.config.ts，P6-8 plugins.spec 先例同款），主门禁的 /tools 页 4 组合已含状态卡对账正常态。

## 七、回滚方式

- 本批零迁移零生产库写：回滚 = revert 本批提交即可（crates/rp-mcp-parity 整目录、mcp-server resources.rs/prompts.rs/lib.rs dispatch.rs bin、rp_server.py handle_mcp_catalog 与 _tool_catalog_rows 抽取、web 五文件、ci.yml parity job、docs 四件）。_tool_catalog_rows 抽取是行为等价重构（handle_tools 列表分支原逻辑原样搬移，selftest 33 断言回归通过），revert 后 REST 面行为零变化。
- MCP 自锚定如需独立回退（不影响其余交付）：dispatch.rs 两处（submit_correlated 调用与 Registration.trace_id）恢复 None/submit 即回到 P9-2 行为；载具 op3/op5 的链锚断言会相应失败（设计如此：载具是行为契约）。
