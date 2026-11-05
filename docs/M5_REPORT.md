# M5 实施报告：RTIS & Agent Integration（ERI 统一 Resolve 网关）

日期：2026-11-06 03:39-03:50 (UTC+8)　执行：RIP M5 实施工程师　范围：integrations/rtis/ 与生产 rtis.py 单点改动

## 1. 备份

- 备份文件：`integrations/rtis/backups/rtis.py.pre-m5-20261106-033940.bak`
- 改动前 md5：`ba294f6b8c4849461c45c1e6d1818a82`（备份与生产逐字节一致，md5 相同）
- 改动后 md5：`2fa9979b5a429e218e8c78996aa7c883`
- 另有 P2 期备份在 `research-platform/db/backups/rtis.py.pre-p2-*.bak`（未触碰）

## 2. 生产 diff 统计

- 文件：`~/.dsh/redteam/rtis/rtis.py`，1633 行 -> 1648 行
- 净增 15 行，删除 0 行，1 个函数内 3 个相邻插入块（diff 行 654-675，全部位于 `_eri_external_review()`）
- 其余 1630+ 行零改动；knowledge.db / catalog.db / assets.db / eri 生产代码零触碰；无 git 操作

改动内容（一处）：`RTIS_RESOLVER_MODE` 环境变量（`rp|legacy|off`，非法值回落 legacy）：

| 模式 | 行为 |
|---|---|
| `legacy`（默认） | 与改动前完全一致：直调 eri_p2.py bridge-gate |
| `rp` | resolver 候选前置 `eri_adapter.py`（可用性检查后替换 resolver 路径），适配器内部走 rp-resolve -> eri_p2 -> error 链 |
| `off` | 直接返回 None（跳过外部资源复查） |

## 3. 新增组件（integrations/rtis/）

| 文件 | 说明 |
|---|---|
| `eri_adapter.py` | 统一 Resolve 网关，stdlib-only。CLI 契约与 eri_p2 相同（`bridge-gate --task-file`），auto 模式：a) rp-resolve CLI（target/debug 优先，回落 release，env `RTIS_RP_RESOLVE`/`RTIS_RP_DB`） b) 失败回落 eri_p2.py bridge-gate c) 再失败返回 status=error。单路 5s 超时 + 单次重试（env `RTIS_ERI_ADAPTER_TIMEOUT`），输出 external_resource_review 9 字段与 legacy 逐字段兼容，附 `adapter` 观测信封与 JSONL 审计（`adapter_audit.jsonl`，env 可关） |
| `shadow_compare.py` | 影子对照：同一 task 上下文分别走 legacy（eri_p2 直调）与 rp（adapter --mode rp），字段级 diff + 归因分类 |
| `tests/test_eri_adapter.py` | 10 个 fallback 单测（mock 子进程，离线） |
| `tests/test_rtis_regression.py` | 自测基线回归 + gate 三模式真实调用形状校验 |
| `shadow_report.json` | 影子对照结果留存（M8 材料） |

## 4. 自测前后对比

命令：`python3 -I rtis.py test`（全离线官方回归）

| | 改动前（基线 03:39） | 改动后（03:41） |
|---|---|---|
| total | 17 | 17 |
| passed | 13 | 13 |
| failed | 4 | 4 |
| 失败集 | T2 / T4 / T11 / T13 | 完全一致 |

4 个失败均为改动前既有问题（T2/T4 为知识库检索桥既有降级，T11 角色稿覆盖，T13 依赖 T2/T4），与本阶段改动无关，模式逐例比对完全一致。

## 5. gate 真实调用验证

创建一次性任务（已删除任务文件），分别以默认模式与 rp 模式各执行一次 `rtis.py gate`：

- 默认（legacy）：`gate.external_resource_review = {status: ok, query_id: erq-*, candidate_count: 1, detailed_research_required: true}`，CJK 桥把"落地页"桥到 landing/redirect 命中 1 条
- `RTIS_RESOLVER_MODE=rp`：经 adapter 走 rp-resolve，`{status: no_match, candidate_count: 0}`（platform 库无 CJK 桥，预期差异，见 §6）
- 完整 task 记录中 `external_resource_review` 恰为 9 个规范字段；`gate_rec` 内摘要恰为 4 键（status/query_id/candidate_count/detailed_research_required），形状与改动前一致
- `RTIS_RESOLVER_MODE=off` 与 `RTIS_ERI=off`：review 缺席（None），符合语义

## 6. 影子对照（3 组真实查询）

数据源：legacy 查 ERI 生产库（81 资源，含 topics/license/skill_mappings 轴）；rp 查 `migrations/platform-dryrun.db`（81 资源，M1 ETL 同源）。query_id 两侧各自生成，设计上排除。

| 查询 | legacy | rp | 差异 | 异常 |
|---|---|---|---|---|
| 落地页（白牌站群） | ok, 1 条（hacker101） | no_match, 0 条 | 6 项，全部预期 | 0 |
| api security | ok, 5 条 | ok, 5 条 | 2 项，全部预期 | 0 |
| android | ok, 5 条 | ok, 5 条 | 2 项，全部预期 | 0 |

合计：预期差异 10，异常 0，verdict=all_diffs_explained。差异全部可归因到已记录的平台简化：

1. **CJK 桥缺失**：落地页组 rp 侧 no_match，中文画像特征无确定性查表桥（落地页->landing/redirect）
2. **topics/language 轴缺失**：platform 库无该列，词元 blob 退化为 name+description+url，命中集部分位移（api 组 legacy_only: h4cker, cve；rp_only: seclists, awesome-web-hacking）
3. **权重简化**：两轴 0.6/0.4 对比 legacy 五轴（keyword/category/tech/profile/relation/quality）
4. **license 轴缺失**：platform 恒 UNKNOWN -> adoptable_as_dependency 恒 false -> rp 侧 recommended 恒空（legacy api 组可推荐 5 条）
5. **skill_mappings / resource_categories 表缺失**：already_covered_by_internal、capability_gaps、category_matches 恒空 -> detailed_research_required 恒 false

注：rp 侧 android 组正确命中 android-security-awesome（legacy top5 未含），说明 url/description blob 在该组有补偿效果。

## 7. 测试结论

- adapter 单测：15/15 通过（10 adapter + 5 回归；含三路 fallback、单次重试、双路全败 error、mode 隔离、形状兼容、关键词推导与 legacy 一致性、缺失二进制回落、超时配置）
- 自测回归：通过（失败集与基线逐例一致）
- gate 真实调用：通过（三模式形状校验）
- 运行方式：`python3 -I -m unittest discover -s integrations/rtis/tests -t integrations/rtis/tests`

## 8. 回滚步骤

1. `cp /home/emo/code/pensting/research-platform/integrations/rtis/backups/rtis.py.pre-m5-20261106-033940.bak ~/.dsh/redteam/rtis/rtis.py`
2. 核对 md5 = `ba294f6b8c4849461c45c1e6d1818a82`
3. 环境变量默认即 legacy：若曾设 `RTIS_RESOLVER_MODE=rp`，清除该变量（或设 `RTIS_ERI=off` 双保险）
4. 复跑 `python3 -I ~/.dsh/redteam/rtis/rtis.py test` 确认 17 例 13 过 4 败（T2/T4/T11/T13）
5. adapter/shadow/tests 文件在 integrations/rtis/ 内，独立于生产，可保留或整目录移除，不影响回滚后行为

## 9. 局限与未竟事项

- rp 路径当前查 platform-dryrun.db（M1 dryrun 库，81 资源）；正式切流前需指向生产化 platform 库（env `RTIS_RP_DB` 可配，无需改代码）
- 影子对照仅 3 组查询、单轮；M8 切流四检查需要更长窗口与对账脚本（本阶段产出 shadow_report.json 为第一份材料）
- legacy 路径 bridge-gate 会按生产契约向 ERI 库写审计行（resolver_queries/resource_usage）；影子对照组 task_id 前缀 `shadow-m5-` 可辨识
- adapter 审计 JSONL 追加写无轮转，量大后需外部处理
- CJK 桥/topics/license 三轴缺失是 M2 建库时的已记录差异，属后续 ETL/迁移项，不在本阶段修复
- rtis.py 自测套件本身会遗留测试任务文件（既有行为，11-03 起如此）；本会话产生的 2 个 M5 孤儿任务文件已清理，套件自建文件按惯例保留
