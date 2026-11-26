# CISA KEV 与 Nuclei 高风险漏洞知识接入（2026-11-26）

## 目标与边界

本批把外部“已知在野利用”漏洞情报接入 RIP 的知识来源审核面，并将其与官方 Nuclei 检测模板做 CVE 关联。只做元数据摄取、YAML 静态解析、索引与审核；不执行任何模板，不向真实目标发送请求，不把源仓库的 `verified` 字段解释成 RIP 复现结论。

自有旧 POC 逐条补证不是本批的阻塞条件。旧 POC 可保留现状，后续仅对有充分证据且有维护价值的条目投入复核。

## 来源

| 来源 | 用途 | 固定版本 / 许可 | 约束 |
|---|---|---|---|
| [CISA KEV](https://github.com/cisagov/kev-data) | 已知在野利用漏洞主目录 | commit `51e0a777743fa06957afbaeb280f8735d292c1be`；CC0-1.0 | 官方 JSON；不含 CVSS/EPSS，不表示特定目标当前受影响 |
| [ProjectDiscovery Nuclei Templates](https://github.com/projectdiscovery/nuclei-templates) | 检测模板与 CVE/CWE 关联 | `a346d8f00c61bf95bbd30365f12cf923be51ba3d`；MIT；插件 `knowledge.nuclei-templates@0.1.1` | 静态解析 YAML；只批准与 KEV 精确匹配且通过审查的模板；不执行 |
| [Awesome-Hacking](https://github.com/Hack-with-Github/Awesome-Hacking) | 发现其他公开漏洞研究与工具来源 | 当前已接入的固定快照 `440486fd08b0a0d9ad9a041a0bb234dcdf65dee2` | 目录链接不是漏洞证据；不重复导入其巨型下游仓库 |
| [PoC Index](https://github.com/0xMarcio/pocindex) | 补充 CVSS/EPSS、advisory 与 Nuclei CVE 元数据 | commit `e9076fea96095e014fc107c02dd23ee8dc21476d`；MIT metadata-only snapshot | 只下载四份 JSON 并按 CISA KEV 过滤；不导入 PoC 正文，不把聚合标签当权威事实 |

## CISA KEV 接入

- 新增插件：`knowledge.cisa-kev@0.1.2`。固定 CISA 上游快照通过 `scripts/security-intel/build_cisa_kev_package.py` 从干净的固定 commit checkout 生成到 `target/cisa-kev-package/`；插件运行时用 `local_dir` 读取，因此插件 trust 为 `local`。每条 KSE 记录仍明确保存 GitHub 上游 URL、Git source_type 与完整 commit，不依赖每次同步时重新克隆远端。
- 新增 `catalog_entry` 解析器 `cisa_kev_json`，无新增数据库迁移。
- 解析器限制：包内相对路径、16 MiB 文件上限、CVE ID 格式与去重、CISA 声明 `count` 与实际数组长度一致、必需字段与日期格式校验、CWE 格式校验、勒索软件与取证分诊枚举校验。
- 条目稳定键为 CVE ID；记录 CISA 版本、发布日期、厂商、产品、漏洞名、描述、收录日期、修复期限、requiredAction、ransomware 标记、forensicTriage、CWE 与来源 commit。
- 明确将 CVSS/EPSS 留空，`poc_status=not_assessed`、`current_target_status=not_tested`。KEV 收录仅证明该 CVE 被 CISA 纳入已知在野利用目录。
- 许可策略 `metadata_only`；每条初始 `DISCOVERED`，经过字段完整性、来源 commit、CC0 和唯一性核对后，审核状态才变为 `APPROVED`。`APPROVED` 表示来源元数据审核通过，不等于 POC 已复现。

## 隔离验证结果

CISA 数据固定在 `2026.11.25`，发布日期 `2026-11-25T19:48:12.4344Z`。

- 摄取：1,805 条；唯一 CVE ID：1,805；重复：0；过滤/部分失败：0。
- 取证分诊（`forensicTriage`）：Yes 122，No 1,683。
- 勒索软件使用：Known 362，Unknown 1,443。
- 所有条目均保留同一 CISA source commit 与 CC0-1.0。
- 1,805 条通过审核写面分批审核，审计事件 19 条；全部仅是 CISA 元数据审核。
- 临时 Search v6 索引构建：1,805 文档、1,805 FTS 行；查询 `CVE-2026-109618` 成功命中 CISA KEV 记录。
- 初始导入与审核先在 disposable DB/package/index 完成；随后又在 `migrations/dev-p6.db` 的 disposable clone 上演练三插件同步、审核与索引重建。原始 `migrations/dev-p6.db`、core `migrations/platform.db`、RTIS 原始 `knowledge.db` 与生产插件目录均未修改。插件 API 的 KSE 读取路径是 `RIP_PLUGIN_DB`（默认 `migrations/dev-p6.db`），因此生产切换目标应为该插件数据面，而不是 core `platform.db`。

## Nuclei 与 KEV 交叉导入与审核

- 新增插件：`knowledge.nuclei-templates@0.1.1`；本地生成包以 `local` 信任级别读取，模板内容仍按官方 Nuclei commit 与 MIT 许可追溯。上游固定 commit 为 `a346d8f00c61bf95bbd30365f12cf923be51ba3d`；本机模板包版本 `nuclei-templates 1:v10.5.2.r748.ga346d8f-1`，MIT。
- `scripts/security-intel/build_nuclei_metadata_package.py` 只读 RTIS 原始数据库，生成 `target/nuclei-metadata-package/knowledge.db`；只复制解析所需的 12 个元数据列，不复制 `content`、`verified`、`verified_note`、engagement、target、used_on 或 agent 字段。模板 YAML 从与固定版本匹配的 `/usr/share/nuclei-templates` 读取并静态解析，不执行模板。
- 隔离导入：14,111 条模板；`parsed` 14,109、`invalid_yaml` 1、`body_unavailable` 1；模板路径缺失 0。
- 与 CISA KEV 的交集：505 个唯一 CVE、552 条模板。逐条核对 YAML 分类 CVE、RTIS 元数据 CVE、severity、模板路径、`parse_status`、MIT 许可和固定 commit：552/552 通过，0 个不一致。
- 审核状态：552 条匹配模板 `APPROVED`，其余 13,559 条保持 `DISCOVERED`。已批准模板的 severity：Critical 406、High 119、Medium 27。此处 `APPROVED` 只表示静态来源/分类审核通过，不表示模板执行成功或目标存在漏洞。
- Nuclei 审核通过 6 条批次审计事件；未执行任何模板，也未向真实目标发送请求。
- CISA 与 Nuclei 插件均以 `local` trust 从固定版本本地快照同步；CISA 条目保留官方 Git 上游 provenance，Nuclei 条目保留 ProjectDiscovery Git 上游 provenance。两者均不需要在同步时克隆上游仓库。
- 联合构建 Search v6：在包含 RTIS curation Overlay 的完整 cutover clone 上生成 2,487 文档、2,487 FTS 行。查询 `CVE-2014-6271` 同时返回 ShellShock Nuclei 模板与 CISA KEV 的 `CVE-2014-6271` / `CVE-2014-7169` 记录，证明跨来源关联检索可用。

这些数据是**已知在野利用情报 + 风险评分元数据 + 检测模板覆盖**，不是 505 个目标已确认受影响。CISA KEV 原始字段中的 CVSS/EPSS 仍为空；独立的 PoC Index enrichment 保留 FIRST EPSS 与 NVD/CVE Program CVSS assessment。Nuclei 模板自带的 severity/CVSS/EPSS 仅作为模板元数据，不替代独立评分，也不代表 PoC 复现结果。

## PoC Index 风险元数据补全

- 新增插件：`knowledge.pocindex-kev@0.1.0`，上游仓库 commit `e9076fea96095e014fc107c02dd23ee8dc21476d`。运行时使用本地 HTTP snapshot 包，KSE provenance 记录 `source_type=http_snapshot`、`source_url=https://pocindex.io`、完整仓库 commit；插件 trust 为 `local`。
- 下载并固定四份 PoC Index JSON：`kev.json`、`epss.json`、`cve_metadata.json`、`nuclei.json`。每个文件保存 CDN URL、ETag、Last-Modified、SHA-256 和字节数；过滤范围以 CISA 官方 KEV 目录为准。
- 导入 1,296 条 CVE enrichment：1,292 条出现在 PoC Index KEV map、1,295 条有 FIRST EPSS score/percentile、1,295 条有 CVSS assessment、583 条有 PoC Index Nuclei 元数据，合计 9,541 条 advisory URL。PoC Index KEV map membership 只表示其索引收录，不代表存在可执行 PoC。
- CVSS assessment 保留版本、score、severity、vector、source、assessment_type；不把 CVSS v2/v3/v3.1/v4 混成单一分数。EPSS score/percentile 保留 FIRST 来源；CISA KEV 自身的 CVSS/EPSS 字段仍保持空值，不冒充 CISA 原始字段。
- PoC Index `CVE_list.json` 的 PoC 链接全集未纳入本批；本批仅保存风险元数据、advisory URL 与 Nuclei 元数据，不复制 PoC 正文。缺少 PoC Index enrichment 不代表没有 PoC。
- 1,296 条记录在隔离 cutover clone 中完成 schema、分数范围、URL、来源快照校验并显式审核；审核仅代表 metadata-only 来源/结构通过，不代表 PoC 可运行或目标存在漏洞。

## 三源联合索引验证

- CISA KEV：1,805 条；Nuclei：14,111 条（552 条经 CISA CVE classification 精确交集审核）；PoC Index enrichment：1,296 条；保留既有插件 KSE 890 条。
- 明确数据面后，完整 cutover clone 改为从应用实际使用的插件数据面 `migrations/dev-p6.db` 克隆（schema 31→32）；`migrations/platform.db` 是独立 core DB，不承载这批 KSE。三插件增量加入后保留原有 890 条 KSE 与原审核状态，总 KSE 18,102 条。
- 在包含现有 Awesome-Hacking / SecSkills / Tongui 记录、CISA KEV、Nuclei、PoC Index 和 RTIS curation Overlay 的完整 clone 上，Search v6 重建为 3,708 文档 / 3,708 FTS 行。原有 Awesome-Hacking 24 条 APPROVED、301 条 DISCOVERED、12 条 REJECTED 未被覆盖；SecSkills 与 Tongui 的原审核状态也保持不变。
- `CVE-2014-6271` 搜索同时命中 CISA KEV、Nuclei 模板和 PoC Index enrichment；`CVE-2025-55182` 同时命中三源；`CVE-2021-44228` 返回多条相关 Nuclei 模板及 CISA/PoC Index 记录。
- 生成风险优先级报告：`docs/knowledge/KEV-POCINDEX-RISK-PRIORITY-2026-11-26.md`，按 EPSS、CVSS Primary assessment、CISA 收录日期排序；另保留近期 KEV × Nuclei shortlist。

## 后续数据补强

1. 将 PoC Index 聚合评分与 NVD/FIRST 官方 API 做独立交叉校验，保留来源、采集时间、评分版本和缺失状态；不得用 CVSS 分数替代 KEV 在野利用状态。
2. 继续评估 PoC Index `CVE_list.json` 与 Awesome-Hacking 下游 Trickest/CVE 的轻量 PoC-link 增量入口；不全量 clone 巨型仓库，不复制未经许可的 PoC 正文，也不自动执行公开 PoC。
3. Awesome-Hacking 当前固定快照 `440486fd08b0a0d9ad9a041a0bb234dcdf65dee2` 已接入；后续只在上游有内容变化时增量同步。
4. 生产部署前做数据库备份、插件版本冲突检查、索引重建与回滚演练；完整 cutover clone 已通过插件数据面 schema 31→32、三插件同步/审核、索引重建与跨来源查询，本批尚未修改生产数据库。


## 最终 cutover readiness 复核（2026-11-26）

### 正确数据面与差异边界

- 应用知识来源读面默认使用 `migrations/dev-p6.db`（`RIP_PLUGIN_DB`），不是 `migrations/platform.db`。源插件库 SHA-256：`42a22d0fe896fe8e0a155b395b9ebe9aa09790ceef4e40fa4b3bc729f212c067`；schema 31。
- 完整 cutover clone：`/tmp/rip-cisa-kev.qfBoNo/dev-p6-cutover.db`，SHA-256 `ded528333d347881f505e76879ce3f7fe0ee519ae0ad873374f88cd7c7ea64f3`；schema 32；SQLite `integrity_check=ok`，`foreign_key_check=0`。
- 源库与 clone 的表集合完全相同，没有删除表或新增表。仅五个表有预期变化：`audit_event` 38→85（+47）、`knowledge_source_entry` 890→18,102（+17,212）、`plugin` 12→15（+3）、`plugin_capability` 13→16（+3）、`schema_version` 31→32（+1）。其余表行数完全一致。
- 原始 `migrations/dev-p6.db`、core `migrations/platform.db`（SHA-256 `af26bb97f5586c12835457b7c4ddd71e3c4c97112ce58d255802b826eb656124`）和 RTIS `integrations/rtis/knowledge.db`（SHA-256 `7b6083f8e1e84c9aff60a3d4f429f29cfc436ef1aafa7221d4d3cd945df641a7`）均保持不变。

### API 与检索真实读面

- 使用真实 `apps/server/rp_server.py`，将 `RIP_PLUGIN_DB` 指向 cutover clone，启动隔离本地服务后通过 Bearer 认证调用 `GET /api/knowledge_sources`。CISA `cve_record`、Nuclei `nuclei_template`、PoC Index `pocindex_record` 三种结构化对象均按预期返回；未返回模板或 PoC 正文。
- `CVE-2021-44228` 的 PoC Index API 记录返回 4 个 CVSS assessments 和 53 个 advisory；`epss_score` / `epss_percentile` 保留在结构化对象中。CISA 与 Nuclei 的 CVE 元数据也通过 API smoke。
- Search v6 clone：3,708 documents / 3,708 FTS rows，索引库 SHA-256 `2da952cad715e179ee2626b79b55784ab014897e270d18c65c05d1587dd43d54`。
- 对 clone 上真实执行 `rp-search-index query`：`CVE-2014-6271` 命中 CISA 2、Nuclei 1、PoC Index 1；`CVE-2025-55182` 命中三源各 1；`CVE-2021-44228` 命中 CISA 2、PoC Index 1、Nuclei 44。以上是情报/检测元数据命中，不代表目标已验证受影响。

### 本地验收

- `cargo fmt --all -- --check`、`cargo test --offline --workspace`、`cargo clippy --offline --workspace --all-targets -- -D warnings`：通过（workspace 有 4 个既有 ignored 测试，不计为失败）。
- `cargo test --offline -p rp-mcp-parity`：通过；多 key 与 legacy 交替配对 40 组，增量 p95 为 +1.68 ms，小于 5 ms。
- `apps/server/selftest_knowledge_review.sh`：通过；包含结构化元数据、显式审核、审计历史、非法批次/status 与 Bearer 鉴权。
- Nuclei / CISA KEV / PoC Index 三个 metadata package 测试全部通过。

### 切换状态

本次已完成隔离 clone、API、搜索、数据库完整性与本地测试验收，但**尚未替换 `migrations/dev-p6.db` 或生产搜索索引**。原 GitHub Actions run `45778221802` 的 parity job 曾因旧的绝对 HTTP p95 5 ms 断言失败；新配对增量测试已在本地 workspace 与 parity job 命令中通过。修复 commit `6921a5b4` 已在本地创建，但 GitHub SSH/HTTPS 凭据当前不可用，尚未推送，因此还没有针对该 commit 的远程 CI 结果。为避免在远程 CI 未确认前切换，生产数据面保持原状；推送并确认 CI 全绿后再执行带备份与回滚点的切换。
