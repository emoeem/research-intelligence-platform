# 更新说明 · 2026-11-12（SK-3 安全工具目录与静态审查工具）

> P6-11 四件工具全量落地：rip.knowledge.catalog.extract（T0 纯解析目录抽取）、rip.security.extension.audit（T0 纯静态零执行审查）、rip.knowledge.document.fetch（T1 白名单出网定点抓取）、rip.knowledge.source.sync（T1 受控同步 + 候选映射）。六类恶意 fixture 拒绝矩阵逐项断言，私网/浮动分支/无许可证/安装钩子/越权 Shell/危险 MCP Schema 均有静态拒绝或高危告警测试。document.fetch 真实出网冒烟抓出 chunked 解帧失真 bug 并已修复（哈希与 curl 独立互核一致）。

| 项         | 值                                                                                                                                                    |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| 阶段交付   | git 本批提交（见 git log，本行短哈希由回填提交落定）                                                                                                    |
| 数据库迁移 | 012_knowledge_document_fetch.sql（新增 knowledge_document_fetch 抓取审计表，ok/rejected 双态只追加，唯一写面 document_fetch.rs::record_fetch；只在开发库副本与测试内存库执行，platform*.db 与 integrations/rtis 零触碰） |
| 自动化测试 | `cargo test --offline --workspace` 57 套件 ok，379 passed / 4 failed（唯一失败 podman_host_egress 4 项：本机无 podman，panic 原文 podman ps spawn NotFound，与 SK-1/SK-2 批次预期例外同口径）；`python3 -m unittest discover -s workers/python/tests` 211 tests OK（128 既有 + catalog 37 + audit 46） |
| 静态检查   | `cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0 零告警                                                                        |

## 一、面向使用者的变化

1. 工具登记面 +4（manifests/tools/ 双份 yaml+json，rp-tool validate/register 全过）：catalog.extract 从 Markdown/HTML 目录文档抽条目（name/描述/规范 URL/分类路径/去重键/置信度，分页截断）；extension.audit 对 Skill/MCP/Tool Manifest/仓库做纯静态审查（findings + risk_level + review_status + required_permissions + license_status + supply_chain_notes + 固定 disclaimer）。
2. rp-plugin 新增 sync 子命令：受控 update-check + ingest（dry-run/幂等键/文件与字节与耗时限额/重试 attempts/部分失败机器可读），--map-candidates 把 APPROVED skill_candidate 映射为 Resource 候选（CANDIDATE/ACTIVE，resource-engine 受控写面）+ 审计留痕；幂等键台账落 state/sync-keys.jsonl（非数据库侧车）。
3. 新二进制 rp-knowledge-fetch：approved 闸（只接受 Registry 已批准 canonical URL）+ 域名 allowlist + 私网全闸（loopback/RFC1918/link-local 含 169.254.169.254 metadata/CGNAT/ULA）+ 重定向逐跳复核 + 大小/MIME/超时上限 + 内容哈希与抓取时间落 012 审计表；抓取审计拒绝行同样落库。
4. 审计动作词表 +2：plugin.knowledge.sync、plugin.knowledge.map_candidates（audit-engine plugin_actions 唯一出口）。

## 二、实现与根因

1. 复用优先（用户硬性规则）：catalog.extract 直接 import parsing_worker 既有 extract_markdown_links/_link_scheme/extract_frontmatter（SK-1 落地面零重写）；document.fetch 复用 tool-engine adapter/egress_proxy.rs 既有 host_from_url/EgressProxy 白名单代理（策略面双重防御），新增面只补 egress_proxy 没有的私网分类/DNS 全址复核/重定向逐跳/限额/approved 闸；source.sync 复用 P6-4 source_fetch verify/fetch_into 与 SK-1 ingest 受控 API；候选映射复用 resource-engine 既有 resource 表语义（lifecycle=CANDIDATE）。
2. 闸序裁定：document.fetch 单跳复核私网先于 allowlist（内网目标无论 allowlist 形态先拒）；source.sync pin 前置闸先于幂等键与限额（浮动分支 fail-fast 不触源），kind=commit 必须全 40 位（比 registry rule 5 的 7-64 短哈希更严，完整 Commit 语义）。
3. extension.audit 零执行能力自证：AST 断言锁定 worker 源码 import 面（只允许 __future__/json/pathlib/re/sys/worker_runtime，subprocess/socket/urllib/os 等 15 类禁止）与危险调用点（eval/exec/compile/__import__/open/system/popen/spawn 族），审查 fixture 时不存在可执行被审查物的代码路径。
4. 冒烟实测抓出并修复真 bug：example.com chunked 响应下初版读取面把分帧记进正文（哈希失真 8dade617...，577+12=589 字节）；read_http_response 重写为 Content-Length 精确取/chunked 解帧/close 定界三种形态后，落库哈希 25ddf2c8...（577B text/html）与 curl 同头独立 sha256sum 逐字节一致。
5. 登记面回归同步：context-broker 登记面断言 20 → 24（acceptance_no_stuffing/catalog_sources），no_stuffing 预算 8000 → 8200 tokens（四工具卡片增量 ~100，no-stuffing 不变式不变）；database 迁移链发现断言 011 → 012、链尾/user_version 011 → 012、lifecycle_full_path 重放距离 2 → 3（断言按链尾距离演进的自然口径）。

## 三、已知取舍

1. https 传输面未实现（workspace 无 TLS 依赖栈，Cargo.lock 无 rustls/reqwest/openssl）：rip.knowledge.document.fetch 本批仅 http 明文（egress ports 取 80），https 按 scheme_unsupported 结构化拒绝不静默降级；TLS 依赖的 vendor/离线构建评估归独立批次，tongui 正文等 https-only 源的定点抓取待解锁。
2. catalog.extract 的 resource_kind/confidence 为确定性启发（URL 形状 + 证据完整度公式），非内容判断；rust 侧 ingest meta.resource_kind 恒 link，worker 侧细分属登记口径差异（不改 knowledge_parse.rs 约束下如实登记，收敛归 Phase 7）。
3. source.sync 对 local_dir 来源无 --package 时按 source.path 相对 cwd 取包（Plugin Runtime 既有语义未改动），冒烟以 --package 显式 staging 规避；max_seconds 不可中断摄取（同步调用），超限如实计入部分失败报告。
4. fetch 冒烟的 APPROVED 条目为一次性 dev 库副本中人工登记的 smoke 条目（http://example.com/），只用于 approved 闸与哈希落库冒烟，不入仓不进生产库。
5. Web Tool Center 展示（四工具卡片、audit findings 视图）不在本批，归 P6-8 后续迭代；SESSION-HANDOFF.md 未更新（与 SK-1/SK-2 同口径）。
6. podman_host_egress 4 项未在本环境验证（本机无 podman 二进制）：按批次预期口径记为待验证，需有 podman 的宿主复测。

## 四、回滚方式

git revert 本批提交即可：012 迁移为纯追加新表（回滚后 012 表残留在曾执行过的开发副本中无害，执行器按 user_version 跳过），四工具 Manifest 与新 worker/模块为独立文件无既有行为耦合；生产 platform*.db、integrations/rtis 全程零写（012 只进开发副本与测试内存库）。
