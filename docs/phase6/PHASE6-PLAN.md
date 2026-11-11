# Phase 6 阶段说明：Plugin Runtime 与 Plugin Center

| 项     | 值                                                                                    |
| ------ | ------------------------------------------------------------------------------------- |
| 状态   | 生效中（P6-1 起随批次滚动更新）                                                       |
| 日期   | 2026-11-11（首次制定，P6-3 批次内落稿）                                               |
| 基准   | 计划 §14 Phase 6（806 到 813 行）、ROADMAP §7（407 到 463 行）、PLUGIN-CONTRACT-DRAFT 全文 |
| 相关   | docs/phase0/PHASE-ROADMAP.md、docs/phase0/PLUGIN-CONTRACT-DRAFT.md、docs/DOC-STANDARD.md |

本文件按参考计划 §24 模板编写，是 Phase 6 的阶段说明（ROADMAP §1.2 末段纪律）。批次验收命令原文与退出码随批次登记在「批次台账」，逐批滚动维护。

## 目标

插件 Manifest、Registry、生命周期；Knowledge/Tool/Project/Agent/Workflow/Integration 六类插件；本地目录、Git、MCP、HTTP 接入；依赖检查、版本固定、启停和升级（计划 §14:806 到 811）。验收口径（计划 §14:813）：至少接入一个外部项目和一个知识插件；插件启停不影响核心数据库；元数据与完整内容按需加载。

## 范围

- 新增 `crates/plugin-registry`（静态面）与 `crates/plugin-runtime`（生命周期面），`schemas/plugin-manifest/` 契约与示例，`migrations/009_plugin.sql` 数据面，`docs/phase6/` 阶段文档。
- 生命周期事件全部经 audit-engine Emit（PLUGIN-CONTRACT §11.4），治理扩展落 quality-engine（§11.5 既有裁定）。
- rip-data/plugins 布局与位置配置（§8）。
- 不修改：生产 migrations（platform*.db）、RTIS 生产行为、M0 到 M8 已交付能力、web 既有页面行为。

## 非目标（本阶段不做）

- Plugin Center 页面（P6-8，宿主为 Phase 5/7 壳层）。
- 五类来源中的 git/archive/mcp_server/http_api 全量接入（P6-4 逐类补）。
- 激活层依赖解析与环检测（P6-7）。
- 升级数据迁移钩子执行与回滚窗口清理（P10-2）。

## 批次台账（验收命令与退出码随批次登记）

| 批次 | 内容（ROADMAP §7.3 原口径）                              | 状态 | 关键落地物                                                                                          |
| ---- | -------------------------------------------------------- | ---- | --------------------------------------------------------------------------------------------------- |
| P6-1 | plugin-registry 静态面：strict 校验、七条、静态链、checksums、lock | 已完成 | crates/plugin-registry（四模块 + 静态链门面）、schemas/plugin-manifest/v1.json + 两示例双份、serde_yaml_ng 0.10.0（仓库首个 yaml 依赖） |
| P6-2 | plugin-runtime 生命周期 + 009 迁移 + 审计 + 治理扩展       | 已完成 | crates/plugin-runtime（跃迁表/retention/upgrade/CLI rp-plugin）、domain::plugin 两实体、009 两表、audit-engine plugin_actions、quality-engine PluginGovernor |
| P6-3 | rip-data/plugins 布局 + 位置可配置重定位 + state/ 文件承载 | 已完成 | paths.rs（resolve 三级优先级 + ensure_layout + stage_package）、state_store.rs（快照原子写 + transitions.jsonl）、CLI 接布局与 state 双写、.gitignore rip-data/ |
| P6-4 | 五类来源接入（local_dir 先行，git/archive/mcp_server/http_api 逐类补） | 已完成 | crates/plugin-registry/src/source/（trait + 五实现 + 信任档位）、lock.negotiation 协商快照、静态链 fetch_reject、tests/fixtures/ 五包、CLI --package 可省走来源 |
| P6-5 | Knowledge 插件落地 knowledge.rtis-redteam（摄取 + 溯源）  | 已完成 | migrations/010_knowledge_ingest.sql、plugin-runtime ingest.rs（权限闸 + 三类语料 + 幂等）、CLI ingest 命令、docs/phase6/RTIS-REDTEAM-PLUGIN-NOTES.md（license 裁决 + 条数收敛） |
| P6-6 | Tool 插件 + 外部 Project 插件（rtis_mcp.mjs project_endpoint） | 已完成 | capability/{tool_register, project_endpoint}.rs、install 深注册接线（stage→validate→checksums→lock→注册）、project.rtis-mcp 示例双份、docs/phase6/DUAL-MCP-BOUNDARY.md |
| P6-7 | 三层依赖与兼容检查（激活层 + 运行层）                      | 已完成 | deps.rs（依赖解析 + requires + 环检测 + 机器可读报告）、compat.rs（rip_core/manifest_schema/worker_protocol 三项）、health.rs（间隔下限 1s + 失败进 ERROR 停止重试 + 探测注入）、enable 激活层接线 |
| P6-8 | Plugin Center 页面                                        | 已完成 | web PluginsPage/PluginDetailPage（四分类 + 三级加载 + 兼容性列 + 依赖图 + 审计 trail）、rp_server /api/plugins 读写端点（Bearer 强制、零直连 SQL 写）、selftest_phase6 PASS=49 FAIL=0、Playwright 真实后端 E2E + 截图 12 张（验收报告 docs/phase6/PHASE6-ACCEPTANCE.md；本行原「未开工」为陈旧状态，2026-11-12 SK-1 批次按验收报告修正） |
| P6-9（收口扩展） | 武器库扩展路径：ADDING-A-TOOL 指南 + 脚手架一条线 + /api/tools aux 增量合并 | 已完成 | docs/guides/ADDING-A-TOOL.md、scripts/rip_new_tool.py、rp-tool validate/register 与 rp-tool-health ping、样例工具 rip.http.tls_inspect 端到端露出（收口记录 docs/releases/RELEASE_2026-11-12_PHASE6.md §三/§四；ROADMAP §7.3 原表止于 P6-8，本行为收口批新增编号） |
| P6-9（SK-1） | 通用 Knowledge Catalog/Document Ingestion（SK 计划 §8 P6-9 原口径，与上行同号异源） | 已完成 | 011 两表（knowledge_source_entry + context_citation_evidence）、domain::knowledge 四实体、ingest per-kind 注册表 + dry-run SAVEPOINT、validate rule 3 knowledge_provides_invalid、search-engine 第三源（INDEX_SCHEMA_VERSION=2）、context-broker PluginSource 实装 + Evidence 受控写面、真实 RTIS 对账复跑 15155 行幂等（实测记录见下方 SK-1 节，收口 docs/releases/RELEASE_2026-11-12_SK1-INGEST.md） |
| P6-10（SK-2） | 三个安全知识来源接入（SK 计划 §8 P6-10） | 已完成 | manifests/plugins/ 三源双份 Manifest（固定 Commit pin，git 通道实装校验）、tests/fixtures/sk2/ 最小快照 fixture、scripts/sk2_tongui_catalog.py 派生目录（74 篇 + 31 精选双 source_ref 合并）、CLI 实跑链 262 条对账幂等（85/102/75）、review APPROVED + rp-search-index 双闸、web 三页来源/许可/Commit/审核展示 + browser-use E2E 截图 4 张（实测记录见下方 SK-2 节与 docs/phase6/SK2-SOURCES-NOTES.md） |
| P6-11（SK-3） | 安全工具目录与静态审查工具（SK 计划 §8 P6-11） | 已完成 | manifests/tools/ 四件双份 Manifest（catalog.extract T0 / extension.audit T0 / document.fetch T1 egress_allowlist / source.sync T1）、012 抓取审计表、catalog_worker.py（复用 parsing_worker 三函数）+ extension_audit_worker.py（零执行 AST 断言）、document_fetch.rs（复用 egress_proxy + 私网全闸 + approved 闸 + 重定向逐跳 + 012 落库）、source_sync.rs + rp-plugin sync（pin 前置闸 + 幂等键 + 限额 + 重试）+ --map-candidates（resource-engine 受控映射）、tests/fixtures/sk3/ 六类拒绝矩阵全过、真实出网冒烟哈希互核（实测记录见下方 SK-3 节与 docs/phase6/SK3-TOOLS-NOTES.md） |

## 已实施批次实测记录

### P6-1（2026-11-11）

- 验收命令与结果：`cargo test --offline -p plugin-registry` exit 0（28 lib + 11 集成）；`cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0；`cargo test --offline --workspace` exit 0（62 个 test result: ok）。
- 依赖增量：serde_yaml_ng 0.10.0 + unsafe-libyaml 0.2.11 + ryu 1.0.23（增量 cargo vendor，.cargo/config.toml 字节级还原，sha256 复核一致）。
- 两示例 Manifest json/yaml 双份经七条校验通过（knowledge.rtis-redteam、tool.rip.python-workers）。
- 对齐修订（PLUGIN-CONTRACT §10.2 两处，yaml 文件头注释声明）：补顶层 resources（§4.1 kind=tool 必填）、embed/evaluate capability 补 input/output schema（§3.1，形状取 manifests/tools/rip.text.embed.yaml 与 rip.kb.evaluate.yaml）。

### P6-2（2026-11-11）

- 验收命令与结果：`cargo test --offline -p plugin-runtime` exit 0（1 + 6）；`cargo test --offline -p quality-engine` exit 0（14，含 PluginGovernor 2 项）；`cargo test --offline --workspace` exit 0（66 个 test result: ok）；`cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0。
- migrations/009_plugin.sql 幂等：测试文件副本两次 migrate 无差异 + 009 SQL 原文双重放无差异（crates/plugin-runtime/tests/lifecycle_full_path.rs migration_009_idempotent_on_file_copy）。
- 五类生命周期操作各 Emit 一条审计事件（audit_event 中 plugin.install/enable/disable/upgrade/uninstall 恰好各 1）且十张核心表行数零变化（five_ops_emit_audit_and_core_tables_untouched）。
- CLI 冒烟：install/enable/disable/uninstall exit 0；`--db migrations/platform.db` 被红线守卫拒绝 exit 1。
- database 四个迁移钉住测试随 009 演进（001..008 到 001..009，各自演进注释内更新，非破坏性）。

### P6-7（2026-11-11）

- 验收命令与结果：`cargo test --offline -p plugin-runtime --test deps_cycle_and_missing` exit 0（8 项：缺失依赖 JSON 清单、version_unsatisfied 带安装版本、requires 指向 DISABLED/UNINSTALLED 双拒绝、依赖环 [cyc.a, cyc.b, cyc.a] 与自依赖、compat rip_core 区间外与 worker_protocol v2 拒绝、健康失败进 ERROR 停止重试、间隔下限 1s、内置 stdio 探测）；`cargo test --offline --workspace` exit 0（72 个 test result: ok）；`cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0。
- enable 激活层接线：静态层（七条 validate_manifest）→ 激活层（deps.resolve_dependencies + compat.check_compatibility，失败带机器可读 details 且状态不变）→ 跃迁；运行层归 health.rs（LOADED/ACTIVE 参与面，间隔下限 MIN_INTERVAL=1s，构造即校验 interval_too_short，失败 mark_error 进 ERROR 并从后续周期剔除即停止重试）。
- 平台版本面注入：PluginRuntime::with_platform(compat::Platform)（enable 求值对象；缺省 0.2.0 / 1 / v1），测试以平台回滚 0.1.0 场景验证 enable 重过兼容。

### P6-6（2026-11-11）

- 验收命令与结果：`cargo test --offline -p plugin-runtime --test tool_plugin_registration` exit 0（5 项：三工具注册可查、podman_host 七种外拒绝、remote_runner 注册拒绝、幂等重注册 skipped、七种词表）；`--test project_endpoint_handshake` exit 0（3 项：真实握手快照进 lock + mcp_client 映射、endpoint 缺失拒绝、URL 形态拒绝）；`cargo test --offline --workspace` exit 0（71 个 test result: ok）；`cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0。
- 真实落位（CLI 实跑，与 P6-5 插件同 rip-data 根）：tool.rip.python-workers install exit 0（三工具 tool.rip.text.parse/embed/knowledge.evaluate 自动注册）；project.rtis-mcp install exit 0（rtis_mcp.mjs 真实握手：10 工具，serverInfo=rt-knowledge 2.0.0，快照 sha256 进 plugin.lock.negotiation）。
- tool-registry 只读对账：13 行（3 个 tool.rip.* T0 + 10 个 rtis.mcp.* mcp_client 映射 T1），tool_version 13 行。
- 计划 §14:813 达成：installed/ 下 knowledge.rtis-redteam + tool.rip.python-workers + project.rtis-mcp 三插件在位（知识 + 工具 + 外部项目三面）。
- 依赖增量：tool-registry 直连边（plugin-runtime Cargo.toml 注释声明，P6-6 任务要求，台账登记）。
- 双 MCP 边界明示：docs/phase6/DUAL-MCP-BOUNDARY.md（命名空间隔离 rtis.mcp.* vs rip.*、代码独立、数据面隔离、鉴权差异、生命周期跟随插件）。

### P6-5（2026-11-11）

- 验收命令与结果：`cargo test --offline -p plugin-runtime --test knowledge_ingest_provenance` exit 0（5 项：溯源三元组与幂等重跑、权限闸 ingest_permission_denied、content_kinds 白名单拒绝、卸载保留已入主库知识、真实契约示例仍过七条）；`cargo test --offline -p database` exit 0（6 套件全过，迁移钉住随 010 演进 001..010）；`cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0。
- 真实落位（CLI 实跑）：install 真实 RTIS 树到 rip-data/plugins/installed/knowledge.rtis-redteam/0.1.0/（exit 0，manifest.yaml/checksums.sha256 460 行/plugin.lock 在位）；ingest 真实语料 exit 0：poc 14717 + skill_md 125 + method_case 313 = 15155 行（6286781 字节），幂等重跑 skipped=15155；溯源对账 plugin_id/plugin_version/corpus_id 全行命中、14717 行带 source_pk。
- 条数口径收敛（V-12 关闭）：README 14717 与实查一致（权威口径），rtis.py 头注 14664 为 2026-11-02 陈旧快照（头注不改，红线 rtis.py 零 diff）。详见 docs/phase6/RTIS-REDTEAM-PLUGIN-NOTES.md。
- license 裁决（V-16 部分关闭）：维持 UNLICENSED（SPDX 保留标识，rule 4 通过），LICENSE 文件选型移交维护者，依据记录在 NOTES §一。
- RTIS 红线核查：knowledge.db sha256 摄取前后一致（f8580feb...）、rtis.py 零 diff、git status integrations/rtis 0 行、RTIS_RESOLVER_MODE 三态机制实查在位；SQLite ro 连接产生的 0 字节 WAL 边车已移除复位。

### P6-4（2026-11-11）

- 验收命令与结果：`cargo test --offline -p plugin-registry --test sources_five` exit 0（6 项：local_dir 全链 + local_dir git 工作树 pinned HEAD 复核 + git file:// 全链与浮动分支双重拒绝 + archive 全链与 sha256 不匹配进 INVALID + mcp 握手快照进 lock + http 契约哈希进 lock）；`cargo test --offline -p plugin-registry -p plugin-runtime` exit 0（42 + 6 + 11 + 3 + 5 + 6）；`cargo test --offline --workspace` exit 0（68 个 test result: ok）；`cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0。
- 依赖增量：tar 0.4.46 + filetime 0.2.29（archive 解包复用维护中实现，增量 cargo vendor，.cargo/config.toml sha256 复核未动）。
- plugin.lock 新增 negotiation 协商快照字段（serde default 兼容旧 lock）：mcp_server 握手（协议版本 + serverInfo + tools 快照 sha256）、http_api 契约哈希、git resolved_commit、local_dir resolved HEAD。
- CLI 冒烟：git 来源（file:// 临时仓）经 --package 省略通道 install exit 0，lock 携带 pinned==resolved_commit，state REGISTERED；浮动分支 manifest install exit 1（rule 5 floating_branch_rejected）。

### P6-3（2026-11-11）

- 验收命令与结果：`cargo test --offline -p plugin-runtime` exit 0（lib 3 + layout_relocatable 5 + lifecycle_full_path 6）；`cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0。
- 布局实测：RIP_DATA_DIR 指向 /tmp 临时根时，plugins 下 registry/installed/disabled/cache/state/logs 六子目录与 installed/knowledge.rtis-redteam/0.1.0/{manifest.yaml, capabilities/, docs/, data/, checksums.sha256, plugin.lock} 全树建立（CLI 冒烟 find 实录）；rip.plugins.dir 单独配置时六子目录落在显式插件根，data_dir 侧无 plugins/（layout_relocatable.rs plugins_dir_override_is_independent）。
- state 文件实测：plugin-state.json 快照随操作翻转（REGISTERED 到 DISABLED），transitions.jsonl 追加 install/register/enable/disable 四行可读回（CLI 冒烟对账 + state_store.rs 单测）。
- gitignore 实测：`git status --porcelain -- rip-data/` 输出为空，`git check-ignore -v rip-data/plugins/state/plugin-state.json` 命中 .gitignore:42。
- 源码字面量复核：`grep -rn "rip-data" crates/ apps/` 全部命中为文档注释与相对缺省名（paths.rs DEFAULT_DATA_DIR_NAME = "rip-data" 相对名，拼部署根或配置根）；apps/ 零命中；无任何绝对路径字面量。crates/context-broker/src/cache.rs:110 的 `.join("rip-data")` 为 P4-5 既有相对默认（join 可配置根），非本批引入，未改动。

### SK-1（2026-11-12，半成品收尾）

- 接手基线：WIP 备份 commit fec5a3f（40 文件 +3831/-159）已含计划 §8 P6-9 第 1 到 7 步代码级交付（ADR、Manifest/Schema、Ingest 通用化、Parser、Search 第三源、Context PluginSource、Evidence 固化），本批不推翻；收尾三项为真实 RTIS 对账复跑、批次台账登记、RELEASE 文档。
- 验收命令与结果：`cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0；`cargo test --offline -p plugin-runtime --test knowledge_ingest_provenance --test sk1_extended_ingest` exit 0（5 + 4 passed）；`cargo test --offline --workspace` exit 101，唯一失败套件 tool-engine podman_host_egress 4 项（egress_env_injection_frozen_via_live_inspect、egress_allowlisted_get_succeeds_through_proxy、non_allowlisted_host_connect_rejected_structured、network_none_dispatch_stays_isolated），panic 原文 `podman ps spawn: Os { code: 2, kind: NotFound }`，本机无 podman（`command -v podman` 零输出），其余 54 套件 `test result: ok` 零失败，符合批次预期口径。
- 真实 RTIS 对账复跑（本批硬红线收口）：`migrations/dev-p71.db` 复制为 /tmp/sk1-recon.db（摄取前 knowledge_ingest 0 行，证实 15,155 证据是 P6-5 重构前实测）→ `RIP_DATA_DIR=/tmp/sk1-recon-data rp-plugin install --manifest rip-data/plugins/installed/knowledge.rtis-redteam/0.1.0/manifest.yaml --package integrations/rtis` exit 0 → ingest 首跑 counts poc 14717 + skill_md 125 + method_case 313 = 15155（skipped=0）→ 二跑 skipped=15155、changed=0 → 只读 `SELECT content_kind,count(*) FROM knowledge_ingest GROUP BY content_kind` = {'poc':14717,'skill_md':125,'method_case':313}，plugin_id 全行 knowledge.rtis-redteam（version 0.1.0、corpus_id rtis-redteam），sum(body_bytes)=6286781 与 P6-5 基线逐字节一致，poc source_pk 非空 14717 行。
- RTIS 红线核查：integrations/rtis/knowledge.db sha256 摄取前后一致（f8580feb...e70f7e3）；`git diff --stat -- integrations/rtis/rtis.py` 空；`git status --porcelain -- integrations/rtis migrations/platform.db` 零输出；integrations/rtis 无 -wal/-shm 边车残留。
- 依赖增量：零新外部依赖（plugin-runtime 仅新增 workspace 内部 dev-dependencies 四条 context-broker/resource-engine/search-engine/tool-registry，仅测试环消费，Cargo.lock 相应四行）。
- 台账修正：P6-8 行「未开工」与 PHASE6-ACCEPTANCE.md 已验收矛盾，按验收报告修正；收口批 P6-9（武器库扩展路径）与 SK 计划 §8 P6-9（SK-1）两个同号异源批次分别登记。
- 浏览器级验证归 SK-2（本批只收 fixture 级与真实语料对账，sk1_extended_ingest.rs 四项已覆盖：三类 dry-run、100% 幂等、禁用即不可见、历史 Evidence 可读）。

### SK-2（2026-11-12，三源接入）

- 接手基线：SK-1 收口后的通用摄取管线（ingest.rs per-kind 注册表、011 knowledge_source_entry、search-engine 第三源双闸）零改动复用；本批为纯消费批次 + web 展示补全，RTIS 三类写面与 knowledge_ingest 表零触碰。
- 验收命令与结果：`cargo test --offline -p plugin-registry --test static_chain` exit 0（13 tests：既有 11 + 三源 Manifest 七条校验 + rule 3/rule 2/rule 5 反例拒绝）；`cargo test --offline -p plugin-registry -p plugin-runtime` exit 0（42+6+13 与 18+8+5+5+6+3+4+4+5 全绿，含新增 sk2_source_ingest 四项：三源条数对账幂等、tongui 合并双 ref、changed 不覆盖历史行 + 不物理删、审核/检索双闸）；`cargo clippy --offline --workspace` exit 0；web `npm run build`（tsc --noEmit）与 `npm run test`（vitest 73）全绿。
- 三源固定 Commit 实查：`git ls-remote` 三仓 HEAD/分支与计划 §3 值逐一相等（440486fd... / 1867eb6b... / 45a12a75...）；git 通道 install 实跑 plugin.lock pinned 全 40 位且 negotiation.resolved_commit 相等（ah/secs 主环境 + tongui 一次性验证环境）。
- CLI 实跑链对账（RIP_DATA_DIR=/tmp/sk2-data，--db /tmp/sk2.db）：ah 85（no_scheme 2）、secs 102（no_scheme 17，首跑 skipped 1 = 上游第 98/99 行同 URL 重复条目去重）、tongui 75（74 篇 + 主页）；dry-run 与实跑逐数一致；二跑 skipped=85/103/75、changed=0；enable + review APPROVED（85/102/75）后 rp-search-index rebuild doc_count=262；disable secskills 重建 160 且 strix 命中 0（复 enable 恢复 262）。
- 只读断言全过（python3 file:ro）：tongui document_markdown=0、策略分组 (CC0-1.0,catalog_only,85)/(Apache-2.0,catalog_only,102)/(UNLICENSED,metadata_only,75)、Git 路径条目 74、精选双 ref 31、source_commit 全行 40 位、review 全量 APPROVED 262、RTIS knowledge.db sha256 前缀 f8580feb 未变、git status integrations/rtis + platform.db 零输出。
- 50 URL 抽样（seed 20261112，分层 17/17/16）：50/50 URL 可达、50/50 分类对账（方法与明细表见 docs/phase6/SK2-SOURCES-NOTES.md §七）。
- 批次偏差（详 docs/phase6/SK2-SOURCES-NOTES.md §三/§六）：tongui catalog_entry 来自派生目录 scripts/sk2_tongui_catalog.py（上游树整仓仅 1 个绝对 URL、站点 slug 与 Git 路径不可互推，上游树直摄实测 0 条目）；upstream_missing 自动标记未实现（无第二快照触发场景，物理删已由测试锁定不发生）；secskills skill_candidate 本快照 0（上游未打包 skills/ 目录）；web 视觉门禁（playwright）本机无 chromium 未运行。
- 浏览器 E2E（browser-use 真实浏览器 1280x800 + vite dev 代理 rp_server 7823，RIP_PLUGIN_DB=/tmp/sk2.db）：插件中心许可/Commit/条目徽章、tongui 详情（来源主页/快照 Commit/内容类型/审核状态/审计 trail 含 plugin.knowledge.ingest）、知识库「知识来源」视图 262 条与 tongui 过滤 75 条，截图 4 张入 docs/phase6/browser-e2e/（sk2-01 到 sk2-04）。

### SK-3（2026-11-12，安全工具目录与静态审查工具）

- 接手基线：SK-2 收口后的三源摄取管线与审核闸零改动复用；本批为 P6-11 四件工具全量批次（catalog.extract / extension.audit / document.fetch / source.sync）+ skill_candidate 到 Resource 候选映射写面，RTIS 三类写面与既有检索/上下文双闸零触碰。
- 验收命令与结果：`cargo test --offline --workspace` 57 套件 ok 379 passed（唯一失败 podman_host_egress 4 项，本机无 podman，批次预期例外口径）；`cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0；`python3 -m unittest discover -s workers/python/tests` 211 tests OK（新增 test_catalog_extract 37 + test_security_extension_audit 46，含六类恶意 fixture 拒绝矩阵与零执行 AST 断言）。
- 新增面：manifests/tools/ 四件双份 Manifest（tool-registry sk3_tool_manifests 静态链 + register/discover + rule 5 复验）、migrations/012_knowledge_document_fetch.sql（抓取审计表）、crates/tool-engine/src/document_fetch.rs + rp-knowledge-fetch bin（复用 egress_proxy + 私网全闸 + approved 闸 + 重定向逐跳）、crates/plugin-runtime/src/source_sync.rs + rp-plugin sync 子命令（pin 前置闸 + 幂等键 + 限额 + 重试 + 部分失败）、resource-engine promote_knowledge_candidates 受控映射写面、tests/fixtures/sk3/ 六类反例 fixture。
- 冒烟实测（RIP_DATA_DIR=/tmp/sk3-data，--db /tmp/sk3-smoke.db）：sync dry-run 零落盘 → 实跑 85 → 幂等键二跑整体跳过 → 无键三跑 skipped=85 changed=0 → 限额失败机器可读（fail-fast）；skill-pack 映射先 DISCOVERED mapped=0、review APPROVED 后 mapped=2（resource 表 CANDIDATE/ACTIVE）；fetch 真实出网 http://example.com/ 哈希落 012 且与 curl 同头独立互核逐字节一致，非 allowlist 域 host_not_allowlisted、127.0.0.1 与 169.254.169.254 private_network_blocked 实拒。
- 冒烟抓出的真 bug 已修：example.com chunked 响应下读取面把分帧记进正文（哈希失真），read_http_response 重写为 Content-Length/chunked 解帧/close 定界三形态（详 docs/phase6/SK3-TOOLS-NOTES.md §四）。
- 已知取舍（详 NOTES §九）：https 传输面未实现（workspace 无 TLS 依赖栈，scheme_unsupported 结构化拒绝，TLS 依赖评估归独立批次）；catalog.extract resource_kind/confidence 为确定性启发（rust ingest 恒 link 的口径差异如实登记）；Web Tool Center 展示不在本批；SESSION-HANDOFF.md 未更新。
- 依赖方向扩展：plugin-runtime 增 resource-engine 直连边（rp-plugin sync --map-candidates 调受控写面，单向依赖），登记于本台账。

## 偏差登记（对冻结文档的显式偏离，均可评审推翻）

1. PLUGIN-CONTRACT §10.2 示例两处对齐修订（P6-1）：补顶层 resources 与 embed/evaluate 输入输出 Schema（§4.1/§3.1 要求），示例文件头注释声明。
2. runtime 词表含 podman_host（P6-1，v1.json description 注明）：PLUGIN-CONTRACT §3.3 列计划七种，Phase 2 ADR 两步走裁决后平台实际词表为七种 + podman_host 过渡型，与 tool-manifest v1.json D-TC-01 同口径。
3. 分层执法（P6-1）：kind 与 capability type 匹配、source 各分支必填键不在 v1.json 表达，由校验器 rule 3/rule 4 精确归因（schema 层 oneOf 报错会掩盖 rule 号定位），schema description 注明。
4. governance 轴词表沿用 resource 表 K-08 四值（P6-2，009 头注注明）：插件 INVALID 属 lifecycle 侧 state 列，治理隔离走 governance 列，两轴正交。
5. quality-engine PluginGovernor 经 plugin-runtime dev-dependencies 接线测试（P6-2）：运行期 plugin-runtime 不依赖 quality-engine（依赖方向约束按批次要求保持），CLI 未直调治理面，Plugin Center 接线归 P6-8。
6. state/ 文件定位为数据库旁路留存（P6-3）：PLUGIN-CONTRACT §11.3 原口径「Phase 6 之前用 state/ 文件承载」，实际 Phase 6 已落库（009），故 plugin-state.json 定位为快照旁路（备份/对账/只读消费），权威在 plugin 表；快照原子写（tmp + rename，worker 缓存先例同款）。
7. license 占位 UNLICENSED 沿用（P6-1 起）：仓库顶层 LICENSE 裁决是 ROADMAP 待验证项 V-16，关闭前 rule 4 仅拒 license=unknown。

## 风险与红线复核

- 红线全程生效：platform*.db 零写入（git status migrations/ 仅新增编号文件）；RTIS 生产行为零改动；宿主机零安装（新依赖仅入仓库 vendor/）；测试一律内存库与临时目录。
- rip-data 首次落地连带项（ROADMAP §7.5.1）已关闭：布局、位置配置、gitignore 三件齐，源码零绝对路径字面量。
- V-16（LICENSE 裁决）仍开放，阻塞 P6-5 前的 license 字段终值。
