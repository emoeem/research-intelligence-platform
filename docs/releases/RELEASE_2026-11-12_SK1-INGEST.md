# 更新说明 · 2026-11-12（SK-1 收尾入库：通用 Knowledge Catalog/Document 摄取接线）

> SK-1（SK 计划 §8 P6-9）收尾：接手 WIP 备份 fec5a3f 的七步半成品（代码级已齐，
> 本批不推翻），补齐三项收尾，真实 RTIS 语料 15,155 条对账复跑收口（本批硬红线），
> 批次台账登记与 P6-8 行矛盾修正，workspace 门禁复核全绿。浏览器级验证归 SK-2。

| 项         | 值                                                                                                                                                    |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| 阶段交付   | git `9058c93`（03:48 CST 收尾入库；代码半成品基线 fec5a3f 已在库，本批为文档收尾零代码改动；本行短哈希由回填提交落定）                              |
| 数据库迁移 | 无新增（011_knowledge_sources.sql 随 fec5a3f 在库，010 零改动；对账在 dev 库副本 /tmp/sk1-recon.db 验证，生产 platform*.db 与 RTIS knowledge.db 零写入，sha256 前后一致实测） |
| 自动化测试 | `cargo test --offline -p plugin-runtime --test knowledge_ingest_provenance --test sk1_extended_ingest` exit 0（5 + 4 passed）；`cargo test --offline --workspace` 54 套件 ok 零失败，唯一例外 tool-engine podman_host_egress 4 项（本机无 podman，panic 原文 podman ps spawn NotFound，见 §三） |
| 静态检查   | `cargo clippy --offline --workspace --all-targets -- -D warnings` 零告警（exit 0 当日实测）                                                            |

## 一、接手盘点：七步半成品状态（对照 SK 计划 §8 P6-9 逐项实查）

WIP 备份 commit fec5a3f（40 文件 +3831/-159）经逐项实查，第 1 到 7 步代码级交付全部在位，与计划一致，未推翻：

1. **ADR（第 1 步）**：docs/adr/SK1-INGEST-ADR.md 已裁决（四实体两表方案 + APPROVED 准入闸 + SAVEPOINT dry-run + Evidence 固化），docs/phase6/SK1-INGEST-ADR.md 为指针文件（docs/README.md:30 已登记）。本批补齐：批次台账登记 SK-1 行。
2. **Manifest/Schema 扩展（第 2 步）**：schemas/plugin-manifest/v1.json provides.content_kinds 六值词表 + kinds per-kind 声明；plugin-registry/src/validate.rs rule 3 增补 knowledge_provides_invalid 精确归因；契约示例双份 schemas/plugin-manifest/examples/knowledge.catalog-example.json/.yaml。
3. **Ingest 通用化（第 3 步）**：crates/plugin-runtime/src/ingest.rs:49 CONTENT_KINDS 六值；RTIS 三类固定写 knowledge_ingest（ingest.rs:389 到 444，P6-5 逻辑逐行保留）；扩展三类写 knowledge_source_entry（ingest.rs:445 起）；dry-run 以 SAVEPOINT 包裹写步后 ROLLBACK（ingest.rs:339），计数与实跑同构、零落盘。**待完成项已收口**：真实 RTIS 语料 15,155 条对账复跑（见 §二，本批硬红线）。
4. **Parser 扩展（第 4 步）**：knowledge_parse.rs（frontmatter/标题树/链接抽取/URL 规范化），ingest.rs 三个 per-kind parser（markdown_links/markdown_doc/skill_frontmatter），scheme 白名单 http/https，纯文本解析不执行 Markdown/HTML 中代码不访问链接；workers/python/parsing_worker.py extract_links 扩展 + manifests/tools/rip.doc.parse.json/.yaml 可选输出。
5. **Search 接线（第 5 步）**：crates/search-engine/src/index.rs:29 INDEX_SCHEMA_VERSION 升 2（旧索引强制整库重建）；load_docs 第三源（index.rs:356 到 366）只收 review_status=APPROVED 且插件可见（is_active=1 AND state IN ('ENABLED','LOADED','ACTIVE') AND source_uninstalled=0）的行；索引仍为可整库重建派生物。
6. **Context Broker PluginSource（第 6 步）**：crates/context-broker/src/sources/plugin.rs 空实现替换为读库实装：mount_all（plugin.rs:68）/ candidates（plugin.rs:193）/ load L3 按需正文（plugin.rs:254，plugin.rs:326 闸 review_status 必须为 APPROVED）；assemble.rs Plugin 桶并入知识桶 L2；每次调用实时联查 plugin 表，无缓存失效窗口，禁用即撤出匹配面。
7. **Evidence 固化（第 7 步）**：migrations/011_knowledge_sources.sql:87 context_citation_evidence（excerpt + excerpt_sha256/content_sha256 + Source ID/Plugin ID/Version/Commit/URL/章节锚点/source_location_json）；resource-engine/src/sqlite.rs 受控写面三方法（insert :64 / list :109 / verify :128）；assemble.rs L2 装入与 L3 expand 两处写入；rp-plugin review 子命令（状态翻转 + 审计 KNOWLEDGE_INGEST）。

## 二、真实 RTIS 对账复跑（本批硬红线收口）

接手时实查：现存 dev 库（migrations/dev-p71.db）knowledge_ingest 行数为 0，15,155 条证据是重构前 P6-5 的实测，重构后的摄取管道必须用真实语料重跑收口。复跑命令与结果（全程 RTIS 生产文件零写）：

```bash
sha256sum integrations/rtis/knowledge.db        # 摄取前 f8580febfbd91adfd8eb0c6d1dc8f2b571c2af38579c88811c9fbae33e70f7e3
rm -f /tmp/sk1-recon.db && cp migrations/dev-p71.db /tmp/sk1-recon.db
RIP_DATA_DIR=/tmp/sk1-recon-data cargo run --offline -p plugin-runtime --bin rp-plugin -- \
  --db /tmp/sk1-recon.db install \
  --manifest rip-data/plugins/installed/knowledge.rtis-redteam/0.1.0/manifest.yaml \
  --package integrations/rtis                   # exit 0
cargo run --offline -p plugin-runtime --bin rp-plugin -- --db /tmp/sk1-recon.db ingest knowledge.rtis-redteam    # 首跑
cargo run --offline -p plugin-runtime --bin rp-plugin -- --db /tmp/sk1-recon.db ingest knowledge.rtis-redteam    # 二跑
```

| 对账项               | 首跑                     | 二跑                  | P6-5 基线（对账目标）    |
| -------------------- | ------------------------ | --------------------- | ------------------------ |
| counts               | poc 14717 + skill_md 125 + method_case 313 = 15155 | 空表 | 14717 + 125 + 313 = 15155 |
| skipped              | 0                        | 15155                 | 15155                    |
| changed              | 0                        | 0                     | 0                        |
| sum(body_bytes)      | 6286781                  | 6286781（未变）       | 6286781                  |

只读 SQL 对账（python3 sqlite3 mode=ro）：GROUP BY 与上表一致；plugin_id 全行 knowledge.rtis-redteam、plugin_version 0.1.0、corpus_id rtis-redteam；poc 类 source_pk 非空 14717 行（与 P6-5 溯源口径一致）。RTIS 红线核查：knowledge.db sha256 摄取前后逐字节一致（f8580feb...e70f7e3）、rtis.py git diff 空、`git status --porcelain -- integrations/rtis migrations/platform.db` 零输出、无 WAL 边车残留。

## 三、门禁复核与已知例外

1. clippy 与两套件全绿（见元信息表）。全 workspace 测试唯一例外：tool-engine podman_host_egress 4 项失败（egress_env_injection_frozen_via_live_inspect、egress_allowlisted_get_succeeds_through_proxy、non_allowlisted_host_connect_rejected_structured、network_none_dispatch_stays_isolated），panic 原文 `podman ps spawn: Os { code: 2, kind: NotFound }`（crates/tool-engine/tests/podman_host_egress.rs:317），根因是本机无 podman（`command -v podman` 零输出），非代码回归；该 4 项需有 podman 的环境复测（批次预期口径内，SK-2 或有 podman 的宿主补测）。同套件 url_host_derivation_guards 通过；其余 54 套件 `test result: ok` 零失败。
2. fixture 级验收（三类 dry-run、100% 幂等、禁用即不可见、历史 Evidence 可读）由 sk1_extended_ingest.rs 4 项覆盖，本批实测全过；RTIS fixture 回归由 knowledge_ingest_provenance.rs 5 项覆盖，实测全过。
3. 浏览器级验证（Web 知识工作区显示来源/分类/许可/审核状态）不在本批，归 SK-2。

## 四、台账与文档收口

1. docs/phase6/PHASE6-PLAN.md 批次台账：登记 SK-1 行（P6-9 SK 计划口径）；修正 P6-8 行「未开工」陈旧状态（PHASE6-ACCEPTANCE.md 已验收 P6-1 到 P6-8，以验收报告为准）；补登记收口批 P6-9（武器库扩展路径）行，两个同号异源 P6-9 分别登记（SK-1 行注明）。
2. 本 RELEASE 按 DOC-STANDARD 落 docs/releases/；计划规划员给的文件名含中文（RELEASE_2026-11-12_SK1-通用知识摄取接线.md），与 DOC-STANDARD §二「文件名只允许字母数字下划线连字符点，不用中文文件名」冲突，按规范取 RELEASE_2026-11-12_SK1-INGEST.md（与 docs/adr/SK1-INGEST-ADR.md 同名族）。
3. docs/README.md Phase 6 行补本 RELEASE 链接。

## 五、已知取舍与移交

1. podman_host_egress 4 项待有 podman 的环境复测（见 §三.1）。
2. 浏览器级验证归 SK-2（见 §三.3）。
3. knowledge_ingest 数据搬迁进 knowledge_source_entry（Phase 7 Knowledge 正文模型收敛时一次性迁移）按 ADR §4.3 不做，本批两表互补不相交。
4. 审核工单系统、目录条目正文抓取、向量索引消费均按 ADR §八不做。
5. SESSION-HANDOFF.md 跨会话状态源未在本批更新（不在批次路径清单），SK-2 接手时按其自身批次收口。

## 六、回滚方式

本批仅文档改动（台账 + RELEASE + 索引），零代码与零迁移变更：`git revert <本批提交哈希>` 即可，无需迁移回退；代码半成品基线 fec5a3f 与 011 迁移不受影响。
