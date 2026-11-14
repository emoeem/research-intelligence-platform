# RIP3 批次报告：P1-1（PG 基座 + Document 域 + CAS + 导入 CLI + SQLite WAL，§17.6 格式）

## A 完成内容

1. **PG 基座**：rip-pg17 镜像族建成（基座 paradedb/paradedb:pg17@sha256:7cb29053 受控拉取；
   pg_jieba 多阶段构建，third_party/ 内置源码含 cppjieba 子模块，零构建期网络；产物
   digest sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45，台账
   containerfiles/README.md 三处同步）；vector/pg_search/pg_jieba 三扩展实测在位；
   jiebacfg 中文分词实测正确（研究/开源/架构/项目/质量切词）。
2. **crates/intel-db**（新 crate，publish=false）：同步 PG 连接池（postgres 0.19 +
   r2d2/r2d2_postgres）、refinery 迁移执行（migrations_pg/ PG-0001 扩展 + PG-0002
   Document 域，每文件事务 + checksum）、Document 域存取（CAS 感知导入/多来源/版本链/
   软删除/引用计数账面）、LocalFs Object Store Adapter（cas:// 寻址 + 读时 sha256
   完整性复核，S3 接口预留）、rp-doc CLI（import/add-source/new-revision/
   delete-source/soft-delete/list/show/health 八子命令）。
3. **domain 扩展**：crates/domain/src/document.rs（DocumentId/DocumentRevisionId/
   BlockId/SourceRecordId 新类型 + DocumentVisibility/DocumentLifecycle/
   RevisionStatus 词表，ids.rs 惯例）+ DomainError 五新变体。
4. **SQLite WAL 显式化**（R2 §6.9）：database::Pool::enable_wal()（journal_mode=WAL
   + synchronous=NORMAL），调用点 rp-tool open_pool 与 mcp-server RunChannel::open_pool
   （可写运行库面）；生产 migrations/platform*.db 不经此路径（指纹红线保持）。
5. **vendor 增量**：Cargo.lock 纯增量 85 外部 crate（postgres 栈 + refinery + icu 族等，
   既有零升级），手工增量 vendor（缓存解包 + .cargo-checksum.json 生成，cargo vendor
   直连 crates.io 的设计行为在镜像受限环境下绕行，与 rmcp 批次同法）。

## B 架构变化

- 双库格局落地：情报核 PG（documents/document_revisions/document_blocks/
  content_objects/source_records 五表）+ 运营核 SQLite（不动）。
- 新 crate 边界：intel-db 不触 SQLite 面、不做 RTIS 直写（R2 3.4）；domain 纯类型。
- 引用计数口径：ref_count = source_records 行数 + document_revisions 行数（同事务维护）。

## C 文件变化

新增：crates/intel-db/（lib/document_repo/object_store/bin/rp-doc/tests）；
migrations_pg/V0001__extensions.sql + V0002__document_domain.sql；
containerfiles/rip-pg17/（Dockerfile + third_party/pg_jieba 含 cppjieba，.git 嵌套已除）；
crates/domain/src/document.rs；apps/server/selftest_phase1_pg.sh。
修改：Cargo.toml（成员 + 依赖锁定段）/Cargo.lock（+85）/domain lib+error/database pool.rs
/ rp-tool.rs / mcp-server dispatch.rs / containerfiles/README.md（台账行）。

## D 数据库变化

- PG：refinery 链 PG-0001（扩展）+ PG-0002（Document 域五表 + 4 索引 + 2 CHECK 域）；
  无 SQLite 新迁移（020 号仍空闲给运营面）。
- SQLite：journal_mode 变化仅发生在显式 enable_wal 的可写运行库（tool-runs 面）；
  生产 platform*.db 零触碰（selftest 指纹断言 + journal_mode=delete 断言）。

## E 测试结果（全部实际执行）

| 门 | 命令 | 结果 |
|---|---|---|
| Rust 单测 | cargo test --offline -p intel-db | 10 passed（object_store 5 + domain 5） |
| 集成测试（真容器） | RP_TEST_PG_DSN=… cargo test --offline -p intel-db --test document_domain | 5 passed 0 failed（V-9 口径：DSN 缺席跳过） |
| selftest | bash apps/server/selftest_phase1_pg.sh | 16/16 PASS exit 0（隔离容器 + 零碰生产） |
| 全量门 | cargo test --offline --workspace / clippy -D warnings / unittest / selftest_phase10_release / selftest_auth | 见收口提交记录（全绿数字） |

## F 性能结果

imports/查询均为小样本（selftest 秒级）；规模数字属 Phase 8 基准，不在本批。

## E2 CI 结论

push 后 GitHub CI（含 rust/parity/python-workers/web 四 job）结果见收口提交记录；
CI 无 PG service：intel-db 集成测试按 V-9 跳过（DSN 缺席口径），纯逻辑单测常跑。

## G 遗留问题

1. rip-pg17 镜像内未做 pg_search 索引建索引演练（V0002 尚无向量/BM25 列，Phase 4 批）。
2. CI 无 PG service：CI 侧 intel-db 集成面为跳过口径（本地实跑为准），PG service job
   评估随 Phase 4 批（hybrid 面需要）。
3. vendor 增量为手工增量 vendor（85 目录 + checksum 生成），cargo vendor 直连
   crates.io 在本机网络环境不可达的绕行口径，已记录。

## H 下一阶段依赖与顺序

P1-2 收口批：REST 面（/api/documents GET + POST 子进程白名单，沿 rp_server 惯例）+
基础上传 UI（Documents 页，browser-use 实测）→ Phase 1 收口（五道门禁 + RELEASE +
CI 硬门槛）→ Phase 2 开批（rip-parser 容器族）。
