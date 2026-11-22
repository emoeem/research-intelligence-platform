# WSW-9 + RELEASE 收口对账单 · PLAN §十四十条件逐条核验（跨平台 / 性能 / 安全 / 故障恢复 / 用户文档）

- 批次：WSW-9（[WEB-SECURITY-WORKBENCH-PLAN.md](../rip3/WEB-SECURITY-WORKBENCH-PLAN.md) §九 :611「Cross-platform / Reliability Acceptance」+ §十四 :787-802 交付完成定义十条件；纯文档收口批——代码面零改动，对账不冒充）
- 日期：2026-11-23
- 对账基线：HEAD `8b79bd26`（RT-7 连带收口）+ RT-7 修固落点（ci.yml / rust-toolchain.toml / selftest 四钉）在工作树待主会话 commit 入库；Cargo.lock 417 / reqwest lock=1 直接依赖=0；migrations 30
- 证据源：WSW-0 至 WSW-8 批次报告 + RT-3/MA-1..4/CB-1/DI-3-RT-4/RT-5-6/RT-7 批次报告与专档 + 本收口批 workspace 全量复跑实测（命令原文见本文 §四）
- 状态口径：绿 = 证据锚点齐全零缺口；黄 = 已交付面有实测证据但存在如实登记缺口（缺口逐条列明）；红 = 无证据或证据矛盾。禁模拟冒充：未实测写未实测、未建成写未建成。

---

## 一、十条件逐条对账表（PLAN §十四 :793-802 原文判据）

| # | 条件（原文词形） | 状态 | 证据锚点 | 缺口说明（坏消息单列） |
|---|---|---|---|---|
| ① | Proxy / MITM 能稳定捕获 HTTP / HTTPS 并按规则拦截 | **绿** | [WSW-2-BATCH-REPORT.md](../rip3/batches/WSW-2-BATCH-REPORT.md) E 节（三通道实拍：明文 200 / CONNECT MITM marker 200+Verify return 0 / 越界三例 403）+ I 节 e2e 16/16 + A1/A3/A4（Listener 六态 / WswAuthority IP SAN / Scope 闸闸序 / 024 拦截队列 Forward-Drop-Modified 三动作）；[WSW-3-BATCH-REPORT.md](../rip3/batches/WSW-3-BATCH-REPORT.md) A.2（capture→落库异步面 mpsc 2048 + 批量事务）+ E4 32/32（种子 1101 经代理全落库）+ F 节（代理增量 +1.22ms <50ms 门 PASS） | capture_fidelity='degraded_no_wire' 债保留（请求头区无原始 wire 字节——WSW-3 G1）；连接级 accept 校验缺口 + 本机访问令牌跟踪项开放（WSW-2 G1/WSW-3 G6）；拦截超时 fail-open 到透传为设计语义（PLAN :536，WSW-2 G2）；被动转发面跳数追踪缺（WSW-3 G4） |
| ② | History 能保存、检索和查看完整请求响应 | **绿** | WSW-3 A.1/A.4（025 四表 + History API 游标分页 + 过滤 + Body 按需 CAS 8MiB 帽）+ E2 curl 实测（1100 行零重零漏 ZERO-DUP-FULL-COVERAGE / filter_cursor_mismatch 400 / limit 帽 400 / 末页空 cursor / search_text 凭据零泄漏 leaks:0）+ E4 e2e（首屏 457ms / 详情 21ms / 六视图 / Body 按需恰一次）+ F 节 p95 8.1ms <300ms 门 | retention 执行器遗留（超帽 LRU 淘汰 + blob GC 留 retention 批——WSW-3 G2）；FTS5 显式缓议（search_text LIKE p95 8.1ms 达标，升级预案保持缓议——WSW-3 G3） |
| ③ | Repeater 能编辑、重放、保留历史并对比响应 | **绿** | WSW-3 A.3（sender.rs 发送引擎：原始 h1 字节写 / 重定向双源 min 帽 / 三超时 / 六态）+ E3（append-only 版本行零覆盖实证：PUT ×3 → 4 版本行 + PATCH 405）+ E4（Send to Repeater origin=from_history / 编辑发送 method-path-body 改写 / 版本回退 origin=restore）+ 修订批 F1/F2（policy 帽消费接线 + Location 相对引用 RFC 3986 归一化 +6 测） | 「对比响应」面 = Comparer 承载（⑤，CLI/API 双入口已交付，UI 页未建）；cancel SIGKILL 滞留行推进留 retention 批（F3 显性化，stale 只读检测已落地——WSW-3 G15） |
| ④ | Fuzzer 能构造受控变体、执行、取消和分析结果 | **绿** | [WSW-4-BATCH-REPORT.md](../rip3/wsw/WSW-4-BATCH-REPORT.md) A.1（028 两表九态/七态 + JobRunner 四闸 + 七类选择器 + Mutator/Processor 链 + wsw_message::diff 原语）+ B.8 selftest_wsw4.sh **PASS=34 FAIL=0**（真实靶机：marker echo 全链 / budget 截断 / scope 全拒零发送 / 429 分类 / SIGTERM CANCELLED / SIGKILL 重启恢复无重复派发 / 吞吐 545.5 case/s / rp_server 四端点）+ C 节判据 12/12 | UI 四区页面未建（WSW-4 G.1 移交 UI 批——§二.8）；multipart/XML 选择器浅层（D10）；SENT 行不终态化（E.6）；SIGKILL 兜底滞留缺口同 F3 同源（E.5） |
| ⑤ | WebSocket、Decoder、Comparer 和 Passive Audit 具有可用的独立入口 | **绿** | [WSW-5-6-BATCH-REPORT.md](../rip3/wsw/WSW-5-6-BATCH-REPORT.md) D 判据 7（`rp-wsw ws connect/replay` / `decode` / `compare` 实跑 exit 0 + scope 拒 exit 3 + 七端点 Bearer 401 负例 + 游标 2010 帧行零重零漏）+ C 节真实 WS 会话 42/42（握手 101 + 8 帧 + 1000 帧对账 + 被动命中 9 行对账 + 性能门代理增量 2ms / p95 11/10/8ms） | UI 四页（WS 帧展示 / Comparer / Decoder / Passive）未建（WSW-5+6 D8 + §G 缺口清单移交 UI 批——§二.8）；Gzip/Deflate/Brotli 解压词表外（12/17 op 落地——D5 裁剪）；被动规则 JSON 文件加载面不进（观察池）；SSE additive kind 扩展不进（D9） |
| ⑥ | 插件和扫描器复用现有工具运行架构 | **黄** | [WSW-7-BATCH-REPORT.md](../rip3/wsw/WSW-7-BATCH-REPORT.md) 判据 3（RTIS 真实库委派：poc 900008 metadata 回读 + use_recorded 回填 hit_count 5→8 + 零 poc add 词形反证 + integrations/rtis 零 diff）+ 判据 4/6/7/8（rip.security.zap manifest 孪生 validate/register + Resolver 期望表 30 份 + 预算双闸族帽 + scope/egress 延续 + 五态 evaluator 复用零第二判定）+ 判据 8/9（独立入口 scan --help 四连 exit 0 + POST /api/wsw/scan 401/400/504 词形）；scanner.rs 全经 Engine submit + Adapter 链零 `Command::new("podman")` 直连 | **黄根因**：ZAP 镜像未建成（D1 选项 B——zap2docker-weekly 下架 + ghcr 迁移 + 624MB 大层传输坐死，恢复路径在案——§二.2）且 nuclei 钉值镜像不可回取（本机重建 digest 4d0b47ef ≠ 钉值 9ff41ce9，mtime 非确定性——§二.3）→ Nuclei/ZAP 容器真实扫描数字面按 V-9 分面 SKIP（判据 1 SKIP / 判据 2 选项 B，代码面 e2e 与 selftest 断言写就镜像在位即跑）；RTIS 委派与登记面/预算面/五态面架构复用实测全绿 |
| ⑦ | Artifact、Evidence、Trace、ResearchTask 和 RTIS 引用闭环真实可用 | **绿** | [P5-7-BATCH-REPORT.md](../rip3/batches/P5-7-BATCH-REPORT.md)（021 research_evidence SHA256 append-only + Trace 六 ID submit_correlated 贯通）+ [P5-9c-BATCH-REPORT.md](../rip3/batches/P5-9c-BATCH-REPORT.md) E3/E4（RTIS Finding 闭环正例：rt.py poc add 900064 → INCONCLUSIVE → 人工审证 CONFIRMED → finding-link → verified=1）+ [WSW-8-BATCH-REPORT.md](../rip3/wsw/WSW-8-BATCH-REPORT.md) 判据 3（evidence 'screenshot' 六 ID 同链：content_hash=artifact.sha256 + storage_ref=artifact uri + trace_id 三列同值 + v_trace_chain 汇出）+ 判据 4（派发唯一路径 Engine submit_correlated + NativeRustCall）+ WSW-7 判据 3（RTIS 零接触负例硬门：零 poc add 词形 + poc use 既有接口唯一回填面） | 零直写零行为变更守恒：全轨 integrations/rtis 零 diff + 生产 platform*.db 五库指纹 1/1/1/0/0 恒定（WSW-7/WSW-8/RT-7 三批反证同值）；agent_run/artifact 两锚在验证链无实体沿用 P5-7 G2 登记（Evidence 档承载 Artifact 语义） |
| ⑧ | Linux、Windows、macOS 支持矩阵有实测结果 | **黄** | **Linux 实测全绿**：workspace 全量 1337 passed / 0 failed（RT-7 §2 判据 11 + 本批复跑 §四）+ release build exit 0 / wall 1m46s / du 2.1G（RT-7 §2 判据 7）+ 容器五族 rt7-verify build exit 0 ×5 构建期零出网（RT-7 专档 §3.2）+ WSW-2/3 浏览器 e2e 16/16 + 32/32（真实 Chromium）+ WSW-8 四模式真实截图 8 测 + selftest 全集（§三表）+ CI 四 job ubuntu 全绿 run 45066230102（8m35s，`gh run list` 实测）。**Windows / macOS：未实测，如实登记不冒充**（本机 Linux-only；CI 矩阵 ubuntu-latest 四 job——ci.yml :30/:64/:89/:115） | Windows/macOS 零实测零构建（rustup 三平台目标为上游支持声明面，本仓库未逐平台验证）；Chromium 探针链跨平台路径词表 = WSW-8 §7.3 登记面（Windows 固定路径级词表待补 + chromiumoxide detection.rs 对照面在案）；hudsucker/rustls/chromiumoxide 三主依赖的平台支持为上游 crate 声明面，未在本机之外复验——全部列入 §二.1 |
| ⑨ | 安全测试、性能测试、自动化测试和真实浏览器验收通过 | **黄** | **自动化测试全绿**：workspace 1337 passed / 0 failed / 2 ignored（Rust）+ python unittest 237 tests OK（skipped 11）+ web vitest 19 files / 126 passed + build exit 0（本批复跑）+ selftest 全集（§三表 12 套数字）。**安全测试面**：PLAN §11.4 逐项落位——DNS rebinding（scope_gate 集成级负例 WSW-2 G11 修复后真实执行全链）/ 重定向越界 403 / IPv4-IPv6 SAN 四例逐位断言 / 私网-回环-云元数据 403 / 非 loopback 拒绑 exit 1 / CA 私钥 0600 no-clobber / 凭据脱敏（redact fail-closed + leaks:0）/ 超大 Body 8MiB 帽 + 413 / 队列耗尽 EXPIRED 终态 / 预算四闸 + FamilyLimiter / fuzz scope 全拒零发送。**真实浏览器验收**：wsw-2 e2e 16/16 + wsw-3 e2e 32/32 + 7 截图 + wsw-8 真实截图四模式（PNG magic + sha256 对账）。**性能测试已测面全过**：代理增量 +0.921/+1.22/+2ms <50ms / History API p95 8.1ms <300ms / WS 三端点 p95 11/10/8ms / 落库吞吐 10091 rows/s / fuzz 545.5 case/s / capture 冷 1372ms 热 1188-1218ms（全部命令回显禁预填） | **黄根因（缺测三项如实）**：并发连接峰值 / 大 Body 内存峰值 / 长时间运行内存稳定性三项 §11.6 基线缺测（无压测夹具，未跑即不填数——WSW-2 F 节 / WSW-3 F 节明文登记，PLAN :754 禁预填）；`cargo fmt --all -- --check` 全仓存量红 120 文件 / 936 hunks 精确持平零扩大（独立 chore 批待立——RT-7 D4）；PLAN §11.4 压缩炸弹面不适用如实登记（Gzip/Deflate/Brotli 解压未实现——WSW-5+6 E.1，无解压面即无炸弹膨胀路径，登记非豁免） |
| ⑩ | 用户文档、安装配置、CA 管理、故障排查和数据保留策略齐备 | **黄** | 已有面（docs/ 盘点）：README.md（根）+ [guides/INSTALL.md](../guides/INSTALL.md)（fresh clone 演练实跑，P10-4）+ [guides/BACKUP-RESTORE.md](../guides/BACKUP-RESTORE.md)（rp-admin backup/verify/restore/reten-archive 四命令）+ [guides/MIGRATION.md](../guides/MIGRATION.md) + [guides/UPGRADE.md](../guides/UPGRADE.md)（P10-4 四文档）+ [guides/ADDING-A-TOOL.md](../guides/ADDING-A-TOOL.md) + [guides/CLI-REFERENCE.md](../guides/CLI-REFERENCE.md)（rp-* 11 bin 帮助面）+ [guides/EXTERNAL-AI-QUICKSTART.md](../guides/EXTERNAL-AI-QUICKSTART.md) + [phase9/REST-API-CONTRACT.md](../phase9/REST-API-CONTRACT.md)（selftest 五重机器对账）+ [ui-ux/AI-IMPLEMENTATION-GUIDE.md](../ui-ux/AI-IMPLEMENTATION-GUIDE.md)（743 行，AI 实现规范）+ WSW 轨 12 份 ADR/任务书/批次报告 + 本对账单。**CA 管理面**：UI 页内说明在案（WswCertPage 指纹/有效期/导出/重生成/各平台手动信任说明——WSW-2 A6/I 节）+ curl/openssl 实拍证据（WSW-2 E 节） | **黄根因（缺口如实盘点，不硬凑）**：①WSW 用户手册缺（Proxy/History/Repeater/Fuzzer/WS/Decoder/Comparer/Passive/Scanner/Browser 十工具操作手册零专文档——UI 四页未建，手册无界面可附图，随 UI 批同立）；②CA 管理独立专档缺（现散在 WswCertPage 页内 + WSW-2/WSW-3 报告，无一页式 CA 生命周期文档：生成/信任/轮换/撤销/卸载）；③故障排查专档缺（listener 僵尸 pid 判定 / send 滞留 stale 检测 / 030 水位守卫 503 / rp_wsw_bin_missing 504 等故障词形散在批次报告，无汇总排查页）；④数据保留策略专档缺（025 只落查询面 `_wsw_history_stats`，retention 执行器 LRU 淘汰 + blob GC 未实现——WSW-3 G2，策略文档随执行器批同立才不冒充）。安装配置面四文档 P10-4 在案为绿 |

**计数：绿 6 / 黄 4 / 红 0。** 红条件零条——十条件无「无证据」项；四条黄的根因全部为如实登记面（平台矩阵未实测 / ZAP-nuclei 镜像面 / 性能缺测三项 + fmt 存量 / 四类用户专文档缺），不是冒充绿。

---

## 二、坏消息面单列（全部如实，禁伪装收口）

1. **Windows / macOS 未实测**：本机 Linux-only（CachyOS，rustc 1.99.1 x86_64-linux）；CI 四 job runs-on ubuntu-latest（ci.yml :30/:64/:89/:115 实读）——Windows/macOS 零构建零测试零浏览器实测。上游面登记：hudsucker 0.25.0 / rustls 0.23.46 / chromiumoxide 0.9.1 平台支持为 crate 声明面；chromiumoxide Windows/macOS 探测路径词表在 vendor（detection.rs）+ WSW-8 §7.3 对照表；browser.rs probe_chromium Windows 固定路径级词表待补（WSW-8 §7.3 登记项）。支持矩阵一栏在 RELEASE 对账按「Linux 实测 + Win/macOS 未实测」呈现，不写「跨平台支持完成」。
2. **ZAP 镜像未建成**（WSW-7 D1 选项 B 触发）：任务书原源 docker.io/owasp/zap2docker-weekly 已下架（hub API object not found + 401 双证）；官方迁移源 ghcr.io/zaproxy/zaproxy manifest 可读但 624MB 大层传输坐死（三次尝试 21-30KB/s 反复停滞，curl --speed-limit abort 循环实录，估算 8h+ 不可达批次窗口）。containerfiles/rip-zap/ 代码面在案（FROM digest 64 零占位 + wrapper.sh D-TC-03 帧协议），镜像零建成 → rip-zap 无台账行、判据 2 按选项 B 验收、`zap.sh -help` 代理注入键实拍未测。恢复路径固化在 WSW-7 §坏消息 1（/tmp/zap_fetch_blobs.sh 可续传 → OCI layout → podman pull → digest 四处回钉 → selftest 数字面自动点亮）。
3. **nuclei 钉值镜像不可回取**（WSW-7 坏消息 5）：钉值 digest `9ff41ce9…` 不在本机镜像库，本机重建实测 `4d0b47ef…` ≠ 钉值（COPY 层 tar 含 mtime，跨 checkout 重建必然异 digest）；当日构建产物不存在于任何 registry。回取唯一路径 = 找回当日镜像（不可达）或 nuclei manifest/provider 五处钉值随更（WSW-5+6/WSW-7/WSW-8 三批均声明禁改面）——Nuclei 容器 e2e 数字面随钉值演进批点亮，代码面断言写就。
4. **selftest 钉值随更遗留链与收口态**：wsw4 :39 `user_version=28` 陈旧钉（029 入链起漂移）+ wsw7 四钉（Cargo.lock diff 3 行 / [[package]] 399 / 跨轨 vendor diff 面）随 WSW-8 417 基线跃迁而红（WSW-8 判据 12 首跑 4 FAIL 实录）→ **RT-7 批已按 owner 三件在案预授权词形收口**（RT-7 §4 会签等效声明：wsw7 :292/:293 期望 3→0、:294 399→417、wsw4 :39 28→30；selftest_wsw8 vendor 钉随 8b79bd26 收口）——收口后 selftest_wsw7 34/4/0、wsw4 34/0 全绿。**登记点**：RT-7 四钉修改仍在工作树未 commit（主会话收口面），RELEASE 前的 git 实态 = `M selftest_wsw4.sh / M selftest_wsw7.sh / M ci.yml / ?? rust-toolchain.toml / ?? RT-7 三文档`。
5. **RT-7 CI 修固面未被 CI run 覆盖**：timeout ×4 + dtolnay@1.99.1 + checkout@v5/setup-python@v6/setup-node@v5 + rust-toolchain.toml 全部在工作树，零 push 零 run——「action ref 升级后四 job 绿 run」无实录（RT-7 坏消息 4 / 观察池⑦）。主会话 commit + push 后 watch 回填（EOF 重试词形已实测成立：`gh run list` 首击 EOF retry 第 2 次成功——RT-7 §5 表尾行）。
6. **fmt 存量红 120 文件 / 936 hunks**：`cargo fmt --all -- --check` exit 1 为 HEAD 基线既有态（WSW-2 K8 / WSW-5+6 E.5 / RT-7 判据 11 三批实测精确持平零扩大）；独立 chore 批待立，禁夹带（RT-5/6 D3 + RT-7 §7.1 词形）。
7. **性能缺测三项**：并发连接峰值 / 大 Body 内存峰值 / 长时间运行内存稳定性（PLAN §11.6 后三项）——WSW-2/WSW-3 F 节如实登记缺测，无压测夹具未跑即不填数；capture 并发池 / 浏览器实例复用池零实现（WSW-8 §7.2 观察池）；长跑内存稳定性随 retention/压测夹具批补测。
8. **UI 面缺口（移交后续 UI 批观察池，后端零改动即接）**：Fuzzer 四区页面（WSW-4 G.1）/ WS 帧展示 + Comparer + Decoder + Passive 四页（WSW-5+6 §G）/ Scanner 工作台 + Plugin Center 投影卡片（WSW-7 §6.2）/ Evidence 页截图查看 UI（WSW-8 坏消息 1）/ 拦截队列结构化编辑器（WSW-2 H 节最小 host/method/path 匹配）/ session 管理 UI（WSW-2 G7）。PLAN §11.5 十二项中 1/4/5/6/10/11/12 已实测，2（Intercept On/Off UI 编辑器）/7（Fuzzer UI）/8（Comparer UI）/9（WS 帧 UI）UI 面未建——后端与 e2e 断言面全过。
9. **WSW-2 安全遗留（跟踪项开放）**：连接级 accept 校验缺口（hudsucker 0.25.0 无逐连接钩子——每请求 fail-closed 403 + bind_guard 前置承载）+ 本机访问令牌方案（WSW-3 G6 跟踪项保持开放）；拦截超时 fail-open 到透传（PLAN :536 设计语义，若需 fail-closed 需裁决）；tee trailer 丢弃；capture 超帽不落 CAS；被动转发面跳数追踪缺（Repeater 面已收口 F1，被动面保留）。
10. **WSW-3/4 遗留**：fidelity 债 degraded_no_wire（逐字节重发仅 source=repeater 面）；retention 执行器（LRU 淘汰 + blob GC + cancel SIGKILL 滞留行 sweeper）留 retention 批；SIGKILL 兜底六态不收敛面（F3 stale 显性化已落）；SENT 行不终态化；multipart/XML 浅层选择器；selftest_wsw4 固定端口 18844 冲突重跑即可。
11. **WSW-5+6 遗留**：Gzip/Deflate/Brotli 解压 vendor 立项（12/17 op 落地）；JSON 规则文件加载面；SSE additive kind 扩展；HAR 导入导出未实现（PLAN :587-594，exchange.rs Import 词形已备）；帧级实时推送（MA-4 D1 server 三否维持）。
12. **WSW-8 遗留**：TIMED_OUT 路径一次性 udir 残留 /tmp（前缀 `rp-wsw-browser-udir-` 可 janitor 清扫——观察池）；RLIMIT_AS 与 Chromium 结构性不兼容（128Gi VA 上限钉值 + RLIMIT_CPU/看门狗实质承载）；manifest known_limits「release 重建即漂移」词形应收窄为「跨路径/跨版本重建漂移」（RT-7 坏消息 1 归因闭环：同路径字节可复现 a226fb56——补注属 WSW-8 产物面，主会话裁量）。
13. **工具链路径 sha 敏感性（运维协议）**：rust-toolchain.toml（1.99.1 版本化目录）在位时默认全量 release 构建必漂移终钉（211 crate 部分重建）；终钉恢复 = `RUSTUP_TOOLCHAIN=stable-x86_64-unknown-linux-gnu cargo build --offline --release -p wsw-runtime --bin rp-wsw-browser`（9.24s 字节级复现 a226fb56——RT-7 §2 判据 8）。
14. **用户文档缺口**（⑩ 黄根因）：WSW 用户手册 / CA 管理专档 / 故障排查专档 / 数据保留策略专档四类缺——缺口明细见 §三盘点，缺口登记不硬凑，后续批按 UI 面 + retention 执行器落地节奏同立。

---

## 三、selftest 与门禁数字台账（对账素材源汇总，全部批次报告实测原值）

### 3.1 四类自动化测试（条件 ⑨ 承载面）

| 门 | 数字 | 来源锚点 |
|---|---|---|
| Rust workspace 全量 | **1337 passed / 0 failed / 2 ignored**（185 测试二进制，EXIT=0） | RT-7 §2 判据 11 + 本批 §四复跑 |
| cargo clippy --workspace --all-targets -D warnings | exit 0 零告警 | RT-7 §2 判据 11 |
| cargo deny advisories + licenses | 双 exit 0（`advisories ok` / `licenses ok`） | RT-7 §2 判据 11 |
| cargo fmt --all --check | 存量红 120 文件 / 936 hunks 精确持平（独立 chore 批待立） | RT-7 §2 判据 11 |
| Python workers unittest | **237 tests OK（skipped 11）**（本批复跑；PHASE5 时点 232 passed + 后续批 additive 演进） | 本批 §四 + ci.yml python-workers job 全绿延续 |
| Web build + vitest | build exit 0 + **19 files / 126 passed**（本批复跑；WSW-3 E.1 时点 124，MA-4 runs-smoke additive 后 126） | 本批 §四 + web npm job CI 全绿 |
| cargo build --release --offline --workspace | exit 0 / wall 1m46.129s / release du 2.1G | RT-7 §2 判据 7 |
| rp-wsw-browser 终钉 | a226fb56… PIN_MATCH（manifest 孪生 binary_sha256 一致） | RT-7 §2 判据 8 |
| Cargo.lock | [[package]] 417 / reqwest lock=1 直接依赖=0（chromiumoxide 传递零 TLS） | WSW-8 §4.3 双检词形 |

### 3.2 selftest 全集（收口态数字，逐脚本 exit 0；WSW 四套为本批复跑实测，其余取自各批次报告）

| 脚本 | 数字 | 来源 |
|---|---|---|
| selftest_wsw4.sh（真实靶机） | **PASS=34 FAIL=0** | 本批复跑 + RT-7 §2 判据 5（钉收口后）+ WSW-4 B.8 |
| selftest_wsw56.sh（真实 WS 会话） | **PASS=42 SKIP=0 FAIL=0** | 本批复跑 + RT-7 §2 判据 6 + WSW-5+6 §C |
| selftest_wsw7.sh（三 Adapter） | **PASS=34 SKIP=4 FAIL=0** | 本批复跑 + RT-7 §2 判据 4（钉收口后；SKIP = nuclei/zap 镜像面分面守卫） |
| selftest_wsw8.sh（真实截图全链） | **PASS=28 SKIP=0 FAIL=0** | 本批复跑 + WSW-8 §2 ⑦ |
| selftest_ma4.sh（SSE 四形态端到端） | PASS=76 FAIL=0 SKIP=0 | MA-4 报告 run8/run9 |
| selftest_ma3.sh（协作协议三硬验收） | PASS=57 FAIL=0 | MA-3 §二.16 |
| selftest_ma2.sh（并发派发 CLI） | PASS=34 FAIL=0 | MA-2 §二.11 |
| selftest_cb1.sh（折叠实测） | PASS=32 FAIL=0 | CB-1 §二 |
| selftest_rt4.sh（29 份迁移守卫） | PASS=20 SKIP=0 FAIL=0 | DI-3/RT-4 §1 表 |
| selftest_auth.sh（鉴权 28 例） | PASS=28 FAIL=0 | MA-2 §二.8（11 既有 + MA-1 #8 块） |
| selftest_phase7_agents.sh | PASS=50 FAIL=0 | MA-4 §一（七套零改动回归行） |
| selftest_wsw8 vendor 钉（8b79bd26 收口） | git status ?? 形恒 0 → 18 目录在库存在性断言 | RT-7 连带收口 commit 词形 |

### 3.3 真实浏览器验收与实拍证据（条件 ⑨ 浏览器面）

| 面 | 证据 |
|---|---|
| WSW-2 Proxy UI | e2e 16/16 PASS（CA confirm 负例/明文/越界 403/CONNECT MITM/时序五项）+ 6 截图 browser-e2e/wsw-2/ |
| WSW-3 History/Repeater UI | e2e 32/32 PASS（虚拟滚动 17 行节点/过滤防抖/Body 按需/发送往返/多 Tab/版本回退/脱敏三连）+ 7 截图 browser-e2e/wsw-3/ |
| WSW-8 Browser Evidence | browser_adapter 8 测（四模式真捕获 + 六 ID 对账 + 截断帽/看门狗/scope/selector 负例）+ selftest 28 PASS（PNG magic 89504e47 + sha256 对账 + meta 九字段） |
| MA-4 Runs caller 维度 | web/qa/runs/ 6 帧（mock 4 + 真实后端 2）+ runs-smoke 126/126 |

---

## 四、本收口批复现（命令原文 + 预期）

```bash
cd /media/emo/code/pensting/research-platform
export CARGO_TARGET_DIR=/home/emo/rt3-target

# 门禁基线（本批唯一全量门；本批实测 1337 passed / 0 failed / 2 ignored，WS_EXIT=0，184 测试二进制 ok 行）
cargo test --offline --workspace --no-fail-fast

# 静态门（本批复跑回显：CLIPPY_EXIT=0 / DENY_ADV_EXIT=0 / DENY_LIC_EXIT=0）
cargo clippy --offline --workspace --all-targets -- -D warnings          # exit 0
cargo deny check advisories && cargo deny check licenses                 # 双 ok
cargo fmt --all -- --check                                                # exit 1 存量红 120 文件/936 hunks（预期持平）

# Python / Web 门（本批复跑：Ran 237 tests OK skipped=11；web build exit 0 + vitest 19 files / 126 passed）
python3 -m unittest discover -s workers/python/tests
npm --prefix web run build && npm --prefix web run test

# WSW 四套 selftest（本批复跑全 exit 0）
bash apps/server/selftest_wsw4.sh                                         # PASS=34 FAIL=0 ALL GREEN
RP_WSW_BIN=$CARGO_TARGET_DIR/debug/rp-wsw bash apps/server/selftest_wsw56.sh   # PASS=42 SKIP=0 FAIL=0 ALL GREEN
RP_WSW_BIN=$CARGO_TARGET_DIR/debug/rp-wsw bash apps/server/selftest_wsw7.sh    # PASS=34 SKIP=4 FAIL=0 ALL GREEN
RP_BROWSER_BIN=${RP_BROWSER_BIN:-} bash apps/server/selftest_wsw8.sh           # PASS=28 SKIP=0 FAIL=0 ALL GREEN

# 对账锚
grep -cE '^\[\[package\]\]' Cargo.lock        # 417
grep -c 'name = "reqwest"' Cargo.lock        # 1
grep -rn '^reqwest' crates/*/Cargo.toml | wc -l   # 0
ls migrations/*.sql | wc -l                   # 30
for f in migrations/platform*.db; do sqlite3 "file:$f?mode=ro" "PRAGMA user_version"; done   # 1/1/1/0/0
git status --porcelain -- crates/ migrations/ vendor/ Cargo.lock Cargo.toml web/ workers/ integrations/rtis/ | wc -l   # 0（代码面零 diff）
```

预期：workspace 1337 零红 + selftest 四套全绿 + lock 417/reqwest 双检命中 + 生产库五库指纹 1/1/1/0/0 + 代码面零 diff（本批纯文档批）。
