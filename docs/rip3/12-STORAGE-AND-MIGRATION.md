# RIP3 存储与迁移策略（12-STORAGE-AND-MIGRATION，冻结物 3）

> 状态：Phase 0 冻结（2026-11-14）；依据：用户裁决（ADR-0001 PostgreSQL 绞杀者路线）、
> 00-ARCHITECTURE-AUDIT.md 第二节 B 组实证。冻结范围：数据分层、CAS 语义、迁移纪律、
> 备份双轨；PG DDL 细节与索引参数按 Phase 1 批次 ADR 滚动。

## 一、数据分层（R2 10.1 四层）

| 层 | 内容 | 引擎 | 说明 |
|---|---|---|---|
| A Metadata | 情报核（Document/Revision/Block/Chunk/KnowledgeCard/Relation/Finding/ResearchTask/Report/治理面） | **PostgreSQL 17**（PG-0001+ 链） | 结构化事实、FTS、向量、JSONB 过滤、RLS |
| A' Metadata 运营核 | tool_run/agent_run/workflow_run/plugin/runtime_node/audit_event/external_api_audit | SQLite（现状 001-019，020+ 增量） | 高频写、小行、零运维；Phase 8 基准后裁决是否移植 |
| B Object Storage | 原始文件/提取图片/解析产物/报告文件/大 Artifact | Object Storage Adapter：本地 FS 起步（rip-data/object-store/，沿 paths.rs 惯例），预留 S3 兼容接口 | 业务代码禁止拼文件系统路径（R2 10.1B），统一走 Adapter trait |
| C Search Index | PG 内 FTS（pg_jieba/pg_search）+ pgvector + 既有 SQLite FTS（运营核知识面） | 双库各自索引，统一 Search API 归并 | 全部支持独立重建（索引版本号） |
| D Cache | 解析/Embedding/摘要/检索/Context 缓存 | 复用既有内容寻址缓存骨架（cache.rs:117-207）+ 新增 Query/Summary Cache | 全部可失效可重建 |

## 二、CAS 内容寻址存储（R2 10.2）

1. 寻址键 = sha256(content)；content_objects 表（03-DOMAIN-MODEL 1.8）为账面，
   对象本体落 Object Storage（路径 = cas:// 前两字节分桶 + 全哈希，防目录爆炸）。
2. 引用计数 ref_count：source_records 与 document_revisions 引用时递增（同事务）。
3. 删除保护：ref_count > 0 禁物理删除；Document 软删不释放对象。
4. 孤儿清理：ref_count = 0 且超宽限期（缺省 30 天）的孤儿对象由 retention 批清理，
   清理前 sha256 复核（对齐 backup.rs 校验惯例）。
5. 完整性检查：定期抽样 sha256 重算 + 全量校验命令（rp-object verify，P1-3 交付）。

## 三、PostgreSQL 侧迁移纪律（PG-0001+）

1. 工具：refinery（同步、每文件事务、checksum 校验，MIT/Apache-2.0），迁移文件
   migrations_pg/V0001__*.sql 命名；执行器嵌入 database crate 新模块（pg.rs），
   与既有 migrate.rs 并列不互扰。
2. 每文件显式事务（refinery 内建）；checksum 不匹配即拒（防手改历史）。
3. 测试副本验证：迁移在临时 PG 实例（容器）上先跑再合入；CI 起 postgres service
   容器（pgvector/pgvector:pg17 自建扩展镜像，digest 钉死入 containerfiles/README 台账）。
4. 扩展装载顺序：V0001 = CREATE EXTENSION vector; V0002 = pg_jieba 配置；
   V0003 起 = 业务表（03-DOMAIN-MODEL 实体）。
5. 既有 SQLite 迁移器零改动（红线邻接：19 次幂等验证的载具不动）；其无显式事务缺口
   以「SQLite 侧仅运营面小迁移 + 断电注入测试」管控（第五节）。

## 四、SQLite 侧（运营核）增量与修复计划

1. 编号从 020 起（00-审计修正：019 已被 P9-3 占用）；新增式、ALTER 守卫沿用
   （migrate.rs:71-112）。
2. WAL 修复（P1-1 批）：pool.rs open_connection 补 journal_mode=WAL +
   synchronous=NORMAL；新增 PRAGMA 自检脚本（selftest 断言 journal_mode/
   busy_timeout/foreign_keys 实际生效），对齐 R2 6.9。
3. 运营核新增迁移仅限：审计/索引类；情报核任何新表不得落 SQLite（绞杀者规则）。

## 五、备份与恢复（双轨）

| 轨 | 载具 | 频率口径 | 校验 |
|---|---|---|---|
| SQLite 运营核 | rp-admin backup/verify-archive/restore（P10-1 面，不动） | 沿用现状 | sha256 manifest + integrity_check |
| PostgreSQL 情报核 | pg_dump --format=custom（逻辑）+ pg_basebackup（物理，Phase 8 评估） | P1-1 起每日可执行 | restore 到临时实例 + 行数/抽样对账 + pg_amcheck |
| Object Storage | 对象目录 tar + sha256 manifest（复用 rp-admin 归档形状） | 随库备份 | 逐成员 sha256 |

恢复演练纳入每阶段 selftest（对齐 2.0 P10-1 演练纪律）。

## 六、规模化约束（R2 10.4 落地口径）

分页读取（cursor 分页 keyset）、批量写入（COPY/多值 INSERT）、增量索引
（embedding_status 批处理）、幂等键（content_hash/revision_number）、软删除、
索引版本、任务优先级（workflow 既有控制面）、连接上限与 prepared statement 复用；
PG 参数（work_mem/max_wal_size/autovacuum）在 Phase 1 基座批定基线，Phase 8 按基准调。
未来迁移路径明确 = VectorBackend/StorageAdapter/Provider 三接口已隔离供应商
（R2 10.3 九个 Provider 接口清单全部保留为 trait 面）。
