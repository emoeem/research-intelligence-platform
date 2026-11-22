# RELEASE：WSW MVP 总账（2026-11-23）

> 阶段范围 = Web Security Workbench 轨全批次（[WSW-0](../rip3/batches/WSW-0-BATCH-REPORT.md) 至 WSW-8 + WSW-9 文档收口批）+ 并行支撑轨（RT-3 / MA-1..4 / CB-1 / DI-3-RT-4 / RT-5-6 / RT-7）。本文为 WSW MVP 阶段总账，逐条对账 [WEB-SECURITY-WORKBENCH-PLAN.md](../rip3/WEB-SECURITY-WORKBENCH-PLAN.md) §十四「交付完成定义」十条件（:787-802），证据全部取自各批次报告原文与本收口批实测。十条件逐条明细（状态/锚点/缺口）见 [phase0/WSW-9-RELEASE-CHECKLIST.md](../phase0/WSW-9-RELEASE-CHECKLIST.md)（绿 6 / 黄 4 / 红 0）。

## 1. 提交链与批次报告

### 1.1 WSW 轨前链（WSW-0 至 WSW-3，RT-3 之前在链）

| 批次 | commit | 报告 |
| --- | --- | --- |
| WSW-0 只读审计与 POC | 4e7b4c69（+ 评审批修订同批） | docs/rip3/batches/WSW-0-BATCH-REPORT.md |
| WSW-1 HTTP Message Foundation | 077a8dd4 | docs/rip3/wsw/WSW-1-BATCH-REPORT.md |
| WSW-2 Proxy / MITM Runtime | 738ebc14（中断点快照）+ a78f412d（收口）+ 5d146e24（迁移钉 1..24 CI 修复） | docs/rip3/batches/WSW-2-BATCH-REPORT.md |
| WSW-3 HTTP History + Repeater | 9aff134a（快照）+ 36638fe5（收口）+ a6aea6a4（实拍追加）+ 6aa95bb6（评审修订 F1-F5）+ 061cfdef（E2 回填） | docs/rip3/batches/WSW-3-BATCH-REPORT.md |

### 1.2 主链（b53562a6..8b79bd26，21 commit / 13 批组）

| 批次 | commit | 报告 |
| --- | --- | --- |
| RT-3 Tool Runtime Policy Resolver | b53562a6 | docs/phase0/RT-3-BATCH-REPORT.md |
| MA-1 Caller 身份与多密钥鉴权 | 0b950345 + 06d347f9 + 6233f127 + fdd593f6（收口 + CI 修复批 ×2 + 报告修订） | docs/rip3/MA-1-BATCH-REPORT.md |
| MA-2 并发派发引擎 | 76e0f559 + d18b7dca（CI 修复批） | docs/rip3/MA-2-BATCH-REPORT.md |
| CB-1 上下文折叠与装配稳定性 | ae23a9d8 | docs/rip3/CB-1-BATCH-REPORT.md |
| WSW-4 Web Fuzzer / Intruder | d664ceef | docs/rip3/wsw/WSW-4-BATCH-REPORT.md |
| MA-3 协作协议与知识供体面 | 86ea8530 + d4827a4a + 0f2922cb（收口 + 诊断观测 + CI 修复批） | docs/rip3/MA-3-BATCH-REPORT.md |
| MA-4 SSE 事件推送与四形态端到端 | f1be3f4e | docs/rip3/MA-4-BATCH-REPORT.md |
| DI-3/RT-4 运行时迁移与路由整合 | aafb87e9 | docs/phase0/DI-3-RT-4-BATCH-REPORT.md |
| WSW-5+6 WebSocket + Decoder/Comparer/Passive | d62e8b87 | docs/rip3/wsw/WSW-5-6-BATCH-REPORT.md |
| WSW-7 Plugin / Scanner Integration | 7a82804a + c962a8d7 + 2eaa19fe（收口 + CI 修复批 ×2） | docs/rip3/wsw/WSW-7-BATCH-REPORT.md |
| RT-5/6 构建与容器预算 | f8d38107 | docs/phase0/RT-5-6-BATCH-REPORT.md |
| WSW-8 Browser Evidence | dca07555 | docs/rip3/wsw/WSW-8-BATCH-REPORT.md |
| RT-7 CI 与发布矩阵核验 | 8b79bd26（selftest_wsw8 vendor 钉连带收口）+ **修固落点在工作树待 commit**（ci.yml timeout ×4 + ref 升级 + rust-toolchain.toml + selftest 四钉 + 两专档） | docs/phase0/RT-7-BATCH-REPORT.md + docs/phase0/RT-7-CI-RELEASE-VERIFICATION.md |
| 本收口批（WSW-9 对账 + RELEASE） | 见本批 commit | 本文 + docs/phase0/WSW-9-RELEASE-CHECKLIST.md |

CI：各批次 push 后 GitHub CI 全绿为硬门槛（MA-1/WSW-2/WSW-7 各有 CI 修复批实录在案）；链头 `8b79bd26` run **45066230102 completed success（8m35s）**（`gh run list` 2026-11-23 实测，EOF 重试词形第 2 次成功）；WSW-8 run 45061902514（9m32s）、RT-5/6 run 45052373746（6m56s）同列全绿。**RT-7 修固面（timeout/ref/工具链钉）未入链零 run 覆盖**——主会话 commit + push 后 watch 回填（CHECKLIST §二.5）。

## 2. 门禁终测（2026-11-23 WSW-9 收口批实测，静场串行）

| 门 | 结果 |
| --- | --- |
| `cargo test --offline --workspace --no-fail-fast`（CARGO_TARGET_DIR=/home/emo/rt3-target） | exit 0，**1337 passed / 0 failed / 2 ignored**（184 测试二进制 ok 行；基线与 RT-7 收口门三次实测零红） |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | exit 0 零告警 |
| `cargo deny check advisories` + `cargo deny check licenses` | 双 exit 0（advisories ok / licenses ok） |
| `cargo fmt --all -- --check` | exit 1 存量红 **120 文件 / 936 hunks** 精确持平零扩大（独立 chore 批待立，RT-7 D4） |
| `python3 -m unittest discover -s workers/python/tests` | Ran 237 tests，**OK（skipped 11）** |
| `npm --prefix web run build` / `npm --prefix web run test` | 双 exit 0；vitest **19 files / 126 passed**（本批复跑；WSW-3 时点 124，MA-4 runs-smoke additive 后 126） |
| apps/server/selftest_wsw4.sh（真实靶机） | exit 0，**PASS=34 FAIL=0** |
| apps/server/selftest_wsw56.sh（真实 WS 会话） | exit 0，**PASS=42 SKIP=0 FAIL=0** |
| apps/server/selftest_wsw7.sh（三 Adapter） | exit 0，**PASS=34 SKIP=4 FAIL=0**（SKIP = nuclei/zap 镜像面分面守卫——§5 坏消息 2/3） |
| apps/server/selftest_wsw8.sh（真实截图全链） | exit 0，**PASS=28 SKIP=0 FAIL=0** |
| selftest 全集其余（ma2/ma3/ma4/cb1/rt4/auth/phase7_agents） | 34/0、57/0、76/0、32/0、20/0、28/0、50/0（各批次报告实测，CHECKLIST §三.2） |
| release build（RT-7 实测） | `cargo build --release --offline --workspace` exit 0 / wall **1m46.129s** / release du **2.1G**；rp-wsw-browser 终钉 **a226fb56… PIN_MATCH**（stable 路径复现闭环） |
| containerfiles 五族 rt7-verify | build exit 0 ×5（构建期零出网）+ rmi 即删 + before/after 清点逐项一致（RT-7 专档 §3.2） |
| Cargo.lock | [[package]] **417** / reqwest lock=1 直接依赖=0（chromiumoxide 传递零 TLS——WSW-8 §4.3 双检词形） |
| 生产库指纹 | platform*.db 五库 mode=ro `PRAGMA user_version` = **1/1/1/0/0**（三批反证同值零触碰） |

脚本教训沿用：workspace 全量与集成面一律 `--test-threads=1` 或 `--no-fail-fast` 串行口径；browser 并发测试内置 BROWSER_TEST_LOCK 串行锁（WSW-8 坏消息 9）。

## 3. WSW MVP 交付总账（WSW-0 至 WSW-8 九批 + 支撑轨五批，详见各批次报告）

- **WSW-0**：只读审计与 POC（四审计 + ADR-0004 D1-D5 + 05 设计/威胁模型 + wsw-poc 六项实测 PASS 不转正 + vendor 167.6MiB 入库 + 评审批 F1-F10 全 applied）。
- **WSW-1**：wsw-message crate（四表示消息模型 / HTTP 解析序列化 / CAS / 023 BodyBlob 迁移 / 17 wire fixtures SHA 钉值 / BodyArtifactAdapter 桥接契约）。
- **WSW-2**：Proxy/MITM Runtime（hudsucker 0.25.0 Listener / WswAuthority IP SAN / Scope 闸闸序 + DNS rebinding 集成负例 / 024 拦截队列 Forward-Drop-Modified / CA 管理面 0600 / rp-wsw CLI + rp_server 七端点 + 两设置页 / e2e 16/16 + 六截图）。
- **WSW-3**：History + Repeater（025 四表 + capture→落库异步面 + 游标分页零重零漏 + sender.rs 原始 h1 发送引擎 + append-only 版本 + 脱敏展示投影 fail-closed + e2e 32/32 + 七截图 + 性能门全过）。
- **WSW-4**：Web Fuzzer/Intruder 引擎（028 两表九态/七态 + JobRunner 四闸 + 七类 Payload 位置 + Mutator/Processor 链 + wsw_message::diff + 真实靶机 selftest 34/0 + SIGKILL 恢复无重复派发）。
- **WSW-5+6**：WebSocket + Decoder/Comparer/Passive Audit（030 两表 + WS 帧捕获/重放 + rp-wsw ws/decode/compare + 七端点 + 真实 WS 会话 selftest 42/42 + 被动规则 14 条五类 + 候选态零 Finding 写面）。
- **WSW-7**：Plugin/Scanner Integration（Nuclei/ZAP/RTIS 三 Adapter 统一入口 scanner.rs 全经 Engine submit + zap manifest 孪生入册 + Resolver 期望表 30 份 + RTIS 真实库委派 poc get/use 零接触硬门 + 五态 evaluator 复用；ZAP 镜像面选项 B 如实登记）。
- **WSW-8**：Browser Evidence（chromiumoxide 0.9.1 vendor 基线跃迁 399→417 + rp-wsw-browser 子进程五级探针链 + 四模式真实截图 + DOMSnapshot + evidence 'screenshot' 六 ID 同链 + manifest 0.1.4 binary_sha256 终钉 + selftest 28/0）。
- **WSW-9（本批，纯文档）**：十条件逐条对账单 + 本 RELEASE 总账 + 用户文档缺口盘点 + docs/README 索引——代码面零改动（§5 回滚面）。
- **支撑轨**：RT-3 PolicyResolver 决策矩阵（144 格 totality + N1-N10 硬拒绝）；MA-1 Caller 鉴权（026 两表 + rp-admin 五子命令 + key 明文一次性输出）；MA-2 并发派发（027 四表 + 三帽 Rust 强制 + FamilyLimiter）；CB-1 转录折叠与装配稳定性（B1 两级触发 + B2 序确定性 + 缓存归因六桶）；MA-3 协作协议（029 task_claim CAS + 邮箱 + MCP 七工具）；MA-4 SSE 推送（close-delimited 三形态 cursor + 四形态端到端 76/76）；DI-3/RT-4 29 份 Manifest 迁移 + dispatch 决策点接线；RT-5/6 磁盘容器预算（A/B probe 34.08% 否决 + 14.5GB 回收 + 治理手册）；RT-7 CI 修固 + 发布矩阵核验（build 三面 + containerfiles 11 族 + selftest 四钉收口）。

## 4. PLAN §十四十条件逐条对账（判据原文 :793-802）

| 判据 | 状态 | 一句证据（全文见 CHECKLIST §一） |
| --- | --- | --- |
| ① Proxy / MITM 稳定捕获并按规则拦截 | 绿 | WSW-2 三通道实拍 + e2e 16/16 + WSW-3 capture 落库 e2e 32/32；缺口：fidelity 债 / 连接级校验+令牌跟踪项 / fail-open 设计语义 |
| ② History 保存检索查看完整请求响应 | 绿 | 025 四表 + 1100 行游标零重零漏 + p95 8.1ms + Body 按需 CAS；缺口：retention 执行器 / FTS5 缓议 |
| ③ Repeater 编辑重放保留历史并对比响应 | 绿 | sender.rs 引擎 + append-only 版本零覆盖 + 版本回退 e2e；对比响应面由 Comparer 承载（⑤） |
| ④ Fuzzer 构造执行取消分析 | 绿 | 028 两表 + 四闸 + 真实靶机 34/0 + SIGKILL 恢复；缺口：UI 四区页 / 浅层选择器 |
| ⑤ WS / Decoder / Comparer / Passive 独立入口 | 绿 | CLI 四面 + 七端点实跑 + 真实 WS 会话 42/42；缺口：UI 四页 / 解压三 op 裁剪 |
| ⑥ 插件和扫描器复用现有工具运行架构 | **黄** | 架构复用实证（RTIS 真实委派 + 期望表 30 份 + 五态复用 + 预算族帽）；缺口：ZAP 镜像未建成 + nuclei 钉值镜像不可回取 → 容器扫描数字面 V-9 SKIP |
| ⑦ Artifact / Evidence / Trace / ResearchTask / RTIS 闭环 | 绿 | 021 + 六 ID + WSW-8 六 ID 同链 + P5-9c RTIS Finding 闭环正例 900064 verified=1；零直写守恒 |
| ⑧ 三平台支持矩阵有实测结果 | **黄** | Linux 实测全绿（1337 + release build + 容器五族 + e2e + selftest 全集 + CI ubuntu 全绿）；**Windows/macOS 未实测如实登记**（Linux-only 本机 + CI ubuntu-latest） |
| ⑨ 安全 / 性能 / 自动化 / 浏览器验收通过 | **黄** | 自动化四类全绿 + 安全面逐项落位 + 浏览器三批 e2e；缺口：性能缺测三项（并发连接 / 大 Body 内存 / 长跑稳定）+ fmt 存量红持平 |
| ⑩ 用户文档 / 安装配置 / CA 管理 / 故障排查 / 数据保留齐备 | **黄** | INSTALL/BACKUP-RESTORE/MIGRATION/UPGRADE（P10-4）+ CLI-REFERENCE 等 9 份在案；缺口：WSW 用户手册 / CA 专档 / 故障排查专档 / 数据保留策略专档四类缺（随 UI 批 + retention 批同立） |

**计数：绿 6 / 黄 4 / 红 0**——四条黄全部为如实登记缺口（不冒充），零「无证据」红项。

## 5. 坏消息与回滚

### 5.1 坏消息（14 项全文见 [WSW-9-RELEASE-CHECKLIST.md](../phase0/WSW-9-RELEASE-CHECKLIST.md) §二，此处单列主干）

1. **Windows / macOS 未实测**：Linux-only 本机 + CI 四 job ubuntu-latest——零跨平台构建/测试/浏览器实测；探针词表登记面 WSW-8 §7.3，Windows 固定路径级待补。支持矩阵呈现为「Linux 实测 + Win/macOS 未实测」，不写跨平台完成。
2. **ZAP 镜像未建成**（WSW-7 D1 选项 B）：zap2docker-weekly 下架 + ghcr 迁移 + 624MB 大层传输坐死（21-30KB/s 三次实录）；containerfiles/rip-zap 代码面在案，镜像零建成零台账行；恢复路径固化（/tmp 续传 → OCI → digest 四处回钉 → selftest 点亮）。
3. **nuclei 钉值镜像不可回取**：9ff41ce9 不在本机、重建 4d0b47ef ≠ 钉值（mtime 非确定性）、当日产物不在任何 registry——Nuclei 容器 e2e 数字面随钉值演进批点亮。
4. **selftest 钉值随更链**：wsw4 28 钉 + wsw7 四钉已由 RT-7 按 owner 三件在案预授权词形收口（RT-7 §4 会签等效声明）——收口后 wsw7 34/4/0、wsw4 34/0；**修改仍在工作树待主会话 commit**。
5. **RT-7 CI 修固面零 run 覆盖**：timeout/ref/rust-toolchain.toml 在工作树未入链——action 升级后 CI 绿 run 待 push 后 watch 回填（EOF 重试词形已实测）。
6. **fmt 存量红 120 文件 / 936 hunks**：精确持平零扩大，独立 chore 批待立禁夹带。
7. **性能缺测三项**：并发连接峰值 / 大 Body 内存峰值 / 长时间运行内存稳定性——无压测夹具未跑不填数（PLAN :754）；capture 并发池/复用池零实现。
8. **UI 面缺口（后续 UI 批观察池，后端零改动即接）**：Fuzzer 四区页 / WS 帧 + Comparer + Decoder + Passive 四页 / Scanner 工作台 / Evidence 截图查看 / 拦截队列编辑器 / session 管理。
9. **WSW-2 安全遗留**：连接级 accept 校验缺口 + 本机访问令牌跟踪项开放；拦截超时 fail-open（设计语义）；tee trailer 丢弃；capture 超帽不落 CAS；被动面跳数追踪缺。
10. **WSW-3/4 遗留**：fidelity degraded_no_wire 债；retention 执行器（LRU/blob GC/cancel 滞留 sweeper）留 retention 批；SIGKILL 六态不收敛面（stale 显性化已落）；SENT 行不终态化；multipart/XML 浅层。
11. **WSW-5+6 遗留**：Gzip/Deflate/Brotli 解压 vendor 立项；JSON 规则加载面；SSE kind 扩展；HAR 导入导出未实现。
12. **WSW-8 遗留**：TIMED_OUT udir /tmp 残留（janitor 观察池）；RLIMIT_AS 128Gi 结构性钉值；known_limits 词形收窄补注待主会话裁量。
13. **工具链路径 sha 敏感性**：rust-toolchain.toml 在位时默认全量 release 构建必漂移终钉；恢复 = stable 路径复跑注册命令（9.24s 字节复现 a226fb56）。
14. **用户文档四类缺口**：WSW 用户手册 / CA 专档 / 故障排查专档 / 数据保留策略专档——缺口盘点见 CHECKLIST §一⑩，不硬凑。

### 5.2 回滚方式

- **本批（纯文档）**：`git checkout -- docs/phase0/WSW-9-RELEASE-CHECKLIST.md docs/releases/RELEASE_2026-11-23_WSW-MVP.md docs/README.md` + 删除两新档即整批撤销；代码面（crates/ migrations/ vendor/ Cargo.lock Cargo.toml web/ workers/ integrations/rtis/）零 diff 无回滚面（`git status --porcelain -- <上述路径>` = 0 实测）。
- **RT-7 工作树落点**：ci.yml ref 回 v4/@stable + 删 timeout 四行 / rust-toolchain.toml 删除即回浮动态 / selftest 两 sh checkout 回陈旧红已知态 / 两专档删档 / README 索引行删（RT-7 专档 §四.五词形全文适用）。
- **各批次回滚**：单 revert 面逐批在案（WSW-2 a78f412d 链 / WSW-3 五 commit 链 / WSW-4 d664ceef / WSW-5+6 d62e8b87 / WSW-7 7a82804a / WSW-8 dca07555——024/025/028/030 迁移随 revert 回退链尾钉值，各批次报告回滚节原文适用）。
- 生产库 / RTIS 目录 / 宿主软件零操作无生产回滚面；rt7-verify 验证镜像已 rmi 零残留。

### 5.3 RTIS 对账零回归声明

WSW 全轨 + 支撑轨全程零直写零行为变更：`git status --porcelain -- integrations/rtis/` 零输出（WSW-7 判据 3 + WSW-8 §2 ⑧ + RT-7 §2 判据 12 三批反证同值）；RTIS 引用唯一回填面 = poc use 既有接口（WSW-7 零 poc add 词形 grep 反证）；漏洞事实只读引用（poc 900008 metadata 锚投影正文零透出）；生产 platform*.db 五库指纹 1/1/1/0/0 恒定三批一致。

### 5.4 下一阶段入口

- **UI 收口批（观察池承接）**：WSW 四页（WS 帧/Comparer/Decoder/Passive）+ Fuzzer 四区页 + Scanner 工作台 + Evidence 截图查看 + 拦截队列编辑器——后端零改动即接（各批 §G/§6 词形交接面）；GUIDE 缺口（decoder/comparer/workbench/scan/plugin 零命中）随 UI 批补设计稿。
- **retention 批**：LRU 淘汰 + blob GC + cancel 滞留 sweeper + 数据保留策略专档同立。
- **镜像面批**：ZAP 镜像建成回钉（恢复路径在案）+ nuclei 钉值随更裁决 → 两容器扫描数字面自动点亮。
- **跨平台批**：Windows/macOS 构建与浏览器实测矩阵（探针词表 WSW-8 §7.3 为输入）。
- **fmt chore 批**：120 文件 / 936 hunks 收敛（独立评审窗口）。
- RIP3 主队列续段与 MA 轨扩展项按 HANDOVER-RIP3-CONTINUATION.md 第三节与 MULTI-AGENT-ACCESS-PLAN.md 观察池推进。
