# SK-3 安全工具目录与静态审查工具备注（P6-11）

| 项     | 值                                                                                     |
| ------ | -------------------------------------------------------------------------------------- |
| 状态   | 生效中（SK-3 批次内落稿，随批次演进）                                                    |
| 日期   | 2026-11-12                                                                              |
| 基准   | docs/RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md §6.2/§6.3/§7/§8 P6-11、docs/phase0/PHASE-ROADMAP.md |
| 相关   | docs/phase6/SK2-SOURCES-NOTES.md（体例参照）、docs/phase6/SK1-INGEST-ADR.md、docs/phase6/PHASE6-PLAN.md |

## 一、四件工具登记（manifests/tools/ 双份 yaml+json）

| Tool | 级别 | runtime | network | family | 载体 |
| --- | --- | --- | --- | --- | --- |
| rip.knowledge.catalog.extract | T0 | python_worker（catalog-extract-worker） | none | knowledge | workers/python/catalog_worker.py |
| rip.security.extension.audit | T0 | python_worker（extension-audit-worker） | none | knowledge | workers/python/extension_audit_worker.py |
| rip.knowledge.document.fetch | T1 | native_rust（rp-knowledge-fetch fetch） | egress_allowlist（ports 80） | knowledge | crates/tool-engine/src/document_fetch.rs |
| rip.knowledge.source.sync | T1 | native_rust（rp-plugin sync） | egress_allowlist（ports 80/443，仅 git 通道） | knowledge | crates/plugin-runtime/src/source_sync.rs |

静态链证据（crates/tool-registry/tests/sk3_tool_manifests.rs 3 项全绿）：双份逐键相等（serde_yaml_ng 解析 == serde_json 解析）、validate_static 全过（native_rust 分支 binary+subcommand 必填、egress_allowlist 分支 ports 必填）、四件全部 register + discover/query（T0/T1 权限词表如载）、license=unknown 按 rule 5 拒登复验。

权限分级（计划 §6.3）：目录解析与静态审查 T0（无外部副作用），定点抓取与受控同步 T1（对外只读 GET/受控 API）；本批无 T2 工具，T2 形态拒绝语义由既有 Tool Policy Resolver 承载并在 sk3_document_fetch.rs 复验（sk3_t2_shaped_call_without_authorization_ref_rejected：缺 authorization_ref → POLICY_DENIED 且不落 run 行，带引用可受理）。

## 二、catalog.extract（T0 纯解析）

复用优先落地：Markdown 路径直接 import parsing_worker 既有 `extract_markdown_links :56` / `_link_scheme :45` / `extract_frontmatter :131`（SK-1 落地面，零重写）；HTML 锚点抽取按 parsing_worker `_HTMLTextExtractor` 同款 script/style 剔除语义补齐（parsing_worker 的 HTML 面只产文本 token，不抽 href，无可复用函数）。

条目契约（计划 §6.2A 全键）：name / description / canonical_url / raw_url / raw_link_text / category_path / resource_kind / source_location{line,file} / dedupe_key（= 规范化 canonical URL，与 011 item_key 同语义）/ confidence / meta{in_table,host}。确定性口径三条：

1. resource_kind 只看 URL 形状（github/gitlab/gitee owner/repo 两段 = repo，其余 link），不猜内容类型；host 单列 meta。
2. confidence 确定性公式：基线 0.6 + 原链接文字非空 0.2 + 后缀描述非空 0.1 + 分类路径非空 0.1，区间 [0.6, 1.0]，不做任何网络核验。
3. 保留原始 URL：canonical 只做 trim + 剥尾斜杠（与 knowledge_parse.rs normalize_url 同语义）；parsing_worker 在抽取层已剥尾斜杠（SK-1 既有行为），故 md 路径 raw_url 与 canonical 一致，worker 不二次猜造。

分页截断：max_entries 作用于去重后序列，超量置 truncated=true 并给 next_offset（offset 分页）；stats 记 total/kept/scheme_rejected/no_scheme/deduped/returned/unique。双重防御：extract_* 已按 scheme 白名单过滤后，产出前再以 _link_scheme 复核一次（scheme_rejected_final 恒 0，回归时有计数）。

测试（test_catalog_extract.py 37 项 = 契约 24 + 单元 13）：scheme 拒绝（javascript:/data:/file: 计数）、围栏与 frontmatter 跳过、去重首见胜出、截断分页 next_offset、category_map 重映射、HTML script/style 剥除与锚点行号、不猜造 URL（相对链接只计数不造 URL）、resource_kind/confidence、输入校验 8 例全拒（-32602）。

## 三、extension.audit（T0 纯静态，零执行）

零执行能力自证（tests/test_security_extension_audit.py AST 断言锁定）：worker 源码 import 面只允许 `__future__/json/pathlib/re/sys/worker_runtime`（subprocess/socket/urllib/requests/http/os/shutil 等 15 类禁止），调用点禁 eval/exec/compile/__import__/open 裸名与 system/popen/spawn/Popen 属性族；审查 fixture（含 curl|sh 安装钩子）时不存在任何可执行被审查物的代码路径。

检查面 12 类（计划 §6.2B 清单逐项落码）：许可与版权声明（LICENSE/COPYING 文件 + Manifest 声明交叉核验 + SPDX 识别）、版本/Commit pin（floating 分支 high、短哈希 medium）、依赖锁文件（package.json 无 lock、go.mod 无 go.sum）、安装脚本（curl|sh、package.json pre/postinstall 带网络取包）、命令执行入口、网络与文件系统能力（info 级 + required_permissions）、Secrets 读取、容器边界（privileged/docker.sock/host namespace）、权限升级（sudo/chmod 777/setuid）、MCP tools 输入输出 Schema（缺 inputSchema high；执行类 tool 名配无约束 Schema critical）、Prompt Injection/越权指令迹象、维护状态（只来自调用方显式元数据，不联网核对）。

输出契约：findings[]（id/severity/file/line/evidence≤200 字符/rule）+ counts_by_severity + risk_level（none/info/low/medium/high/critical 取最大）+ review_status（high/critical → BLOCKED_FOR_REVIEW；medium/low → NEEDS_REVIEW；否则 STATIC_CLEAN）+ required_permissions + license_status + supply_chain_notes + 固定 disclaimer（「只覆盖可见文本的迹象检查，不证明代码安全，不替代人工审查」）。

分级裁定两条（批次内定案）：license unknown 统一 high（计划 §7.1 目录许可不传递，tool-registry rule 5 语义上的登记阻断项，与锁文件缺失的 medium 分层区分）；私网 URL 在审查面给 high（AUD-NET-01）与云 metadata 端点单独成 rule（AUD-NET-02），公网对照条目零误伤（测试锁定）。

## 四、document.fetch（T1，复用 egress_proxy 不重写）

复用面：`adapter/egress_proxy.rs` 的 `host_from_url :151`（host 派生）、`EgressAllowlist`/`EgressProxy::spawn_for_run :215`（run 级白名单代理，绝对形式 GET 转发 + 代理层逐请求 allowlist 复核），策略面双重防御。新增面（egress_proxy 没有的能力，属补齐）：私网分类（`private_reason`）、DNS 全址复核、重定向逐跳、限额与 MIME 闸、approved 闸、012 落库。

闸序（check_hop，私网先于 allowlist）：scheme（本批仅 http，https 按 scheme_unsupported 结构化拒绝，不静默降级）→ host 派生 → 主机名内网先拒（localhost/.local/.internal 等）→ IP 字面量直判 → 端口 → DNS 全地址私网复核（任一地址命中 loopback/RFC1918/link-local（169.254.169.254 属 169.254/16）/CGNAT 100.64/10/ULA/文档段/IPv4-mapped 即拒）→ allowlist 精确匹配。连接使用已复核 IP 语义由逐跳复核承载（防 DNS rebinding TOCTOU 的面在重定向每跳重复全闸）。

approved 闸（计划 §6.2C「只接受 Registry 已批准 canonical URL」）：URL 必须命中 knowledge_source_entry 中 review_status='APPROVED' 的 canonical_url（entry_key 给出时精确到条目且 URL 必须相等）；fail-closed：无 --db 无 Registry 即拒绝。落库：012 knowledge_document_fetch（ok 与 rejected 双态只追加，fetch_id 内容寻址，本模块 record_fetch 是唯一写者），正文默认不落盘只落哈希。

传输面已知边界（批次裁定，登记 manifest known_limits）：workspace 无 TLS 依赖栈（Cargo.lock 无 rustls/reqwest/openssl，新增 TLS 依赖需独立批次评估 vendor/离线构建），本批传输面仅 http 明文，端口取 80。真实出网冒烟用 example.com（http 200）完成，https 语义按结构化错误码拒绝验证。

冒烟抓出的真 bug 与修正（2026-11-12 实测）：example.com 以 chunked 编码响应，初版读取面把 chunk 分帧记进正文（哈希 8dade617... 失真，577+12=589 字节）；修正 read_http_response 支持 Content-Length 精确取 / chunked 解帧（RFC 7230）/ close 定界三种形态后，落库哈希 25ddf2c8...（577 字节）与 curl 同头独立取回的 sha256sum 逐字节一致。

## 五、source.sync（T1，只调受控 API 不直写 SQLite）

编排序（source_sync.rs）：pin 前置闸 → 幂等键 → 包统计 + 限额 → 重试摄取 → 报告。写面只有受控 Ingest API（ingest_knowledge_source_with_opts，dry-run SAVEPOINT 回滚）、audit_event（KNOWLEDGE_SYNC/KNOWLEDGE_MAP_CANDIDATES 两新动作词形）与 state/sync-keys.jsonl 幂等键侧车（非数据库）。

pin 前置闸：git 来源 pinned.kind 只接 commit/tag/release（floating 分支 → floating_ref_rejected，fail-fast 不触源），kind=commit 必须全 40 位（完整 Commit 语义，比 registry rule 5 的 7-64 短哈希更严）；local_dir 的 pinned（若给）必须 commit。与 P6-4 registry 侧 floating_branch_rejected 双闸独立（sk3_source_sync.rs 两闸都断言）。

限额与失败：max_files/max_bytes 超限 fail-fast 不摄取（partial_failures 记「ingest not attempted」）；max_seconds 为墙钟预算（摄取为同步调用不可中断，超限如实计入 limits_exceeded，不伪装中断）；失败重试逐次机器可读（attempts[] index/ok/error_code/error_message），重试间隔固定 100ms。幂等键：命中既有成功 sync 键整体跳过（idempotent_skipped=true，final_report=null，零落盘）；键台账只在实跑成功后追加。

候选映射（计划 §8 P6-11 第 3 步）：resource-engine `SqliteResourceRepository::promote_knowledge_candidates`（sqlite.rs 新增受控写面）只映射 review_status='APPROVED' 的 skill_candidate 行（与 Search/Context 双闸同口径），落 resource 表 lifecycle=CANDIDATE / governance=ACTIVE / resource_type=skill_candidate，resource_id=`kse:<plugin_id>:<item_key>` 可回跳源行，INSERT OR IGNORE 幂等；条目与插件皆缺 URL 时拒映射不猜造（skipped_missing_url）。rp-plugin sync --map-candidates 是操作入口（映射 + 审计留痕）；首批 Tool Pack 只收本地静态分析/代码审计/依赖检查类的选择政策落在 skill-pack fixture 类候选上（tests/fixtures/sk3/FIXTURE-NOTES.md 第 4 条）。

## 六、拒绝矩阵实测记录（六类反例，批次验收逐项）

| 反例类 | fixture | 断言面 | 实测 |
| --- | --- | --- | --- |
| 浮动分支 | tests/fixtures/sk3/floating-branch/manifest.yaml | registry rule 5 floating_branch_rejected + sync floating_ref_rejected + audit AUD-PIN-01 | 双闸皆拒且零行落库（sk3_source_sync.rs）；audit risk=high BLOCKED_FOR_REVIEW |
| 无许可证 | tests/fixtures/sk3/no-license/ | audit AUD-LIC-01 + tool-registry rule 5 | license_status=unknown、registration blocked 语义在 supply_chain_notes |
| 安装钩子（curl|sh 类） | tests/fixtures/sk3/install-hook/ | audit AUD-INSTALL-01/02 + 零执行 AST 断言 | high≥3、BLOCKED_FOR_REVIEW；审查过程零子进程零网络（AST 锁定 import/调用面） |
| 越权 Shell | tests/fixtures/sk3/overreach-shell/SKILL.md | audit AUD-OVERREACH-01(critical)/AUD-INJECT-01/AUD-PRIV-01 | risk=critical，rm -rf 证据行落 findings |
| 私网 URL | tests/fixtures/sk3/private-url/catalog.md | audit AUD-NET-01/02 + document.fetch private_network_blocked | 审查面 127.0.0.1/169.254.169.254/192.168.1.1/localhost 全标 high、公网对照零误伤；fetch CLI 对 127.0.0.1 与 169.254.169.254 实拒（private_network_blocked，exit 1） |
| 危险 MCP Schema | tests/fixtures/sk3/mcp-dangerous/.mcp.json | audit AUD-MCP-03(critical)/AUD-MCP-01 | run_shell 无约束 Schema critical + no_schema_tool 缺 Schema high；受约束 read_config 对照零命中 |

T2 形态复验：sk3_t2_shaped_call_without_authorization_ref_rejected（tool-engine）缺 authorization_ref → MissingAuthorization(POLICY_DENIED) 且 run 行数为零。

## 七、CLI 实跑链冒烟（RIP_DATA_DIR=/tmp/sk3-data，--db /tmp/sk3-smoke.db = migrations/dev-p71.db 副本）

| 步骤 | 命令（均实跑） | 关键回显 |
| --- | --- | --- |
| install | rp-plugin --db /tmp/sk3-smoke.db install --manifest manifests/plugins/knowledge.awesome-hacking.yaml --package tests/fixtures/sk2/awesome-hacking --trust community | knowledge.awesome-hacking 0.1.0 staged |
| sync dry-run | rp-plugin sync knowledge.awesome-hacking --dry-run --idempotency-key smoke-001 | ok=true dry_run=true counts.catalog_entry=85 pin=commit 440486fd...；kse rows=0（python file:ro 复核） |
| sync 实跑 | 同上去 --dry-run | ok=true counts 85 skipped=0 |
| sync 幂等二跑 | 同上同键 | ok=true idempotent_skipped=true final_report=null |
| sync 无键三跑 | rp-plugin sync knowledge.awesome-hacking | 摄取层幂等：skipped=85 changed=0 |
| 限额失败 | rp-plugin sync ... --max-bytes 1 | ok=false limits_exceeded=[max_bytes] partial=[limits exceeded: fail-fast]，package_bytes=271676 机器可读 |
| sync-keys 台账 | cat /tmp/sk3-data/plugins/state/sync-keys.jsonl | {"key":"smoke-001","plugin_id":"knowledge.awesome-hacking",...} |
| 审计留痕 | audit_event 实查 | plugin.knowledge.sync × 5（dry-run/实跑/幂等跳过/无键/限额失败全留痕） |
| skill-pack 映射 | install + enable + sync --map-candidates --package tests/fixtures/sk3/skill-pack（先 DISCOVERED 再 review APPROVED） | 首映射 mapped=0（审核闸，skipped_not_approved=2）；review updated=2 后 mapped=2（skipped_existing=0）；resource 表落点 lifecycle=CANDIDATE / governance=ACTIVE / resource_type=skill_candidate / source_system=knowledge-plugin |
| fetch 成功 | rp-knowledge-fetch --db /tmp/sk3-smoke.db --url http://example.com/ --allow-domain example.com --source-id knowledge.smoke | status=ok，content_sha256=25ddf2c8...（577B text/html）与 curl 同头独立互核逐字节一致，012 落行 |
| fetch 非 allowlist | --url http://example.org/ --allow-domain example.com --no-require-approved | rejected host_not_allowlisted，exit 1 |
| fetch 私网 | --url http://127.0.0.1/ 与 http://169.254.169.254/latest/meta-data/ | rejected private_network_blocked（loopback / link_local），exit 1，rejected 行落 012 |

说明：fetch 冒烟的 APPROVED 条目为一次性 dev 库副本中人工登记的 smoke 条目（canonical_url=http://example.com/），只用于 approved 闸与哈希落库冒烟，不入仓不进生产库。

## 八、门禁与回归口径（2026-11-12 实测）

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| workspace 测试 | `cargo test --offline --workspace` | 57 套件 test result: ok，379 passed / 4 failed；唯一失败 podman_host_egress 4 项（panic 原文 podman ps spawn NotFound，本机无 podman，`command -v podman` exit 1，与 SK-1/SK-2 批次预期例外同口径，需有 podman 宿主复测） |
| clippy | `cargo clippy --offline --workspace --all-targets -- -D warnings` | exit 0 零告警 |
| Python 套件 | `python3 -m unittest discover -s workers/python/tests` | 211 tests OK（128 既有 + catalog 37 + audit 46） |
| 登记面回归 | context-broker 登记面断言 20 → 24（acceptance_no_stuffing / catalog_sources）+ 迁移链发现断言 011 → 012（database 三测试文件 + plugin-runtime lifecycle_full_path 重放距离 2 → 3） | 全绿；no_stuffing 预算 8000 → 8200 tokens（SK-3 四工具卡片增量 ~100，no-stuffing 不变式不变，测试内注释声明） |

## 九、已知取舍与移交

1. https 传输面未实现（workspace 无 TLS 依赖栈）：rip.knowledge.document.fetch 本批仅 http 明文，https 按 scheme_unsupported 结构化拒绝；TLS 依赖（rustls 等）的 vendor/离线构建评估归独立批次。tongui 课程正文等 https-only 源的定点抓取待该批次解锁。
2. 冒烟抓出的 chunked 解帧 bug 已修（§四），修正以单测回归（成功路径/重定向/MIME/限额面 12 项）+ 真实出网冒烟哈希互核双证。
3. catalog.extract 的 resource_kind/confidence 为确定性启发（§二三条口径），非内容判断；rust 侧 ingest（knowledge_parse.rs）目前 meta.resource_kind 恒 "link"，worker 侧按 URL 形状细分属登记口径差异（不改 knowledge_parse.rs 的批次约束下如实登记，收敛归 Phase 7 正文模型批次）。
4. skill-pack fixture 安装链验证过程中补齐 fixture Manifest 两处结构缺项（compatibility 块、capabilities[].summary）并把 capability entry 从目录改文件形态（registry rule 7 entry 必须是文件），fixture 已按 v1 schema 全文校验通过；sync 对 local_dir 来源无 --package 时按 source.path（"."）相对 cwd 取包，冒烟以 --package 显式 staging 规避（local_dir 相对路径基准属 Plugin Runtime 既有语义，未改动）。
5. Web 展示（Tool Center 四工具卡片、audit findings 视图）不在本批，归 P6-8 后续 Tool Center 迭代；本批交付止于 Manifest 登记（rp-tool validate/register 全过）与 CLI 可运行面。
6. SESSION-HANDOFF.md 未更新（不在本批路径清单，与 SK-1/SK-2 同口径）。
7. 迁移 012 只在开发库副本与测试内存库执行；migrations/platform*.db 与 integrations/rtis 零触碰（批次红线，commit 前 git status 复核）。
