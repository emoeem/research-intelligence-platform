# 更新说明 · 2026-11-12（SK-2 三源知识来源接入）

> 三个公开安全知识来源（Awesome-Hacking、SecSkills、tongui 安全课程）以独立知识插件接入 RIP：固定 Commit 快照安装、目录条目元数据摄取、审核闸与检索面接线、Web 来源展示全部走通。tongui 课程（无 LICENSE）只登记章节/标题/URL/路径元数据，正文本体零落库零入仓。总量 262 条目录元数据幂等可重放，50 URL 抽样链接与分类全对账。

| 项         | 值                                                                                                          |
| ---------- | ----------------------------------------------------------------------------------------------------------- |
| 阶段交付   | git `<本批提交短哈希>`（入库时回填，04:xx CST）                                                              |
| 数据库迁移 | 无新增迁移（消费 011 knowledge_source_entry，SK-1 批次已建）；实跑用开发库副本 /tmp/sk2.db，生产 platform*.db 未触及 |
| 自动化测试 | cargo test plugin-registry + plugin-runtime 全绿（61 个 test result: ok，新增 static_chain 2 项 + sk2_source_ingest 4 项）；web vitest 73 全绿 |
| 静态检查   | cargo clippy --workspace 零告警；web tsc --noEmit 零错误                                                     |

## 一、面向使用者的变化

1. 插件中心（/plugins）新增三个知识来源插件卡片，带许可证徽章、快照 Commit 短哈希、来源条数与审核计数；详情页（L2）新增来源主页链接、快照 Commit 全值、内容类型与 per-kind 许可/持久化策略、审核状态计数与最近摄取时间（apps/server /api/plugins 增量字段，P6-8 既有契约不破坏）。
2. 知识库页（/knowledge）新增「知识来源」视图：插件来源条目列表，展示审核状态、条目类型、许可证、持久化策略、快照 Commit、分类路径与原始 URL，支持按插件/审核状态/关键词过滤；平台知识面行为不变（新增 /api/knowledge_sources 只读端点，缺表守卫降级）。
3. 检索面：三源 262 条审核通过条目进入 search-engine 第三源（doc_count 262），禁用任一插件重建索引后其条目立即撤出（实测 262 → 160）。

## 二、实现与根因

1. 三份 Manifest（manifests/plugins/，yaml/json 双份等价）按 schemas/plugin-manifest/v1.json 全文校验 + rule 3 per-kind 声明：source.type=git + pinned.kind=commit 固定计划 §3 核实值；license_policy 分别为 catalog_only（awesome-hacking）、catalog_only + metadata_only（secskills）、metadata_only（tongui）。git 通道 install 实跑完成临时 clone + commit 存在性校验，plugin.lock pinned 全 40 位且 negotiation.resolved_commit 与 pinned 相等。
2. 摄取条数对账：awesome-hacking 85（分类表 81 + 仓库自链/徽章/Twitter/Facebook 4，no_scheme 2）、secskills 102 唯一键（README 自称 102 条中两条同 URL 上游重复合并 + mseep 徽章，no_scheme 17）、tongui 75（74 篇 + 来源主页）。独立 python 模拟器逐行复刻解析语义，与 Rust dry-run 报告逐数一致。
3. tongui 站点/仓库合并（计划 §4.2）：精选站点 31 篇与 Git 仓库 74 篇同文章合并为同一逻辑资源，精选条目 canonical_url 用站点 URL，Git 路径（正文权威来源）与 GitHub blob URL 落条目描述（双 source_ref）；未精选 43 篇 canonical_url 用 blob URL。映射键为文章 frontmatter 编号，由 scripts/sk2_tongui_catalog.py 从固定 Commit 原稿树 + 站点首页快照派生（配对不闭环即拒绝产出）。
4. 审核与幂等：全量条目初始 DISCOVERED，review --status APPROVED 翻转后才入检索面（APPROVED + 插件可见双闸）；二跑 skipped=85/103/75、changed=0；内容变更走 changed 计数不覆盖历史行、上游删除不物理删（sk2_source_ingest 测试锁定）。

## 三、已知取舍

1. tongui catalog_entry 来自 RIP 侧派生目录而非上游树直摄：上游整树仅 1 个绝对 URL（README 博客链接），文章链接全部为仓库内相对路径，精选站点 slug 与 Git 路径互不可推导；git 通道安装 + 上游树直摄实测 0 条目（诚实零，非报错）。详见 docs/phase6/SK2-SOURCES-NOTES.md §三。
2. upstream_missing 自动标记未实现：无第二快照触发场景；物理删已由测试锁定为不发生，自动翻转归 rip.knowledge.source.sync（P6-11）。
3. secskills skill_candidate 本快照计数 0：上游未打包 skills/ 目录（计划 §3.2 仓库结构核实值），候选以 catalog_entry 形态登记，Manifest 声明与 metadata_only 策略已就位。
4. web 视觉门禁（playwright）本机无 chromium 未运行；vitest 73 项 + browser-use 真实浏览器 E2E 截图 4 张（docs/phase6/browser-e2e/sk2-01 到 sk2-04）为浏览器级证据。
5. 推广/娱乐条目（mseep 徽章、secskills 娱乐 12 条等）按治理语义入库待审而非删除；本批为对账演示全量 APPROVED，治理面可随时经 knowledge_review 翻转。

## 四、回滚方式

git revert 本批提交即可：迁移零新增（011 为 SK-1 批次产物），三插件数据只存在于 /tmp/sk2.db 开发副本与 rip-data 安装目录，生产 platform*.db 与 integrations/rtis 全程未触碰（knowledge.db sha256 前缀 f8580feb 摄取前后一致）。
