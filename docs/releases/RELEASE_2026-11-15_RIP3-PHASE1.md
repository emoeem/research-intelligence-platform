# RELEASE：RIP3 Phase 1 文档域与存储（阶段收口）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-15 |
| 阶段 | RIP3 Phase 1（文档域与存储，R2 第十四部分；ADR-0001 绞杀者路线首批落地） |
| 批次 | P1-1（PG 基座 + Document 域 + CAS + rp-doc CLI + SQLite WAL）+ P1-2（REST 通道 + 文档中心 web 面 + 浏览器实测 + 备份双轨演练） |
| 阶段提交 | c4c76a9（P1-1）→ 41bf3359（CI 修复 + P1-2 web 面）→ 15ff4063（docs 回填）→ 本收口提交 |
| 前置 | Phase 0 四冻结物已评审通过（用户 2026-11-14「通过」）；R2 §6.3/§10.1 数据库口径按用户裁决修订（ADR-0001） |

## 一、批次汇总

### P1-1（批次报告 docs/rip3/batches/P1-1-BATCH-REPORT.md）

- rip-pg17 镜像族：paradedb/paradedb:pg17@sha256:7cb29053 基座受控拉取，pg_jieba
  多阶段构建（third_party/ 内置源码含 cppjieba 子模块，零构建期网络），产物
  sha256:229189e4 台账三处同步；vector/pg_search/pg_jieba 三扩展实测在位，
  jiebacfg 中文分词实测正确。
- crates/intel-db：同步 PG 池（postgres 0.19 + r2d2/r2d2_postgres）+ refinery
  迁移执行（migrations_pg PG-0001 扩展 / PG-0002 Document 域五表，每文件事务 +
  checksum）+ Document 域存取（CAS 感知导入/多来源引用/版本链 SUPERSEDED/
  软删除/引用计数账面 source+revision 口径）+ LocalFs Object Store Adapter
  （cas:// 寻址读时 sha256 完整性复核，S3 接口预留）+ rp-doc CLI 八子命令。
- domain document.rs：四 ID 新类型 + 可见性/生命周期/版本状态词表（ids.rs 惯例）。
- SQLite WAL 显式化：Pool::enable_wal() + rp-tool/mcp-server RunChannel 可写运行库
  调用点；生产 migrations/platform*.db 不经此路径（指纹红线保持，journal_mode
  delete 断言在 selftest）。
- vendor 纯增量 85 外部 crate（postgres 栈 + refinery + icu 族，既有零升级）。

### P1-2（批次报告 docs/rip3/batches/P1-2-BATCH-REPORT.md）

- REST 通道：GET /api/documents + GET /api/documents/<id> + POST
  /api/documents/import（base64 → 0600 临时文件 → rp-doc 子进程，体帽 64MiB，
  鉴权/限频/审计三件继承；python 侧零 PG 驱动，进程边界即数据边界）。
- web 面：/documents 文档中心页挂 Knowledge 工作区第二 tab（4.2），上传表单 +
  列表 + 版本历史面板；vitest 基线 120 → 124（documents-smoke 4 用例）。
- 浏览器实测 7/7（真容器 + 真 rp_server + vite proxy + chromium 用户缓存）：
  真实文件上传流（setInputFiles → rp-doc → PG，对象新建 doc-18e7f60a）、新文档
  refetch 在列、版本历史面板、无文件前端拦截、零 pageerror / 零 console.error；
  截图三张 + summary.json 入库 docs/rip3/batches/browser-e2e/。
- 备份双轨演练：pg_dump custom → 全新实例 pg_restore（TOC 过滤 pg_cron/tiger/
  paradedb 基础设施面），四表逐行对账 19/10/23/19 全一致 + 三扩展在恢复库在位；
  SQLite 轨 rp-admin selftest 16/16。恢复实例用后即删。

## 二、P1 验收要点对账（R2 第十四部分 Phase 1 行）

| 验收要点 | 结论 | 证据 |
| --- | --- | --- |
| 同文件多来源引用 | 过 | 集成测试 same_content_multi_source_single_object（1 对象 2 来源 2 版本引用计数 4）+ add_source REST/CLI 面 |
| 内容不重复存储 | 过 | 对象账面单行断言 + LocalFs 单文件断言 + 跨通道复用（CLI 与 REST 各导同内容，content_object_created=false） |
| 更新生成新版本，历史可访问 | 过 | new_revision：旧版本转 SUPERSEDED 在列，revision_number 递增，历史行读面 |
| 删来源不误删 | 过 | delete_source 引用计数 -1，来源删光后对象由版本行持引用（账面保护），物理对象在位 |
| 内容与元数据可独立备份 | 过 | 备份双轨演练（pg_dump 四表对账 + rp-admin tar 对账；Object Storage 目录 tar 口径随 rp-admin 面沿用） |

## 三、门禁（阶段收口复测，2026-11-15）

| 门 | 结果 |
| --- | --- |
| Rust 全量 | cargo test --offline --workspace：667 passed 0 failed（P1-1 批实测；P1-2 批零 Rust 改动） |
| clippy | --workspace --all-targets -D warnings：零告警 |
| Worker unittest | 211 tests OK（skipped=11） |
| Web | build exit 0；vitest 124 passed（基线 120 + documents-smoke 4）；prettier 全过 |
| selftest | selftest_phase1_pg.sh 16/16 + selftest_phase10_release.sh 29/29 + selftest_auth.sh 11/11 |
| 生产库红线 | platform*.db 五库指纹一致 + journal_mode=delete 断言（selftest 内） |
| CI | P1-1 批 CI #63 failure（vendor 嵌套 .gitignore 挡 add，根因与修复见 P1-2 报告 E2 节）→ 修复 commit 41bf3359 CI success 2m59s |

复测期 flake 记录：无（本阶段无并发全量复测场景；单测/集成均为定向实跑）。

## 四、已知取舍

1. CI 无 PG service：intel-db 集成测试 CI 侧按 V-9 跳过（DSN 缺席口径），本地真容器
   实跑为准；PG service job 评估随 Phase 4 批。
2. /documents 页未纳入视觉门矩阵（随 Phase 5/6 页面族统一扩展）；axe/对比度由
   既有 token 面覆盖（muted 实色沿用 P10-4 交付）。
3. rip-pg17 镜像未做 pg_search BM25 索引建演练（V0002 尚无全文/向量列，Phase 4 批）。
4. vendor 增量为手工增量 vendor（cargo vendor 直连 crates.io 在本机网络环境不可达的
   绕行口径，与 rmcp 批次同法），checksum/磁盘/git 三方一致性已核。
5. 浏览器实测文件上传用 playwright setInputFiles；真实用户 file chooser 流由同一
   input[type=file] 承载。

## 五、回滚方式

Phase 1 无 SQLite 迁移、无生产库触碰：回滚 = revert 情报核面（intel-db/rp-doc/
migrations_pg/web documents 面随批 revert）；PG 实例与对象存储目录为运行时数据
（rip-data/，删除即清）；rip-pg17 镜像卸载即清（podman rmi）。RTIS 生产行为零改动
（integrations/rtis/ 本阶段零触碰，B5 守卫在 CI 常驻）。

## 六、队列位置

RIP3 阶段推进：Phase 0（审计冻结，afe6f5b）→ Phase 1（本 RELEASE）→ Phase 2
文档智能（rip-parser 容器族 + 解析管线 + IR 落库）待开批；窗口重排 5 窗方案的
窗口 1（Phase 0 + Phase 1 开批）按计划完成，窗口 2（Phase 1 收口 + Phase 2）启动。
