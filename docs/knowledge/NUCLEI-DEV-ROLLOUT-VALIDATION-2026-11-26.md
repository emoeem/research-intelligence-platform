# Nuclei 开发环境接入与审核工作台验收（2026-11-26）

## 结论

Nuclei 0.2.6 接入已在独立开发数据库副本中完成真实摄取和搜索索引重建验证。生产数据库、持久化开发数据库、原始 RTIS 数据库均未被本轮 rollout 写入。14,111 条模板全部保持 `DISCOVERED`，没有自动批准。

## 隔离 rollout

- 基线：`rip-data/dev/dev-p6-rtis-025-validation.db` 的一次性副本。
- 临时插件包：从 RTIS 0.2.5 包复制，使用当前 `integrations/rtis/knowledge.db` 和 `curation/k_meta_overlay.json`，加入 Nuclei `nuclei_template` kind 声明。
- 插件版本：0.2.5 → 0.2.6；新版本成为副本中的 active pointer，状态为 `ENABLED`。
- Migration：副本 schema 31 → 32；只扩展 `knowledge_source_entry.entry_kind`。
- 摄取通过 `rp-plugin sync` 完成，幂等键为 `rip-nuclei-0.2.6-20261126`。

### 摄取对账

| 指标 | 结果 |
|---|---:|
| `knowledge_source_entry` Nuclei 模板 | 14,111 |
| `DISCOVERED` | 14,111 |
| `parsed` | 14,109 |
| `invalid_yaml` | 1 |
| `body_unavailable`（超过 2 MiB） | 1 |
| 其他审核状态 | 0 |
| 传统 RTIS POC 兼容写面 | 14,721 |
| 传统 RTIS Skill 写面 | 125 |
| 传统 RTIS Method Case 写面 | 238 |

完整安装目录审计另发现 14,389 个 YAML 文件：14,366 个可解析、21 个缺少 Nuclei 模板所需 `info` 元数据的辅助 profile/mapping 文件、1 个无效 YAML、1 个超限文件。21 个辅助 YAML 不属于 RTIS 选取的 14,111 条模板记录，没有被错误导入为模板。

## 搜索与审核准入

- 临时派生索引成功重建。
- 14,111 条 Nuclei 记录仍是 `DISCOVERED`，因此不会进入正式搜索结果或模型上下文；这是 `APPROVED + 插件可见` 准入规则的预期结果，不是索引遗漏。
- `knowledge_source_entry` API 现在接受 `entry_kind=nuclei_template`，返回模板 ID、Severity、Protocol、CVE/CWE、Tags、作者、请求块、Matcher、Extractor 与 Flow 等结构化元数据，不返回模板正文。
- Knowledge 页面新增 Nuclei 模板显示、同一插件内批量批准/驳回和最近审核历史。
- `POST /api/plugins/<plugin_id>/review` 接受 1–100 个唯一 `item_keys`，经 `rp-plugin review` 写入；audit-engine 记录审核事件。只允许明确的审核请求，不自动批准。
- `GET /api/knowledge_sources` 返回最近审核历史，UI 可追溯审核状态变化。

## 验证

- `apps/server/selftest_knowledge_review.sh`：PASS；覆盖 DISCOVERED 元数据读取、审核写入、审计历史回读、空批次/非法状态拒绝、Bearer 鉴权。
- Knowledge 页面专项测试：8/8 PASS。
- Nuclei 相关 3 项原 ignored 测试已手动运行并通过：模板投影、完整安装语料审计、隔离数据库完整摄取。
- `bench_assemble_latency` 与 `bench_dispatch_latency` 两项原 ignored 基准已手动运行并通过。
- `browser_capture_truncation_cap_fidelity` 仍保持 ignored：Chromium 158 对 1 MiB 页面 CDP `get_title` 有已记录超时，不在本轮强行运行。

## 生产接入仍未执行

- 未对任何持久化或生产 `platform*.db` 执行 Migration 032。
- 未升级生产 RTIS 插件，也未向生产库摄取 14,111 条模板。
- 未重建生产派生搜索索引。
- 未批准任何真实 Nuclei 模板。
- 原始 `integrations/rtis/knowledge.db` 仅只读使用，SHA-256 保持不变。

因此本报告证明的是**隔离开发副本的完整接入链路通过**，不代表生产 rollout 已完成。生产切换仍需单独安排数据库备份、迁移窗口、索引重建和回滚验收。


## 后续持久化开发库切换与插件路径修复（2026-11-26）

上面的隔离 rollout 结论描述的是当时的一次性验证副本。此后，经过独立备份和验收，CISA KEV / Nuclei / PoC Index 数据面已切换到持久化开发库 `migrations/dev-p6.db`；这仍是开发环境，不是生产数据库。

- 当前 KSE 数据面：schema 32，`knowledge_source_entry` 18,102 条；其中 Nuclei 14,111 条。
- Nuclei 审核状态：552 `APPROVED`、13,559 `DISCOVERED`。552 条有明确 `user:cli` 审核事件；没有自动批准。
- Search v6：`migrations/platform-search.db`，3,708 documents / 3,708 FTS rows；Nuclei 552 条 APPROVED 记录进入搜索索引。
- 隔离 rollout 中三个插件的 `installed_dir` 曾指向已清理的 `/tmp` 临时目录。已新增 `rp-plugin repair-path`，并将 CISA KEV 0.1.2、Nuclei 0.1.1、PoC Index 0.1.0 的安装目录恢复到持久化 `rip-data/plugins/installed/`。修复仅更新路径与包指纹，不改 plugin 状态、KSE 行或 review 状态。
- 三个插件的 `ingest --dry-run` 分别检查 1,805 / 14,111 / 1,296 条记录，全部 `changed=0`，证明包路径可读且与现有数据幂等一致。
- 修复前备份：`/home/emo/backups/research-platform/20261126-182822-dev-p6.db.pre-plugin-path-repair.bak`；SHA-256 `6a5c05fcd9ca92b54d0da187ea5e05b4447f12cf602428f84a5047894fea24a7`。

详细实现、审计事件、包指纹和验证命令见 [PLUGIN-INSTALL-PATH-REPAIR-2026-11-26.md](PLUGIN-INSTALL-PATH-REPAIR-2026-11-26.md)。本节不改变“未对生产数据库执行 rollout”的历史结论。
