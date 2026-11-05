# 2. ERI_ARCHITECTURE_REVIEW

日期：2026-11-05　对象：`eri/eri.py`(P0) + `eri_p1.py` + `eri_p2.py`，共 2,701 行 + 15 表。全部为本 M0 前三阶段刚交付的代码，审计基于一手实现记忆 + 复查。

## 2.1 模块与职责实况

| 组件 | 职责 | 数据 | 成熟度 |
|---|---|---|---|
| eri.py | 解析/URL 规范化/D1-D4 去重/入库/增量同步/元数据检索 | resources + sources + categories + sync_state | 生产，幂等实测 |
| eri_p1.py | GitHub 元数据补全（ETag/限流/断点）、License/维护分类、D5 初筛、质量评分、隔离、Skill 映射、查询、备份 | +6 表 | 生产，100% 补全 |
| eri_p2.py | ResourceResolver（六维加权）、RTIS gate 钩子、ResourceReader（缓存+回落）、UsageTracker（七态）、三审核队列、100k 性能器 | +4 表 | 生产，端到端实测 |

## 2.2 做对了的（应保留进 Architecture 2.0）

1. **单写者纪律**：每个库一个写入口（rt.py / eri*.py），无锁协议但无写竞争。这是当前系统能用 SQLite 稳定运行的根本原因。
2. **确定性优先**：全链路零 LLM（P0-P2 累计 AI 调用 0），成本可控、结果可复现、测试可离线。
3. **来源强制**：无来源不入库；canonical_url 与原始 URL 分离；commit/行号级溯源。
4. **软删除**：UNLISTED/QUARANTINED/UNAVAILABLE 全走状态位，无物理删除。
5. **断点与幂等**：commit+hash 双校验 no-op；enrichment_state 逐资源断点；403/429 停止保存进度。
6. **传输层可注入**：测试用假 transport 全离线，57+ 断言秒级跑完。
7. **存储抽象事实上存在**：DB_PATH 环境变量 + SQL 集中，P2 性能测试直接换库路径跑 10 万行。

## 2.3 结构性弱点（Architecture 2.0 要解决的）

1. **单文件增长模式**：P0→P1→P2 三个文件靠 import 链（p2→p1→eri）叠加，全局可变状态（DB_PATH/CACHE_DIR 被测试 monkeypatch）在多进程/多 Worker 下会踩。
2. **评分与阈值硬编码在代码里**：RESOLVER_WEIGHTS、质量分项权重、D5 阈值、CJK_BRIDGE 全是常量，调参要改代码。
3. **评分无回归集**：权重"初始可调"但没有黄金查询集来自动验证调参后果（RTIS 侧有 gold set 概念，ERI 侧没有）。
4. **reader_cache 摘要是章节目录而非语义摘要**：LLM 摘要层留了表结构（summary/summary_content_hash）但无实现。
5. **CJK_BRIDGE 17 组查表太薄**：画像到检索词的翻译覆盖率未量化（无中英对齐评测集）。
6. **rtis 集成是 subprocess 钩子**：每 gate 一次进程冷启动（实测可接受但属临时方案），错误静默策略让集成故障不可见（无 error 事件上报）。
7. **无 schema 版本表**：catalog.db 无 user_version/迁移日志，P0→P1→P2 的表变更靠 CREATE IF NOT EXISTS 叠加。

## 2.4 对统一平台的输入

- Resource/ResourceVersion/Review/AuditEvent 四个域实体已在 ERI 侧有事实结构，UNIFIED_DOMAIN_MODEL 直接以其为蓝本。
- "检索即执行"的纪律文案（MCP 工具描述里）与平台"AI 先检索后行动"原则一致，接口层可直接沿用。
- ERI 的 81 条真实数据 + 全部测试是迁移的回归锚点：任何新实现必须复刻 `tests/run_tests.py` 的 8 组行为。
