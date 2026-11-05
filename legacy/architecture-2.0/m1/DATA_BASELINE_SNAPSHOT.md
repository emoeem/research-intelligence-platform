# Data Baseline Snapshot（M1 统一口径数据基线快照）

| 项 | 值 |
|---|---|
| 快照时间 | 2026-11-06 00:35-00:45 CST（本会话实测复放时刻） |
| 性质 | 满足 [计划 §2.4:172]「输出一份签名或带时间戳的数据基线，不以本段摘要作为最终迁移数量」；本文档为基线，后续数量变更须以新快照覆盖 |
| 来源 | `world.run sqlite3 直查` 侦察快照（`[侦察:确定性事实]`、`[侦察:28库盘点]`）+ 本会话 [实测] 逐条复放（复现率 100%，零偏差） |
| 边界 | 仅计数与分布，不含内容样本、不含脱敏数据；所有命令以只读方式访问源库 |

## 1. 快照锚定（库文件指纹）

[实测] 命令：`sha256sum` 与 `ls -la`，输出：

| 库 | 绝对路径 | SHA-256 | 大小（字节） | mtime |
|---|---|---|---|---|
| ERI catalog | `/home/emo/code/pensting/security-research-upgrade/eri/db/catalog.db` | `cd495a81a59cb1b23df7d080a864fa094a344cd4efb79409110e4d90a9f135e8` | 569,344 | 2026-11-05 23:28 |
| RTIS knowledge | `/home/emo/.dsh/redteam/knowledge.db` | `c3f42f58b05aafec8bec76965d82a437e967e77863f42ee5512771f30f1984ee` | 28,299,264 | 2026-11-05 04:25 |

复放对账：若库哈希与本表不一致，说明基线后发生了写入，应以重放命令重新生成快照，不得沿用本文数字。

## 2. 统一基线表

### 2.1 ERI catalog.db（资源/队列/质量/许可）

| metric | value | 来源命令（sqlite3，均 [实测] 复现） | 口径解释 |
|---|---|---|---|
| resources INDEXED | 81 | `select status,count(*) from resources group by status;` | 全表仅此一状态；与 [计划 §2.1:89]「Resource Catalog：81 条资源」一致 |
| d5 候选总量 | 68 | `select decision,count(*) from d5_candidates group by decision;` | = 63 + 5，见 §3 口径解释 |
| d5 NEEDS_REVIEW（待人工） | 63 | 同上 | 待人工判定的疑似重复对；与「63 条 D5 待人工」摘要一致 |
| d5 RELATED_RESOURCE | 5 | 同上 | 判定「相关资源、保留双方」的对；即早期摘要「63+5」中的 +5 |
| review_log 总行数 | 165 | `select count(*) from review_log;` | 三队列规则审核留痕总数（reviewer 默认 `eri-p2-rules`）；165 > resources 81，为 K-07「Review 实为 1:N」证据 |
| review_log: d5 队列 | 68 | `select queue,count(*) from review_log group by queue;` | 决策分布：NEEDS_MORE_EVIDENCE 63 + RELATED_KEEP_BOTH 5（`select decision,count(*) from review_log where queue='d5' group by decision;`）；与 d5_candidates 68 对账一致 |
| review_log: license 队列 | 79 | 同上 | 规则审核行数；与 github_metadata license_status 三分类总数 79 对账一致 |
| review_log: mapping 队列 | 18 | 同上 | 规则审核行数 |
| skill_mappings APPROVED_REFERENCE | 16 | `select mapping_status,count(*) from skill_mappings group by mapping_status;` | 注意：该列名为 mapping_status，取值域为 APPROVED_REFERENCE/COMPLEMENTARY/NEEDS_MORE_EVIDENCE（D5 风格复用），非 schema 注释中的 SUGGESTED/REVIEWED/ADOPTED/REJECTED；口径差异记录在案 |
| skill_mappings COMPLEMENTARY | 1 | 同上 | |
| skill_mappings NEEDS_MORE_EVIDENCE | 1 | 同上 | mapping 待人工 = 1 |
| quality A / B / C / D | 4 / 28 / 44 / 5（合计 81） | `select grade,count(*) from quality_scores group by grade;` | 覆盖全部 81 资源；A+B=32 为可采纳候选口径 |
| license IDENTIFIED | 56 | `select license_status,count(*) from github_metadata group by license_status;` | 许可证字段已识别 |
| license NO_LICENSE_FILE | 16 | 同上 | 无许可文件 |
| license FILE_UNIDENTIFIED | 7 | 同上 | 有文件未能识别；三分类合计 79 与 review_log license 队列一致 |

### 2.2 knowledge.db（知识库）

| metric | value | 来源命令（sqlite3，均 [实测] 复现） | 口径解释 |
|---|---|---|---|
| poc 总量 | 14,717 | `select count(*) from poc;` | 与 [计划 §2.3:132]「约 14,717 条记录」一致 |
| poc verified=1 | 130 | `select verified,count(*) from poc group by verified;` | 130/14717 = 0.883% ≈ 0.88%，与 [计划 §2.3:134]「验证率约 0.88%」一致；「13」笔误共两处（SYSTEM_INVENTORY.md:61、KNOWLEDGE_BASE_REVIEW.md:36），均经 [实测] grep 核实并裁定（K-01） |
| poc verified=0 | 14,587 | 同上 | 14,587/14,717 = 99.12% 未验证，与 SYSTEM_INVENTORY 同句百分比自洽 |
| k_meta | 28 | `select count(*) from k_meta;` | |
| k_negative（反知识） | 17 | `select count(*) from k_negative;` | 与 [计划 §2.3:135]「17 条反知识记录」一致 |
| k_event | 633 | `select count(*) from k_event;` | |

### 2.3 Engagements 与 28 库

| metric | value | 来源 | 口径解释 |
|---|---|---|---|
| engagement 目录数 | 28 | [实测] `ls ~/.dsh/redteam/engagements/ \| wc -l` = 28 | 与 [计划 §2.3:156]「28 个 engagement」一致 |
| assets.db 实存 | 27 | [侦察:28库盘点]（world.run 直查；未本机逐库复放） | `王红娘-520-wanghongniang-com` 无 assets.db（目录仅 engagement.yaml/SCOPE.md 与空 reports/runs，sizeBytes=0，userVersion 记 -1 表示不适用） |
| schema 指纹分布 | `55c9ad012317b709acd77b0ba2f00f34` x 26；`3ad6325b559209d720d26f456d527bc9` x 1 | [侦察:28库盘点] | 26 库同构（28 表，compatible）；异构库为 `openredir-landing`（37 表，差异 9 项：evidence_index/plan_revision/postmortem/profile_delta/research_plan/site_feature/site_profile/state_transition/workflow_run，判 unsupported） |
| user_version | 可读 27 库全为 0 | [侦察:28库盘点] | 无任何库写 schema 版本，佐证 R-01；schema_version 机制进 Schema V1（M1-B） |
| 最大库 | 联想-lenovo-src：25,483 行 / 4,382,720 字节 / asset_rows 2,335 | [侦察:28库盘点] | ETL Dry-run 抽取量级参考 |
| 库行数区间 | 28 至 25,483（口径：totalRows = 全表合计行数，含元数据表） | [侦察:28库盘点] | 最小值 28（彩票-博彩站点群）并非空库：28 张表均有行，但业务表 asset_rows=0、http_evidence=0，即无业务数据；最大值为联想-lenovo-src 25,483 |
| asset_rows 总量参考 | 最大 2,335（联想）；多数 0-99 | [侦察:28库盘点] | 28 库合计 asset_rows ≈ 3,308（侦察快照逐行求和，[建议] M1-C 正式盘点时复核） |

兼容级别预盘点结论（M1-C 将正式化）：26 compatible / 0 conditional / 1 unsupported / 0 unknown / 1 不适用（无库）。

## 3. D5 口径解释（计划 §2.4 要求项）

1. **候选总量**：68 对（d5_candidates 全表），其中 NEEDS_REVIEW 63 + RELATED_RESOURCE 5。
2. **待人工量**：63（NEEDS_REVIEW）。后续战略说明「63 条 D5 待人工」即此口径。
3. **已审核量**：review_log 中 d5 队列 68 行，全为规则审核（reviewer 默认 `eri-p2-rules`），决策分布 NEEDS_MORE_EVIDENCE 63 + RELATED_KEEP_BOTH 5。即候选全部已过规则层，人工层尚未处理。
4. **已拒绝量**：0（无 REJECTED/DISTINCT 决策记录；d5_candidates 仅两值、review_log d5 仅两值 [实测]）。
5. **异常量**：0（两表行数对账一致：68=68）。
6. **「+5」的含义与统计口径**（[计划 §2.4:171] 要求项 3）：+5 指 d5_candidates 中 decision=RELATED_RESOURCE 的 5 对，规则层判定为「相关但非重复、双方保留」（review_log 对应 RELATED_KEEP_BOTH），不属于待人工重复合并队列。早期摘要「63+5」以候选总量表述，两者相加恰为 68，与 review_log 一致，R-06 关闭候选。

## 4. 与计划书摘要的差异清单

| 计划书/M0 摘要 | 本次实测 | 判定 |
|---|---|---|
| [计划 §2.3:132]「约 14,717 条记录」 | 14,717 | 一致 |
| [计划 §2.3:134]「验证率约 0.88%」 | 130/14717 = 0.883% | 一致；「13」笔误共两处（SYSTEM_INVENTORY.md:61、KNOWLEDGE_BASE_REVIEW.md:36），K-01 已裁定 |
| [计划 §2.3:135]「17 条反知识」 | k_negative = 17 | 一致 |
| [计划 §2.3:156]「28 个 engagement」 | 目录数 28 | 一致；assets.db 实存 27（1 无库） |
| 早期摘要「D5 63+5」vs 后续「63 条待人工」（[计划 §2.4:167]） | 68 = 63 + 5，两口径均正确，见 §3 | 差异已解释（R-06 关闭候选） |
| [计划 §2.4:167]「License 待人工 8 条」 | **未能导出该口径**：license 队列 79 条均已规则审核；license_status 56/16/7 中无自然产生 8 的切分 | **待人工确认**（K-12）；可能来自其他时点快照或另一系统；不阻塞 M1 |
| [计划 §2.3:142]「hunt-* 约 42 个」 | huntCount = 42 | 一致 [侦察:Skills/Tools复核] |
| [计划 §2.3:144]「tags 仅 6 个 Skills」 | tagsCount = 6 | 一致 [侦察:Skills/Tools复核] |
| [计划 §2.3:143]「6 个 Skills 缺 role 字段」 | 6 文件缺 role/whenToUse/enabled 三字段 | 一致且更精确 [侦察:Skills/Tools复核] |
| [计划 §2.3:149-150] fscan 探测问题、gogo installed 失真 | 实测细化：fscan version=报错首行；gogo 二进制存在可执行（27,079,465 字节）仅不在 PATH；frpc 0.71.1→0.72.0 漂移；suo5 version=usage 头（实测 --version v2.2.0）；rtis.py version 硬编码「1」；pd-httpx 误判（实际在 ~/.local/bin/httpx） | 一致并细化（R-08 更新）[侦察:Skills/Tools复核] |
| [计划 §2.3:157]「28 个库缺少统一 Schema Version」 | user_version 全 0 [侦察:28库盘点] | 一致 |

## 5. 可重放命令（全文）

以下命令在本会话按序执行并产出 §1/§2 全部数字；任何持有库只读访问权的环境可复放验证。

```bash
# 0) 锚定指纹（快照有效性前提）
sha256sum /home/emo/code/pensting/security-research-upgrade/eri/db/catalog.db \
          /home/emo/.dsh/redteam/knowledge.db
ls -la /home/emo/code/pensting/security-research-upgrade/eri/db/catalog.db \
       /home/emo/.dsh/redteam/knowledge.db

# 1) engagements 目录数
ls /home/emo/.dsh/redteam/engagements/ | wc -l    # 预期 28

# 2) ERI catalog.db
DB=/home/emo/code/pensting/security-research-upgrade/eri/db/catalog.db
sqlite3 "$DB" "select status,count(*) from resources group by status;"                      # INDEXED|81
sqlite3 "$DB" "select decision,count(*) from d5_candidates group by decision order by 2 desc;"  # NEEDS_REVIEW|63  RELATED_RESOURCE|5
sqlite3 "$DB" "select count(*) from review_log;"                                            # 165
sqlite3 "$DB" "select queue,count(*) from review_log group by queue;"                       # d5|68 license|79 mapping|18
sqlite3 "$DB" "select decision,count(*) from review_log where queue='d5' group by decision;"# NEEDS_MORE_EVIDENCE|63  RELATED_KEEP_BOTH|5
sqlite3 "$DB" "select mapping_status,count(*) from skill_mappings group by mapping_status;" # 16|1|1
sqlite3 "$DB" "select grade,count(*) from quality_scores group by grade order by grade;"    # A|4 B|28 C|44 D|5
sqlite3 "$DB" "select license_status,count(*) from github_metadata group by license_status order by 2 desc;" # 56|16|7

# 3) knowledge.db
KB=/home/emo/.dsh/redteam/knowledge.db
sqlite3 "$KB" "select count(*) from poc;"          # 14717
sqlite3 "$KB" "select verified,count(*) from poc group by verified;"  # 0|14587  1|130
sqlite3 "$KB" "select count(*) from k_meta;"       # 28
sqlite3 "$KB" "select count(*) from k_negative;"   # 17
sqlite3 "$KB" "select count(*) from k_event;"      # 633

# 4) 28 库逐库指纹（world.run 侦察命令，本会话未逐库复放；重放时按此模式执行）
# 对每个 ~/.dsh/redteam/engagements/<engagement>/assets.db：
#   sqlite3 <assets.db> "pragma user_version;"                       # 预期 0（无库记 -1）
#   sqlite3 <assets.db> "select count(*) from sqlite_master where type='table';"
#   表集 md5 指纹、sizeBytes、逐表行数与 asset_rows/http_evidence_rows 统计
# 完整预期输出见本文档 §2.3（来源：world.run 快照 [侦察:28库盘点]）
```

## 6. 未尽事项

- License「8 条待人工」口径（K-12）：待人工确认后补入本文档修订版或对账报告。
- 28 库逐库行数/SHA-256：属 M1-C 正式盘点交付（[计划 §6.2:409-419]），本快照仅收录侦察级指纹。
- skill_mappings 的 mapping_status 取值域与表注释（SUGGESTED/REVIEWED/ADOPTED/REJECTED）不符，实际取值为 D5 风格三值：已记录为口径注释，M1-C 字段映射时复核是否为历史遗留。
