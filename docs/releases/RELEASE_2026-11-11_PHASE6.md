# RELEASE 2026-11-11 Phase 6：Plugin Runtime 与 Plugin Center

> Phase 6 交付了插件全生命周期：从来源拉取到 Manifest 校验注册，从 Worker
> Protocol v1 握手到 Tool Registry 落库露出，从工具脚手架一条线到 Plugin
> Center 网页操作回路。八批次串行交付，自验全部通过。

## 元信息

| 字段           | 值                                                                                                    |
| -------------- | ------------------------------------------------------------------------------------------------------ |
| 阶段交付       | P6-1 到 P6-8（八批次串行，本日收口）                                                                   |
| 数据库迁移     | 009_plugin.sql + 010_knowledge_ingest.sql（开发副本与测试内存库验证；生产库 platform.db 未触及）        |
| 依赖增量       | serde_yaml_ng 0.10.0 + unsafe-libyaml 0.2.11 + ryu 1.0.23 + tar 0.4.46 + filetime 0.2.29（增量 vendor） |
| 自动化测试     | cargo test --workspace exit 0（72 个 test result: ok）；vitest 73 tests 全过；Playwright 8 passed       |
| 静态检查       | clippy --workspace --all-targets -D warnings exit 0；tsc --noEmit exit 0                                |
| selftest       | selftest_phase5 exit 0（PASS=31 FAIL=0）；selftest_phase6 exit 0（PASS=49 FAIL=0）                     |
| 红线核查       | platform*.db 未写；integrations/rtis 生产零改动（sha256 前后一致）；宿主机零安装                        |

## 分章正文

### 一、插件数据面（P6-2/P6-5）

- `migrations/009_plugin.sql`：plugin 版本化行（十二态 + is_active 升级指针 +
  governance 治理轴 + source_uninstalled 孤儿来源标记）+ plugin_capability 七类型行。
- `migrations/010_knowledge_ingest.sql`：knowledge_ingest 摄取落地表（溯源三元组
  plugin_id/plugin_version/corpus_id + content_sha256 幂等键）。
- `crates/domain/src/plugin.rs`：Plugin/PluginCapability 域实体 + PluginId 新类型
  + PluginState 十二态 + PluginKind/CapabilityType/PluginTrust 词表枚举。
- RTIS 语料真实摄取：15155 行（14717 poc + 125 skill_md + 313 method_case），
  6286781 字节正文，幂等重跑 skipped=15155。
- 条数口径收敛（V-12 关闭）：README 14717 为权威口径，rtis.py 头注 14664 为陈旧快照。

### 二、插件生命周期（P6-2）

- `crates/plugin-runtime/src/lib.rs`：PluginRuntime 门面 + §5.2 跃迁表十四行。
  install → REGISTERED → enable → disable（复合步先 deactivate）→ uninstall（前置
  自动 disable + §5.4 保留处置报告）→ upgrade（§5.3 新旧并存 + 失败自动回滚）。
- `store.rs`：plugin/plugin_capability 两表唯一写面。
- `retention.rs`：§5.4 disable/uninstall/upgrade 三份机器可读处置报告。
- `upgrade.rs`：§5.3 升级计划 + 回滚窗口（草案 D6 保留上一版本一个）。
- `bin/rp-plugin.rs`：CLI（五类操作 + update-check + ingest + list + 主链辅助步）。

### 三、五类来源接入（P6-4）

- `crates/plugin-registry/src/source/`：五类来源 trait + 实现。
  local_dir（git HEAD pinned 复核）、git（file:// 闭环 + commit 存在性）、
  archive（sha256 对账 + tar 解包）、mcp_server（stdio MCP 2024-11-05 握手）、
  http_api（最小 GET 契约哈希）。
- `lock.negotiation` 字段：mcp_server 握手快照 / http_api 契约哈希 / git
  resolved_commit / local_dir resolved HEAD 进 plugin.lock。

### 四、Plugin Center 网页（P6-8）

- `web/src/routes/PluginsPage.tsx`：四分类目录（已安装/可更新/已禁用/其他）。
- `web/src/routes/PluginDetailPage.tsx`：三级加载（L1/L2/L3 按钮）+ capability
  清单 + 兼容性表 + 依赖图 + 审计 trail + 写操作按钮（状态驱动出现）。
- `apps/server/rp_server.py`：/api/plugins 读端点（RIP_PLUGIN_DB 只读直查）+
  写端点（rp-plugin 子进程，零直连 SQL 写）+ Bearer 强制。
- `web/qa/plugins/`：12 张真实浏览器截图（视觉矩阵 8 张 + 真实后端 E2E 4 张）。

### 五、武器库扩展路径（P6-9）

- `docs/guides/ADDING-A-TOOL.md`：一条线可复制扩展指南（五步命令块）。
- `scripts/rip_new_tool.py`：stdlib-only 脚手架（D-TC-02 正则 + Manifest 骨架 +
  worker stub + rp-tool validate/register + rp-tool-health ping + 露出核验）。
- `crates/tool-registry/src/bin/rp-tool.rs`：validate/register/list/get CLI。
- `crates/tool-engine/src/bin/rp-tool-health.rs`：Worker Protocol v1 ping CLI。

## 已知取舍

1. Context Broker L1 插件目录源未接入：CB 的 L1 是 Phase 4 交付，不含 plugin 表。
2. LICENSE 文件选型：维持 UNLICENSED，LICENSE 文件移交维护者。
3. FTS/向量索引对接：摄取行进检索面属 Phase 4 search-engine 轴向。
4. upgrade 数据迁移钩子与回滚窗口清理：归 P10-2。
5. http_api 契约路径是 PLUGIN-CONTRACT §12.4 待验证项，本批落最小 GET 形态。

## 回滚

- 数据面：`git revert` 迁移提交（009/010 追加结构，删除表即可回退）。
- 插件副本：`rm -f migrations/dev-p6*.db /tmp/rp_p6_*.db`（开发数据不入库）。
- rip-data：`rm -rf rip-data/`（gitignore 覆盖，运行时数据可再生）。
- web 前端：`git revert` 路由与页面提交（/plugins 路由移除即回到 P5 基线）。
