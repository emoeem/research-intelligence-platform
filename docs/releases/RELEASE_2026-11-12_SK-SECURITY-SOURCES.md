# 更新说明 · 2026-11-12（SK 安全知识源接入收口：SK-1 到 SK-3 三批总账）

> SK 安全知识源接入三批接续完成：SK-1 通用 Knowledge Catalog/Document 摄取接线（011 迁移 + 检索、上下文、证据三面接线），SK-2 三个公开来源固定 Commit 快照插件化（262 条目录元数据，审核双闸进检索面），SK-3 目录抽取与静态审查四件工具（012 抓取审计表）。本批为收尾入库：V-9 podman 缺席门禁修复（三批共同例外项清零）、独立验收复测截图四张、三道门禁全量复跑全绿、RTIS 旧链 15,155 条对账在 SK-3 后代码态复跑逐数一致。生产 platform*.db 与 RTIS 生产文件全程零写（knowledge.db sha256 摄取前后逐字节一致实测）。

| 项         | 值                                                                                                                                                                                                                      |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 阶段交付   | SK-1 `9058c93`（03:49 CST，代码半成品基线 `fec5a3f` 03:23 已在库）+ SK-2 `f77e7b5`（04:37 CST）+ SK-3 `ff9e93e`（05:39 CST）+ 本批收尾入库 `2bf8e9c`（06:14 CST，V-9 门禁修复 + 复测截图 + 本 RELEASE 与登记） |
| 数据库迁移 | 011_knowledge_sources.sql（SK-1，knowledge_source_entry + context_citation_evidence）与 012_knowledge_document_fetch.sql（SK-3，抓取审计表 ok/rejected 双态只追加）均在开发库副本与测试内存库执行；生产 platform*.db 与 integrations/rtis/knowledge.db 零写入（本批对账复跑前后 sha256 一致实测） |
| 自动化测试 | 本批全量复跑（2026-11-12 仓库根 CWD）：`cargo test --offline --workspace` exit 0，430 passed / 0 failed / 0 ignored（80 条 test result 行 = 67 测试二进制 + 13 Doc-tests 段，58 套件有用例；podman_host_egress 5/5，V-9 修复后原 4 项 panic 转绿）；`python3 -m unittest discover -s workers/python/tests` 211 tests OK（skipped=11）。SK-3 批次记录 57 套件 379 passed / 4 failed（4 失败即 podman 缺席 panic），与本批总数差值属当时统计口径未对齐，本批以逐行求和实测为准 |
| 静态检查   | `cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0 零告警（本批复跑实测）                                                                                                                          |

## 一、SK-1：通用 Knowledge Catalog/Document 摄取接线（9058c93）

1. 接手 WIP 备份 `fec5a3f` 七步半成品（代码级已齐，未推翻）并收口：schemas/plugin-manifest/v1.json content_kinds 六值词表与 per-kind 声明、plugin-registry validate rule 3 精确归因、plugin-runtime ingest.rs 六类 CONTENT_KINDS（RTIS 三类固定写 knowledge_ingest，扩展三类写 knowledge_source_entry，dry-run 以 SAVEPOINT 包裹零落盘）、knowledge_parse.rs 解析面、search-engine 第三源（INDEX_SCHEMA_VERSION 升 2，只收 APPROVED 且插件可见行）、context-broker PluginSource 空实现换实装（L2 桶并入 + L3 按需正文，review_status 闸）、011 迁移 context_citation_evidence（excerpt/content sha256 + Source/Plugin/Version/Commit/URL 溯源）与 rp-plugin review 子命令。
2. 硬红线收口：真实 RTIS 语料 15,155 条对账复跑（口径见 §四；本批在 SK-3 后代码态复跑，逐数一致）。
3. 台账修正：docs/phase6/PHASE6-PLAN.md 批次台账登记 SK-1 行（P6-9 SK 计划口径），修正 P6-8 行陈旧状态，两个同号异源 P6-9 分别登记。
4. 批次详情：docs/releases/RELEASE_2026-11-12_SK1-INGEST.md（含 SK-1 批次当日对账原始记录与门禁例外明细）。

## 二、SK-2：三源知识来源插件化（f77e7b5）

1. 三插件按固定 Commit 快照接入（manifests/plugins/ 双份 yaml/json，按 schemas/plugin-manifest/v1.json 全文校验；git 通道 install 实跑临时 clone + commit 存在性校验，pinned 全 40 位）：

| 插件                          | 许可证    | 快照 Commit | 摄取条数 | per-kind 策略                     |
| ----------------------------- | --------- | ----------- | -------- | --------------------------------- |
| knowledge.awesome-hacking     | CC0-1.0   | `440486f`   | 85       | catalog_only                      |
| knowledge.secskills           | Apache-2.0| `1867eb6`   | 102      | catalog_only + metadata_only      |
| knowledge.tongui-course-notes | UNLICENSED| `45a12a7`   | 75       | metadata_only（正文本体零落库零入仓） |

2. 对账与幂等：总量 262 条目录元数据；二跑 skipped=85/103/75、changed=0；tongui 精选站点 31 篇与 Git 仓库 74 篇同文章合并（精选条目双 source_ref，Git 路径为正文权威来源），未精选 43 篇 canonical_url 用 blob URL。
3. 审核与检索双闸：全量条目初始 DISCOVERED，review 翻转 APPROVED 且插件可见才入检索面；三源 262 条进 search-engine 第三源（doc_count 262），禁用任一插件重建索引即撤出（实测 262 → 160）。
4. Web 面：插件中心三卡片（许可证徽章、快照 Commit、来源条数与审核计数）与知识库「知识来源」视图（/api/knowledge_sources 只读端点，缺表守卫降级）；批次 E2E 截图 sk2-01 到 sk2-04。
5. 批次详情：docs/releases/RELEASE_2026-11-12_SK2-SOURCES.md 与 docs/phase6/SK2-SOURCES-NOTES.md。

## 三、SK-3：目录抽取与静态审查四件工具（ff9e93e）

1. P6-11 四件全量（manifests/tools/ 双份登记，rp-tool validate/register 全过）：rip.knowledge.catalog.extract（T0 纯解析目录抽取，直接复用 parsing_worker 既有抽取面）、rip.security.extension.audit（T0 纯静态零执行，AST 断言锁定 worker import 面与危险调用点）、rip.knowledge.document.fetch（T1：approved 闸 + 域名 allowlist + 私网全闸 + 重定向逐跳复核 + 大小/MIME/超时限额 + 内容哈希落 012 审计表）、rip.knowledge.source.sync（T1：pin 前置闸 + 幂等键 + 限额 + 部分失败机器可读 + --map-candidates 候选映射受控写面）。
2. 六类恶意 fixture 拒绝矩阵逐项断言（私网/浮动分支/无许可证/安装钩子/越权 Shell/危险 MCP Schema）；审计动作词表 +2（plugin.knowledge.sync、plugin.knowledge.map_candidates）。
3. 冒烟抓出并修复真 bug：chunked 响应分帧记进正文（哈希失真 8dade617，589 字节）后 read_http_response 重写为 Content-Length 精确取/chunked 解帧/close 定界三形态，落库哈希 25ddf2c8 与 curl 独立互核逐字节一致。
4. worker unittest 增量 211 = 128 既有 + catalog 37 + audit 46。
5. 批次详情：docs/releases/RELEASE_2026-11-12_SK3-TOOLS.md 与 docs/phase6/SK3-TOOLS-NOTES.md。

## 四、RTIS 旧链对账（本批复跑实测，SK-1 红线复核）

SK-1 批次红线在本批（SK-2/SK-3 改动之后）用同口径复跑。命令（仓库根 CWD，RTIS 生产文件只读）：

```bash
sha256sum integrations/rtis/knowledge.db   # 摄取前 f8580febfbd91adfd8eb0c6d1dc8f2b571c2af38579c88811c9fbae33e70f7e3
rm -f /tmp/sk-recon.db && cp migrations/dev-p71.db /tmp/sk-recon.db
RIP_DATA_DIR=/tmp/sk-recon-data cargo run --offline -p plugin-runtime --bin rp-plugin -- \
  --db /tmp/sk-recon.db install --manifest rip-data/plugins/.manual-mf/knowledge.yaml --package integrations/rtis
RIP_DATA_DIR=/tmp/sk-recon-data cargo run --offline -p plugin-runtime --bin rp-plugin -- --db /tmp/sk-recon.db ingest knowledge.rtis-redteam   # 首跑
RIP_DATA_DIR=/tmp/sk-recon-data cargo run --offline -p plugin-runtime --bin rp-plugin -- --db /tmp/sk-recon.db ingest knowledge.rtis-redteam   # 二跑
```

| 对账项      | 首跑                                        | 二跑                              | SK-1 批次记录（对账目标） |
| ----------- | ------------------------------------------- | --------------------------------- | ------------------------- |
| counts      | poc 14717 + skill_md 125 + method_case 313 = 15155 | 空表（新增 0）             | 14717 + 125 + 313 = 15155 |
| skipped     | 0                                           | 15155（by_kind 14717/125/313）    | 15155                     |
| changed     | 0                                           | 0                                 | 0                         |
| total_bytes | 6286781                                     | 0（未新增）                       | 6286781                   |

只读 SQL 对账（sqlite3 mode=ro）：GROUP BY content_kind = poc 14717 行 2,098,032 B / skill_md 125 行 1,029,879 B / method_case 313 行 3,158,870 B，合计 6,286,781 B 与报告一致；溯源全行 knowledge.rtis-redteam / 0.1.0 / rtis-redteam；poc 类 source_pk 非空 14,717 行。RTIS 红线：knowledge.db sha256 摄取前后逐字节一致（f8580feb...e70f7e3）。结论：与 SK-1 批次记录逐数一致，SK-2/SK-3 两批改动未回归 RTIS 旧链。

（口径说明：SK-1 批次命令引用安装后 Manifest 路径 rip-data/plugins/installed/knowledge.rtis-redteam/0.1.0/manifest.yaml；本批 rip-data 运行时根已轮换无 installed 目录，改用同版本 0.1.0 手工 Manifest rip-data/plugins/.manual-mf/knowledge.yaml，capabilities content_kinds 与 corpus_id 一致，摄取语义相同。）

## 五、本批收尾入库内容

1. V-9 门禁修复（crates/tool-engine/tests/podman_host_egress.rs:287 与 :311 两处守卫）：podman 不可用时 rip_containers() 返回空集、force_cleanup_rip_containers() 空操作，SKIP 判定留给用例入口。根因：SK-1/SK-2/SK-3 三批共同记录的 4 项失败（panic 原文 podman ps spawn NotFound）发生在 podman 可用性守卫之前的清理调用，无 podman 宿主必触发。修复后本机（probe exit=1 无 podman）实测该文件 5 passed / 0 failed，workspace 全量转绿（见元信息表）。
2. 独立验收复测截图四张入库（docs/phase6/browser-e2e/skverify-01 到 skverify-04 + README 登记段）：插件中心三卡片（ENABLED + 许可证徽章 CC0-1.0/Apache-2.0/UNLICENSED + 快照 Commit 440486f/1867eb6/45a12a7 + 85/102/75 全带 APPROVED 计数）、知识来源视图检索 strix 命中 1 条（Apache-2.0/仅目录元数据/1867eb6 徽章 + 原始链接 github.com/usestrix/strix）、按 tongui 过滤 75 条 UNLICENSED 逐条带 blob 链接、tongui 详情页（许可证 UNLICENSED、快照 Commit 全 40 位、lock sha256、catalog_entry · metadata_only、APPROVED: 75、审计 trail 含 plugin.knowledge.ingest）。复测环境：browser-use 真实浏览器 1280x800，vite dev 5181（/api 代理）到 rp_server 127.0.0.1:7831（RP_PLUGIN_DB=/tmp/sk2.db 开发库，本地无鉴权模式），数据面只读零写库；跳转实证与登记见 docs/phase6/browser-e2e/README.md 新增段。入库员抽验 skverify-01 截图本体：三卡片徽章、Commit 短哈希与计数与登记描述逐项一致。
3. 文档收口：本 RELEASE 总账 + docs/README.md 登记 + docs/SESSION-HANDOFF.md 第三节补 SK 扩展完成行（SK-1 到 SK-3 各批按自身口径未更新交接文档，本批统一补登记）。

## 六、已知取舍

1. rip.knowledge.document.fetch 传输面仅 http 明文（workspace 无 TLS 依赖栈，Cargo.lock 无 rustls/reqwest/openssl），https 按 scheme_unsupported 结构化拒绝不静默降级；TLS 依赖的 vendor/离线构建评估归独立批次（SK-3 口径不变，tongui 等 https-only 源定点抓取待解锁）。
2. tongui 正文本体零落库零入仓（无 LICENSE 红线），只登记章节/标题/URL/路径元数据与哈希，按需读由浏览器跳转原始来源承担。
3. SK-3 批次 cargo 总数记录（57 套件 379 passed / 4 failed）与本批复跑（80 行 430 passed / 0 failed）总数未逐项追溯对齐（当时统计口径未留原始日志）；两记录中 4 项失败与 podman 缺席的对应关系经本批 V-9 修复实测闭环，本批数字以逐行求和实测为准。
4. web 五门禁（tsc/build/vitest/视觉门/prettier）本批未复跑：web/ 零改动，沿用 SK-2 批次记录（vitest 73 全绿、tsc 零错误）。
5. 复测截图为验收员在 /tmp/sk2.db 开发库副本上的实测产物，入库员未重跑浏览器面（本批证据为截图本体与 README 登记段，入库员抽验截图 01 与登记一致）；browser-use 复测沿用页面脚本触发视图切换的路径（IAB 高层 click 对该 SPA 连续超时的既有替代，见 SK2-SOURCES-NOTES.md §八）。
6. knowledge_ingest 数据搬迁进 knowledge_source_entry、审核工单系统、目录条目正文抓取、向量索引消费等按 SK1-INGEST-ADR 不做项维持原裁决（归 Phase 7 收敛）。

## 七、回滚方式

1. 三批代码：`git revert ff9e93e`（SK-3）、`git revert f77e7b5`（SK-2）；SK-1 提交 9058c93 本身为文档收尾零代码改动（revert 其 docs 提交即可）。011/012 迁移均为纯追加新表，回滚后曾执行过的开发副本残留表无害，执行器按 user_version 跳过；生产 platform*.db 与 integrations/rtis 全程未执行迁移。
2. 本批：`git revert <本批提交短哈希>` 即可，零迁移、零生产库触碰。
3. RTIS：integrations/rtis 零写（knowledge.db sha256 前后一致实测），无需回滚动作。
