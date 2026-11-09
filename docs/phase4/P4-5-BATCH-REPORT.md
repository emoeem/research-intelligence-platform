# 批次验收报告：P4-5（内容寻址缓存、七类失效触发器与 rp-context CLI）

- 批次：P4-5（第 6/7 批 = 设计稿 CONTEXT-BROKER-DESIGN §11 M4.4；2026-11-10）
- 修改范围：crates/context-broker（cache.rs 新增 + bin/rp-context.rs 新增 + assemble.rs Broker 缓存位与 expand 读载荷缓存 + sources/registry.rs brief 缓存位 + lib.rs 登记）、tests/cache_invalidation.rs（新增 10 项）、docs/phase4/（本报告）
- 依赖面变更：零（缓存键哈希复用 tool_registry::hash::sha256_hex，原子写为 std::fs tmp + rename，worker_runtime.py:587-591 先例同构；无新第三方、无新内部依赖）
- 新增接口：BrokerCache（open/default_dir/cache_key/put/get/stats）、CacheValidation（version/content_hash/source_fingerprint 三元组）、CacheEntry、CacheStats、TTL_CATALOG/TTL_KNOWLEDGE/TTL_SCHEMA/TTL_RTIS 分层常量、Broker::with_cache、RegistrySource::with_cache、ContextSource 面不变
- 数据变更：缓存目录为运行时数据（默认 /tmp/rip-data/runtime/broker-cache 或 $RIP_DATA_DIR，gitignore 面外）；开发库副本 dev-p4.db 由 CLI 幂等迁移使用；生产 platform*.db 零写入（红线，git status 无任何 *.db 行）

## 判据逐条（P4-5 批次 ask）

1. **缓存目录可配置不硬编码**：BrokerCache::default_dir 读 env `RIP_DATA_DIR`（缺省回落系统临时目录 rip-data/runtime/broker-cache），CLI --data-dir 可覆盖；设计稿 §7.1 `<rip-data>/runtime/broker-cache/` 形状保持。
2. **validation 读时比对 + TTL 仅兜底 + 失败不落缓存无负缓存**（设计稿 §7.1）：get 对 version/content_hash/source_fingerprint 三元组逐字段比对 + policy_version 比对 + 文件 mtime TTL 兜底（put 原子改名后 mtime 即写入时刻，跨进程可靠）；坏 JSON 条目视同未命中重建；put 只在取数成功后由调用方写入，API 面无失败条目（负缓存无落点）。
3. **七类触发器逐条失效用例**（cache_invalidation.rs，设计稿 §7.2 表逐条）：
   - 触发器 1 工具清单 version/digest：version 变化未命中 + content_hash 变化未命中（p4_5_trigger1）；
   - 触发器 2 知识治理跃迁：fingerprint governance 分量 ACTIVE→QUARANTINED 未命中（p4_5_trigger2）；
   - 触发器 3 INDEX_SCHEMA_VERSION：index_schema=1→2 未命中（p4_5_trigger3）；
   - 触发器 4 knowledge.db mtime+size：rtis_db 分量变化未命中（p4_5_trigger4）；
   - 触发器 5 插件状态：plugin=ENABLED→DISABLED 未命中（p4_5_trigger5）；
   - 触发器 6 policy_version：policy-1→policy-2 未命中（p4_5_trigger6）；
   - 触发器 7 broker_version：broker 分量变化未命中（p4_5_trigger7）。
4. **改一个工具版本号后 cache-stats 显示对应条目重建（M4.4 判据）**：rip.doc.parse 登记 9.9.0 → 首次 brief 生成 stats entries=1；同版本再取命中（entries 仍 1，零重建）；登记 9.9.1 → brief 重建，stats entries=2（新键新条目），旧 9.9.0 条目按旧 validation 仍命中（历史不受失效影响）；新旧 brief 内容不同（随登记面更新）。
5. **rp-context 四子命令对开发库副本跑通（ROADMAP §5.4 第 2 组命令形式）**：本会话实测（仓库根，命令原文见下节）：
   - `cargo run --offline -p context-broker --bin rp-context -- assemble --task-id t-acceptance`：退出码 0，签发 ctxm-14c4b1316d5ebe159bb33da5（intent ok、L0 七规则、L1 计数、citations 齐全）；
   - `-- manifest-show --manifest-id ctxm-...`：退出码 0，items 含 catalog:digest（catalog-card）与 rp-rules:*（summary）；
   - `-- expand --manifest-id ctxm-... --item-id rp-rules:on-demand-loading`：退出码 0，granted_tokens=55、remaining_l3=1997（l3_reserved 扣账），载荷落缓存（cache-stats entries=1）；
   - `-- cache-stats`：退出码 0，{dir, entries:1, bytes:437}。
6. **历史 Manifest 不受缓存失效影响**：缓存条目删除后 expand 重建成功，manifest 的 selection_hash 与条目集逐字节不变；缓存存在与否不改变装配分配语义（两次装配 allocated 相等）。
7. **Broker expand 读载荷缓存**：首次 expand 未命中生成并落缓存（stats +1），第二次命中（stats 不变）且 EXPANDED 记账照常（ref_log 事件 +1，扣账与未命中路径一致）。
8. **【串行】依赖 P4-4**：Broker/ManifestStore/expand 面为 P4-4 交付物，本批只增缓存位（with_cache 构建器，P4-4 测试零改动全绿）。

## 执行中修正记录（如实声明）

1. **TTL 判定落面**：设计稿 TTL 以条目 created_at（ISO8601 字符串）计算需时间解析面，定稿改用文件 mtime（put 原子改名后 mtime 即写入时刻，跨进程可靠），语义等价（写入后超过 TTL 未命中重建）；created_at 字段保留（人读对账面）。
2. **clippy 三处**：let-and-return（build_broker）、needless_borrow、match 通配臂类型（usage() 改 return usage()），语义不变。
3. **entry 读取错误面**：read_dir 逐项错误跳过（不炸 stats）；坏 JSON 条目视同未命中（重建），不缓存失败态。

## 测试命令与结果（实测）

- `cargo test --offline -p context-broker --test cache_invalidation`：10 passed 0 failed（回环/原子写/无负缓存 + 七触发器 + M4.4 版本重建 + expand 载荷缓存 + 历史 Manifest 稳定）
- `cargo test --offline -p context-broker`：56 passed 0 failed（lib 1 + assemble_contract 10 + cache_invalidation 10 + catalog_sources 5 + intent_pure 11 + policy_estimate 13 + rtis_source 6）
- `cargo test --offline -p domain -p context-broker -p database`：114 passed 0 failed
- `cargo test --offline --workspace`：268 passed 0 failed（P4-4 基线 258 + 本批 10）
- `cargo clippy --offline --workspace --all-targets -- -D warnings`：零告警，退出码 0
- `cargo fmt --all -- --check`：通过
- `grep -P '[\x{2014}\x{2013}]'`（本批全部新文件）：退出码 1
- CLI 四子命令：assemble/manifest-show/expand/cache-stats 全部退出码 0（输出摘要见判据 5）

## 未执行的测试

- TTL 过期路径的时钟推进用例：mtime 兜底语义未直接实测超期（超期即未命中重建的代码路径与坏条目路径同构，未做时钟快进）；分层 TTL 常量为设计稿默认值，校准随 P4-6
- knowledge/rtis 源的缓存接线（rtis 检索结果 TTL 10 分钟）：CLI 与 Broker 面已具缓存位，rtis 源接线随 P4-6 验收端到端
- CI 面的 CLI 冒烟：CLI 属本地验收命令（ROADMAP §5.4 本地口径），CI 不新增

## 对 ERI / RTIS / KB / Skills / Tools 的影响

- RTIS 本体零改动（红线）；ERI/KB 无接触；Tool Registry 登记面零改动（缓存只读其公开接口 + tool_version.content_hash）

## 风险

- 缓存目录默认回落临时目录：持久化部署需显式设 RIP_DATA_DIR（CLI 与文档口径一致，P4-6 收口时入 README 面）
- brief 缓存键含 tool_version.content_hash：登记面重注册同版本（code 刷新）会改 content_hash → 旧条目失效重建（符合触发器 1 语义，无脏读）
- 原子写依赖同目录 rename 原子性（POSIX）；Windows 面不承诺（workspace 目标 Linux，ADR 口径）

## 回滚方式

- 代码：`git revert <批次 commit>`；cache.rs 与 bin/rp-context.rs 整文件新增，revert 即移除；Broker/RegistrySource 的 with_cache 与缓存位随批回滚（P4-4 测试零改动面，回滚后原签名仍编译）
- 数据：缓存目录（/tmp/rip-data 或 $RIP_DATA_DIR）整目录删除即清净，运行时数据不入库不进 git；生产库零写入，无生产回滚面
- 文档：本报告随批回滚

## 后续工作

- P4-6（M4.5 收口）：验收三测试（不默认塞入/可解释/Artifact 引用）进 CI + 实施差异文档汇编（P4-0 §5.3 种子）+ 阶段更新记录（docs/releases/）
