# Phase 4 实施差异文档（CONTEXT-BROKER-DESIGN 对照）

- 日期：2026-11-10（P4-6 收口汇编；种子为 PHASE4-DESIGN §5.3 差异对照表 D-1 到 D-3）
- 蓝本：docs/phase0/CONTEXT-BROKER-DESIGN.md（下称「设计稿」，ROADMAP §5.1 认定）；本文件按设计稿 §11 M4.5「文档（本设计补充实施差异）」要求汇编，覆盖批次执行期全部已知差异与待验证项处置
- 性质：差异只记「与设计稿不同且已落地的选择」，每条含设计稿原文位置、落地口径、依据；未列出的条目一律按设计稿原文执行

## 一、P4-0 冻结决议（种子三处，逐条兑现核销）

| 编号 | 设计稿位置与原文要点 | Phase 4 落地口径 | 依据与状态 |
|---|---|---|---|
| D-1 | §6.1（:177）迁移草案号 005_context_broker.sql | 008_context_broker.sql（本会话合入前 `ls migrations/*.sql` 实测 001 到 007） | ROADMAP §1.4 让位规则；P4-1 落库两次幂等无差异实测，已兑现 |
| D-2 | §4.3（:108）、§10（:350）Registry 未落地期用 Mock 源先行 | 目录源直连 tool-registry 公开接口（Registry::discover/query/schemas，lib.rs:93/98/103），Mock 降为纯测试载具 | P4-0 决议 2；P4-2 起全部批次按直连执行，已兑现 |
| D-3 | §8（:267）tier2 brief 在 Registry 登记期预生成随 Manifest 存储 | brief 生成落位 crates/context-broker（sources/registry.rs generate_brief + brief_for），装配期生成 + broker-cache 缓存（P4-5），登记链路零改动 | P4-0 决议 3；生成开销实测（cache hit 零生成、版本号变更重建，P4-5 测试），已兑现 |

## 二、批次执行期新增差异（P4-1 到 P4-5）

| 编号 | 设计稿口径 | 落地口径 | 依据 |
|---|---|---|---|
| D-4 | §13.3：RTIS knowledge.db 条数两处口径差（README 14717 与 rtis.py 头注 14664），影响「不默认塞入」测试规模口径 | 实测裁定 14717 条（本会话 `python3 -I` mode=ro 实查 `select count(*) from poc` = 14717，两处口径差核销；acceptance_no_stuffing 按此量级构造候选面） | P4-3 实测；测试规模口径已落 |
| D-5 | §4.3：past_success 取 tool_execution 历史成功率 | 恒 0 只留通路（tool_execution 当前 0 行，盘点:218 口径；CandidateMetrics.past_success None → 0，score.rs 文档明示），排序效果评估留首批 Run 数据 | 设计稿 §13.4 待验证项；不修，随 Phase 7 数据校准 |
| D-6 | §7.1：TTL 以条目 created_at（ISO8601）计算 | TTL 兜底判定用文件 mtime（put 原子改名后 mtime 即写入时刻，跨进程可靠；created_at 字段保留为人读对账面），语义等价（超期未命中重建） | std 无 ISO8601 解析面，不引时间库（最少依赖集 DEC-001）；P4-5 批次报告声明 |
| D-7 | §9.2 第 5 条「序列化后校验一次」 | 整体序列化 vs 总预算（fail fast）；连带小总量策略（约 600 tokens 以下）被结构开销拒绝，Phase 7 context_policy 下限需按此设定 | 校验对象「内联部分」的机械落面取最保守口径；PHASE4-ACCEPTANCE §三 命令实证 |
| D-8 | §6.1 context_ref_log EVICTED 事件 | 装配期溢出候选不写 EVICTED 事件（事件只指向已装条目，复合外键约束）；溢出记录在 budget_report.evicted 与 totals_json；EVICTED 事件留给运行期对已装 L3 展开的收回 | P4-4 执行中发现（ref_log 复合外键），语义等价（清单头快照承载驱逐留痕） |
| D-9 | §9.1 l1 catalog_digest_ref 与 counts | L1 装为单项条目（kind=doc，included=catalog-card，source=catalog，signals=l1:catalog-digest）；kind 取词表内最近项（设计稿 §6.1 kind 词表无 catalog 值） | 词表约束下的最小落位；008 CHECK 域未扩（避免迁移变更） |
| D-10 | §8 三级接口 tool_cards/tool_briefs/tool_schema | tool_briefs/tool_schema 落 RegistrySource 公开方法，tool_cards 落 RegistrySource::tool_cards；另增 ContextSource::brief_for 默认方法（端口面，默认 None）供装配取 brief | 端口隔离保留（trait 默认方法不破边界）；签名与设计稿 §10 等价 |
| D-11 | §13.2：ContextSource 与 Registry 对接签名待验证 | P4-2 实测定稿（discover/query/schemas 三接口直连 + ToolSummary/ToolDetail/VersionRow 投影），brief 键含 tool_version.content_hash | 设计稿待验证项核销 |
| D-12 | §13.1：Token 估算常数待校准 | `scripts/calibrate_est_tokens.py`（python3 -I 只读）对 bge-small-zh-v1.5 tokenizer.json（WordPiece，词表 21128）八类样本实校：est/bge 中位数 0.9333，落在容差 [0.8, 1.25] → 中位数口径常数保持；留痕 docs/phase4/est-tokens-calibration.json | ROADMAP §5.5 风险 1 收口。**样本级越界如实记录（2026-11-10 修复轮校正）**：brief-mixed 样本 0.7536 低于容差下限 0.8，est 对 brief 形态低估约 25%，L2 brief 预算按 est 计可能被真实 tokenization 超出；纯 ascii 标识符/路径类样本低估更重（0.40 到 0.48，bge 对 id/路径切分远细于 4:1）。两类均不修常数（中位数规则 + 预算分项相对意义），**brief 形态与重 id 面列入 Phase 7 校准范围**（届时按形态分桶修正常数或换精确计数服务） |
| D-13 | §6.2 向下/向上引用闭环（knowledge 源 L3 全文） | platform-kb 源 load 显式拒绝（sources/knowledge.rs）：platform 库 resource 表无正文载荷列（name/description/canonical_url 即全部），L3「全文」在 Phase 4 数据面不存在；接线随 Phase 7 知识正文存储定义（PHASE4-ACCEPTANCE §七.6）；rtis 源 poc get 不受此限（P4-3 已可用） | 验收意见修复轮新增；原「P4-4 expand 记账面」注释口径不准确已改 |
| D-14 | §7.1/§7.3 缓存设计 | BrokerCache 本批无淘汰与容量上限（过期条目永留磁盘、stats 只增）；生产化前加容量上限/清扫或评估维护中的内容寻址缓存库（读时校验语义层保留平台自定义） | 验收意见修复轮新增；移交 Phase 7 生产化（PHASE4-ACCEPTANCE §七.7） |
| D-15 | §6.1 EVICTED/CITED_* 事件词表 | CITED_IN_TOOL_CALL 存储面就绪（008 CHECK 域 + append_ref_event）但 Phase 4 零发射点（grep 全 crates/ 无调用方）：验收方裁定接受 Phase 7 边界，ROADMAP §5.4 第 4 条期望已修订（2026-11-10 注记）并入 Phase 7 验收清单 | 验收意见修复轮新增（PHASE4-ACCEPTANCE §七.1） |
| D-16 | §9.1 id 形态「ctx-<ulid>」 | 落地为 sha256 截断 id（无 ULID 依赖，最少依赖集）；熵源经修复轮升级：纳秒时戳 + 进程熵（RandomState OS 种子，RandomState::new 每实例独立）+ pid + 进内序号，替代原秒级 now_iso + 序号（同 task 同秒跨进程 100% 碰撞，第二次 INSERT 撞 UNIQUE 硬失败，两进程实测复现后修复） | 验收意见修复轮第 2 轮新增；回归测试 p4_4r2_new_id_unique_across_processes_same_second（CARGO_BIN_EXE 双进程背靠背同秒装配，两成功且 id 不同 + 库内两行） |

## 三、待验证项处置汇总（设计稿 §13 逐条）

| 设计稿 §13 条目 | 处置 |
|---|---|
| 1 Token 估算常数未校准 | 已校准（D-12，verdict keep） |
| 2 ContextSource 与 Registry 签名 | 已定稿（D-11） |
| 3 RTIS 条数口径差 | 已裁定 14717（D-4） |
| 4 tool_execution 0 行 past_success | 维持待验证（D-5，Phase 7 数据到位后校准） |
| 5 向量检索轴 | 按边界不实现（预留接口位不落地），Phase 4 范围外 |
| 6 RTIS 子进程超时与失败语义 | 已实测（P4-3 假 kit 三类 + 真实 kit：5s 超时单次重试两态落定，eri_adapter.py 参照） |

## 四、非差异声明（复核过的等价面）

- 三表列集与设计稿 §6.1 草案一致（另补 selection_hash，§7.3 明文）；五类 CHECK 词表与 domain::context 枚举一一对应
- 四级加载、七分项预算、五权重打分、同 content_hash 去重、降级链、驱逐顺序、citations 分离、expand 记账均按设计稿原文执行并有验收测试落证
- 复用纪律：tokenize/expand_keywords/keyword_hits（resource-engine）、SearchEngine::search（search-engine）、emit_best_effort（audit-engine）、discover_migrations/migrate（database）、sha256_hex（tool-registry）、原子写（worker 先例）全部走既有公开接口，Broker 内零重写
