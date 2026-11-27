# RIP 六域资产库存、质量与来源覆盖审计

- 日期：2026-11-27
- 范围：Platform DB、活动插件版本的 KSE、RTIS `knowledge.db`、派生搜索索引
- 方法：SQLite `mode=ro` + `PRAGMA query_only=ON`；无数据库写入
- 自动化报告：`asset-domain-inventory-2026-11-27.json`
- 可复现命令：

```bash
.venv-check/bin/python scripts/security-intel/audit_asset_domains.py \
  --output docs/knowledge-governance/asset-domain-inventory-2026-11-27.json
```

## 1. 结论

RIP 当前属于**多源库存较大、审核与 AI 可用性尚未闭环**。不能用单一“知识条目数”代表资产规模，也不能把漏洞目录、Nuclei 模板、Payload 和可复现 POC 混为一个数字。

| 域 | 库存口径 | 当前质量判断 |
|---|---:|---|
| 知识 | 46 条平台知识卡；活动 KSE 17,471 条源记录；81 个资源目录项 | 卡片中 37 VERIFIED、7 CANDIDATE、2 DISCOVERED；源记录仅 3,659 APPROVED，13,797 DISCOVERED，15 REJECTED |
| 漏洞 | CISA KEV 1,805；PoC-in-GitHub/PoCIndex-KEV 1,296；RTIS 中 4,413 条带 CVE 的记录、4,359 个不同 CVE 值 | 两个上游目录各自内部 key/hash 唯一；跨源重叠不能直接相加；RTIS 有 10 个重复 CVE 分组和 1 个格式异常值 |
| POC/其他武器 | RTIS 非 Nuclei 记录 610，其中 `kind=poc` 41 | 610 条中 134 仅有历史 `verified=1` 标志；仅 28 条有结构化 k_meta/verification_basis，不能据此宣称 134 条已复现 |
| Nuclei 模板 | 活动模板 14,111 | 552 APPROVED、13,559 DISCOVERED；元数据解析 14,109 parsed、1 invalid_yaml、1 body_unavailable；尚未做逐模板引擎验证或目标验证 |
| 工具 | 20 | 16 installed/ok、3 installed/degraded、1 missing/unknown；20 条均有来源系统、来源主键和 provenance_ref |
| 技能 | 125 | 119 enabled、125 有 body_ref；required_tools 和 knowledge_refs 均为 0，技能尚未形成正式知识/工具依赖图 |

活动插件源记录合计 17,471；插件历史版本总行数 18,102。历史版本不能与活动版本重复累计。搜索索引 3,789 条：3,107 catalog_entry、552 Nuclei、79 GitHub repo、49 curated RTIS POC、2 GitHub page。索引规模不等于完整源库存。

所有四个 SQLite 数据库的 `integrity_check` 均为 `ok`。

## 2. 知识域

### 平台知识卡

| Lifecycle | 数量 |
|---|---:|
| VERIFIED | 37 |
| CANDIDATE | 7 |
| DISCOVERED | 2 |
| 合计 | 46 |

其中 23 条 own + 14 条 negative 为 VERIFIED。这里的 VERIFIED 是平台治理生命周期字段；不能自动推导为每个结论均有当前可复现的外部证据。

### 活动知识源（按活动 plugin_id + version）

| 来源 | 记录数 | APPROVED | DISCOVERED | REJECTED | 内容体 | 来源 URL | Commit pin | License 状态 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Awesome-Hacking 0.1.3 | 84 | 6 | 75 | 3 | 84 | 84 | 84 | 84 |
| CISA KEV 0.1.2 | 1,805 | 1,805 | 0 | 0 | 1,805 | 1,805 | 1,805 | 1,805 |
| Nuclei Templates 0.1.1 | 14,111 | 552 | 13,559 | 0 | 14,111 | 14,111 | 14,111 | 14,111 |
| PoCIndex KEV 0.1.0 | 1,296 | 1,296 | 0 | 0 | 1,296 | 1,296 | 1,296 | 1,296 |
| SecSkills 0.1.3 | 101 | 0 | 89 | 12 | 101 | 101 | 101 | 101 |
| Tongui Course Notes 0.1.2 | 74 | 0 | 74 | 0 | 74 | 74 | 74 | 74 |
| **合计** | **17,471** | **3,659** | **13,797** | **15** | **17,471** | **17,471** | **17,471** | **17,471** |

来源 URL / commit / license 状态字段覆盖完整，不代表上游内容仍在线、许可证法律结论已人工确认或每条记录已审核。Nuclei 的 `canonical_url` 字段当前为 0；其 `source_url` 是仓库级来源，具体文件身份依赖 `item_key`/`source_path` 和 pinned commit。

资源目录 81 项：A 4、B 28、C 44、D 5；81/81 有 canonical URL，81/81 有 resource_source 来源链接、source URL 和 commit。C/D 级资源不应无条件进入 Agent 默认上下文。

## 3. 漏洞域

- CISA KEV 活动源：1,805 条，1,805 个唯一 item_key 和内容 hash。
- PoCIndex KEV 活动源：1,296 条，1,296 个唯一 item_key 和内容 hash。
- RTIS 14,721 条记录中 4,413 条有 CVE 值；按规范化字符串计算 4,359 个不同值；存在 10 个重复 CVE 分组及 1 个格式异常值。
- RTIS 记录按 source：Nuclei 14,111、web 451、self 139、manual 12、kb 7、source 缺失 1。

**计数限制：** CISA KEV、PoCIndex 和 RTIS 的 CVE 记录可能跨源重叠，不能将 1,805 + 1,296 + 4,359 作为独立漏洞总数。当前尚未完成跨源 CVE 主键归一、别名映射、修复/撤销状态与受影响版本的完整对账。

## 4. POC 与其他武器域

排除 Nuclei 后 RTIS 有 610 条记录：payload 444、script 74、poc 41、rule 17、template 14、exp 10、case 4、kind 缺失 4、method 2。610 个 code 值唯一。

| 字段 | 覆盖 | 缺失 |
|---|---:|---:|
| Source URL | 461 | 149 |
| Description | 604 | 6 |
| Content | 605 | 5 |
| Affected versions | 39 | 571 |
| 结构化 k_meta | 28 | 582 |
| verification_basis | 28 | 582 |

134 条带有旧 `verified=1` 标志，其中 `kind=poc` 41 条里的 33 条带此标志。该字段是历史状态，不是当前隔离实验室复现证明；应迁移为明确的 `static_reviewed`、`lab_tested`、`target_observed` 等证据状态，并附测试环境、日期、版本、结果和证据引用。其余 610 条里多数是 payload/script/rule，不应全部标为 POC。

## 5. Nuclei 模板域

- 活动库存：14,111；`item_key` 和 `content_sha256` 各自均唯一。
- Review：552 APPROVED，13,559 DISCOVERED，0 REJECTED。
- Source URL、pinned commit、license_status、body_bytes 均覆盖 14,111/14,111。
- 解析状态：14,109 parsed、1 invalid_yaml、1 body_unavailable。
- 模板元数据：template_id 14,109；author 14,109；description 12,541；matcher_count 14,109；references 14,109；classification 8,288。
- 严重性：info 5,388、high 3,046、medium 2,870、critical 2,020、low 519、missing 209、unknown 59。
- 协议覆盖（一个模板可有多个协议）：HTTP 11,995、Code 960、File 447、TCP 294、Headless 222、Workflows 207、JavaScript 194、SSL 40、DNS 34。
- RTIS 投影中 4,412 条 Nuclei 记录带 CVE；0 条有 affected versions；7,834 条有 description。

`parsed` 仅代表导入阶段元数据解析成功，不等同于 Nuclei Engine `-validate`，更不等同于模板成功检测到漏洞。尚未对活动模板做逐模板 `nuclei -validate`、matcher/DSL 静态审查、请求副作用分类或隔离靶场回归。不得因模板带 `verified` 元数据就视为已复现。

## 6. 工具域

20 个注册项均有 `source_system`、`source_pk`、`provenance_ref`，且各有一个 tool_version 记录。健康情况：16 ok、3 degraded、1 unknown/missing。权限层级：T0 17、T1 3。工具注册和 binary_ref 存在不代表每次执行都安全；仍需逐工具核验版本、许可证、维护状态、二进制 hash、调用参数、权限边界和隔离策略。

## 7. 技能域

125 项均来自 `rtis-skills`，125/125 有 body_ref 和 source_pk；119 enabled。当前 `required_tools`=0、`knowledge_refs`=0。已生成 146 个 Skill→Tool 候选关系，覆盖 67 个技能；静态规则排除了 6 个负向提及，120 个候选在当前状态下可进入人工审核。**批准映射数仍为 0**。这些候选不得被当成已经生效的技能能力或权限。

下一步映射应从已审阅的关系证据构建 Skill→Tool、Skill→Knowledge、Skill→Vulnerability、Skill→Nuclei、Skill→POC 关系；只有通过人工审核的关系才回填 `required_tools`/`knowledge_refs`。

## 8. AI 可用性与检索差距

搜索投影 3,789 条，与源库存 17,471 条不同。当前投影有 552 Nuclei 和 49 curated RTIS POC；不是完整漏洞/POC 搜索目录。审计的基础 platform.db 中 Agent registry 仍为 0 agent_definition、0 agent_run；另一个开发运行库 rip-data/dev/agent-runtime-search-2026-11-27.db 已启用 agent.research.minimal 与统一检索工具。workflow_definition 仍为 0，另有 13 条 legacy workflow。统一搜索和 LLM Planner 已接入，但不能据此宣称 AI 已能可靠使用全部资产。

主要阻断项：
1. 13,797 条活动源记录仍处于 DISCOVERED。
2. Nuclei 模板有 13,559 条待审核，缺少逐模板引擎验证。
3. 610 条非 Nuclei 武器只有 28 条具备结构化验证依据；版本字段缺失 571 条。
4. 技能与知识/工具的正式映射仍为零。
5. 搜索索引覆盖明显低于源库存，且 Agent/Workflow 定义注册表未填充。

## 9. 建议的审核顺序

1. **P0 数据身份与去重**：以 CVE、模板路径+commit、POC 内容 hash、工具 ID、Skill ID 建立稳定身份；完成 CISA/PoCIndex/RTIS 跨源交叉表，保留来源关系，不合并原始来源。
2. **P1 高价值漏洞与 POC**：先审核 Critical/High、KEV、真实 POC 类记录；补 affected versions、source URL、许可证、修复建议、negative cases 和隔离实验室复现证据。
3. **P1 Nuclei 模板**：处理 2 条解析异常；对 14,109 条 parsed 模板做分批 `nuclei -validate`；先审核高危、KEV/CVE、关键协议模板，再做 matcher/请求副作用与授权靶场回归。
4. **P1 技能映射**：审阅 146 个候选关系，批准后才投影为技能可用工具；随后通过显式实体引用映射知识/漏洞/模板/POC。
5. **P2 搜索覆盖与质量评测**：扩展分域索引，建立人工 qrel，按域统计 Recall@K/MRR/NDCG、重复率、来源覆盖率和证据充分率。
6. **P2 工具治理**：修复 3 个 degraded 和 1 个 missing 项；校验来源、版本、hash、license、权限与沙箱；不要仅凭安装状态开放调用。

## 10. 安全边界与未完成项

本轮仅执行只读 SQL 和元数据统计，没有执行 POC、Nuclei 模板或对任何目标发起请求，没有更改数据库审核状态，也没有自动批准任何映射。Nuclei 引擎逐模板验证、POC 实验室复现、跨源 CVE 主键对账和人工质量审核仍是后续任务，报告不将其伪装成已完成。


## 11. 本轮新增治理工件（2026-11-27）

- `poc-governance-candidates-2026-11-27.json`：610 条非 Nuclei 武器的只读风险/字段审计；原始正文和验证备注不导出。
- `poc-review-batch-001-2026-11-27.json`：41 条真正 `kind=poc` 记录的人工审核任务。
- `vulnerability-nuclei-governance-2026-11-27.json`：5,657 个规范化 CVE 身份的精确跨源关系快照（CISA KEV、PoCIndex、Nuclei）；610 条非 Nuclei RTIS 武器没有有效 CVE 关联。
- `nuclei-review-batch-001-2026-11-27.json`：首批 200 个 Nuclei 审核任务，后续批次通过 `build_nuclei_review_batch.py --offset N --limit 200` 生成。
- `poc-sensitive-review-batch-001-2026-11-27.json`：35 条疑似凭据/秘密材料的脱敏复核任务。
- 所有候选均未自动批准；没有执行 POC、模板或对任何目标发送请求。Nuclei Engine `-validate` 在隔离配置下 20 秒超时，未取得引擎验证结果。


## 12. Agent 统一检索验收与边界

- `rip.kb.unified_search@0.2.0` 已在开发 Tool Registry 启用，默认 Agent `agent.research.minimal` 只声明该统一入口；输入 `query` / `include_candidates` 贯穿 Agent → Tool Engine → Python Worker → `rp-search-unified`。
- 一次性 PostgreSQL 迁移后，Agent 对 CVE-2021-44228 的默认 ACTIVE 检索返回 5 条结果；对 `sourcemap` 显式开启候选通道返回 5 条结果（3 Nuclei + 2 RTIS），全部保留 CANDIDATE/DISCOVERED 状态。
- E2E 使用 KeywordSearchPlanner 和 50,000 token 上下文预算，不是实际 LLM Provider 调用。12,000 token 上下文超限、L3 相关性偏差、真实 Provider 以及 PostgreSQL 非空知识卡片通道仍待验收。
- 当前运行中的 API 进程仍是旧版本，默认派生索引仍为 schema 7；schema 7 可只读兼容已批准搜索。服务重启后由 schema 8 自动重建候选索引。
- `rip_m4_acceptance` 与 `rip_user_test_20261127` 两个持久 PostgreSQL 数据库本轮只读检查，没有写入候选关系；173 条 Skill 关联已在 disposable PostgreSQL 验证，待确认持久目标库后再导入。

## 13. AI 检索相关性回归（2026-11-27）

- Context Broker 现按 query 的具体词项过滤和排序 L2 知识/插件候选；CVE 作为身份约束，匹配范围包含标题、摘要、item_id 和权威 ref_id。插件源对当前活动版本的全部 APPROVED 项评分后再截断 limit，避免先命中一批通用 `CVE` 记录造成截断偏差。
- 无关候选不再进入可信上下文；工具目录和当前任务的 Artifact 引用保持独立通道。历史 Artifact 仍按 task_id 限定，不把其他任务的产物注入本次上下文。
- 12K Agent E2E：CVE-2021-44228/Log4Shell、sourcemap 候选通道、Apache Struts 技术检索均 Agent Run / Tool Run 成功。Apache Struts 回归上下文保留 16 条相关 APPROVED Nuclei/CISA KEV 项，约 926 摘要 tokens；CVE 回归含精确 CVE Nuclei 引用和 CISA KEV/PoCIndex 证据。
- sourcemap 候选检索返回 5 条候选（3 Nuclei、2 RTIS），状态均为 CANDIDATE/DISCOVERED；候选只在 Artifact 中呈现，不进入可信知识摘要。
- 上述 E2E 使用 KeywordSearchPlanner 与估算 Token 口径，未做真实模型调用、未执行 POC/模板、未修改持久数据库。当前 PostgreSQL E2E 是空库，pg_intel 通道尚未覆盖。

## 14. 统一人工审核队列

- 队列索引：`governance-review-queue-index-2026-11-27.json`。
- CVE 身份：5,657 条，按 200/批，29 批；首批 200 条均为 KEV + CVSS 高风险优先级 P0。
- RTIS 武器：610 条，按 100/批，7 批；首批先处理 35 条疑似秘密、39 条真实 POC、9 条 EXP 和 17 条脚本（按优先级顺序）。
- 真实 POC 子集 41 条单独审核；35 条疑似敏感材料单独脱敏复核。
- Nuclei：14,111 条，按 200/批，71 批；P0 2 条解析异常、P1 525 条 KEV Critical/High、P2 27 条其他 KEV、P3 4,541 条 Critical/High。
- Skill→Tool 146 条、Skill→Knowledge/Vulnerability/Nuclei 173 条，共 319 条候选关系；仍全部是 CANDIDATE，未写入持久关系图。
- 新增 `build_vulnerability_review_batch.py`、`build_weapon_review_batch.py` 及测试；批次只包含哈希、来源、状态、风险元数据和审核字段，不导出原始 POC/模板正文，不执行任何内容。
- `poc-sensitive-review-batch-001` 的 JSON 尾部转义换行已规范化，现为单一有效 JSON 文档；队列索引及各首批 JSON 均通过 `python -m json.tool`。

持久写入状态：未写入个人 PostgreSQL。虽然已选 workspace=personal / PostgreSQL=Podman / review=batch，但当前有 `rip_m4_acceptance` 与 `rip_user_test_20261127` 两个数据库候选，尚未确认哪一个是实际 personal workspace 的目标库；因此只保留候选文件和导入器 dry-run。

## 15. CVE 精确身份查询

- 统一检索现在自动识别单一 CVE ID，并将 SQLite 通道路由到既有跨来源 `vulnerability_id` JSON1 结构化过滤；CISA KEV、PoCIndex、Nuclei 使用相同的 CVE identity，不要求编号必须出现在标题/摘要 FTS 文本中。
- 同一 query 包含多个不同 CVE 时不自动选第一个，避免静默缩窄检索范围。
- `CVE-2021-44228` 跨来源精确查询在独立 schema 8 派生索引中返回 46 条：CISA KEV 1、PoCIndex 1、Nuclei 44。Agent 在 12K 预算下真实调用统一检索工具成功。
- ACTIVE 默认通道 100 CVE 自一致性：Recall@10 / Precision@10 / MRR / Top100 完整覆盖均为 1.0；ACTIVE+CANDIDATE 显式通道同样为 1.0。
- 以上仅验证结构化身份过滤，不等同于 BM25/向量自然语言排序的人工相关性基准；之前的 4% 指标由于把 FTS 查询和 ACTIVE+CANDIDATE gold 混用，已在新报告中纠正，不再作为有效产品指标。RTIS 610 条自有武器目前仍没有结构化 CVE 关联。
