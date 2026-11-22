# RT-5/6 合并批任务书 · Cargo Profile A/B + 磁盘治理 + Podman 资源与镜像预算（构建与容器预算批；零迁移零 Rust src 改动）

- 批次：RT-5 + RT-6 合并批（NATIVE-FIRST §九 :227「RT-5 Cargo Profile A/B 与磁盘治理：profile 对比报告、debug=1 决策、缓存/清理流程，与 WSW-1 分离，先 probe 后改配置」+ :228「RT-6 Podman 资源与镜像预算：按镜像族/卷/容器归属的报表、阈值告警和安全清理流程，RT-0 基线」）
- 日期：2026-11-23
- 状态：任务书已定稿（2026-11-23 侦察后落稿；实施按本任务书执行；禁 git add/commit/push，工作树留主会话审查收口）
- 依据：[NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md) :227-228（RT-5/RT-6 交付定义）/:241（Cargo Profile A/B 报告含磁盘、时间、诊断能力和测试结果；**没有未验证就改 Profile**）/:244（Podman 清理只作用于确认归属 RIP 且无引用的对象，不执行全局 prune）/:194-196（镜像、容器、卷、构建缓存分别计量；预算触发先报告归属再清理；50 GiB = 历史上限按镜像族重定）+ [RT-0-RUNTIME-BASELINE-AUDIT.md](RT-0-RUNTIME-BASELINE-AUDIT.md) :11（target/debug/deps 约 69 GiB、60.78 GiB 未 strip ELF = 磁盘病理首要对象）/:16（Distrobox Ubuntu/Kali 不应误算 RIP 工具容器）/:71（RT-5 建议：独立稳定 SSD 路径做 `[profile.dev] debug = 1` A/B，记录磁盘/冷热构建/测试时间/backtrace 诊断能力；**没有 A/B 结果前不改主 Cargo.toml、不 cargo clean、不把 target 放 tmpfs**）/:148 + [PHASE-ROADMAP.md](PHASE-ROADMAP.md) :775（2026-11-18 实测 target/ 约 70 GiB 病理与 RT-5 立项词形）/:789-790（RT-5 有数据后再决定 Profile 不降质量门；RT-6 镜像/容器/卷分别计量只清理 RIP 无引用对象）+ [TOOL-RUNTIME-ADR.md](TOOL-RUNTIME-ADR.md) :197/:217/:234（50 GiB 初始预算与复审阈值）
- 前置：WSW-7 已收口（收口 commit `7a82804a` + CI 修复批 `c962a8d7`/`2eaa19fe` 在链，HEAD `2eaa19fe`，`git status --porcelain` = 0 行实测；workspace 基线 = WSW-7 收口 1315 passed 零红，WSW-7-BATCH-REPORT.md:51/:115）；**032 不立项**（D5：零迁移——profile 为 Cargo.toml 值变更、磁盘治理为文档/脚本）
- 设计裁决：本批特有裁决见 §0 D1-D5（**不另立 ADR**——WSW-4/WSW-5-6/WSW-7-TASK 先例，裁决内嵌本档）
- 现状锚点：§1 A1-A12 均为 2026-11-23 本会话实测（HEAD `2eaa19fe`、工作树 0 行、Cargo.lock 399、migrations=30 实测），实施时若行号漂移按词形重定位并以本档词形为准

---

## 0. 设计裁决（本批特有，ADR 内嵌）

| # | 裁决 | 理由 |
|---|---|---|
| D1 | **Profile A/B 照原文裁决 = A「现状默认」vs B「debug=1」，不是「A=dev 增量优先 / B=瘦身 release」**：①**A 面 = 现状默认 profile**（全仓唯一 profile 段 = `[profile.release] strip="debuginfo"`，Cargo.toml:117-119；dev/test 全默认：debuginfo 全量 + incremental 默认开；lto/codegen-units/opt-level 全零设置——A4 实测）；②**B 面 = `[profile.dev] debug = 1`**（test profile 继承 dev——不另落 `[profile.test]` 段，S2 词形核实实测）；③**probe 载体 = `CARGO_PROFILE_DEV_DEBUG=1` 环境变量 + 独立 probe target-dir `/home/emo/rt5-probe`**（/home = nvme1n1p2 独立盘 239G 可用，A7 实测——RT-0 :71「独立、稳定 SSD 路径」词形），probe 期主 Cargo.toml 零改动；④**决策阈值写死**：磁盘节省 ≥40% 且 workspace 全量测试零红（1315 基线延续）且 backtrace 诊断能力实测可接受（`RUST_BACKTRACE=1` 下 panic 输出含源码行号 + 抽样 ELF `readelf -S` debug 段实测入档）三条件同时成立 → 主 Cargo.toml 落 `[profile.dev] debug = 1`（两行级值变更）；任一不成立 → 主 Cargo.toml 零 diff + 报告登记「维持默认」+ 复测条件词形 | RT-0 :71/:148 与 ROADMAP :775 已把 A/B 定义为「默认 Debug 与 debug=1 的磁盘/耗时/回溯诊断 A/B」——本批照原文不另造 A/B 语义；NATIVE-FIRST :227「先 probe 后改配置」+ :241「没有未验证就改 Profile」= 环境变量 probe 满足「probe 不改配置」词形；release 面（lto/opt-level/codegen-units）**零触碰**（无 A/B 数据不验证不发布面——release 只在 P4-4 benchmark 与 CI 域使用，本批范围外） |
| D2 | **磁盘治理 = 零 sudo 三层动作清单 + 文档化**：①**L0 report（脚本默认 dry-run）**：target/ 与 rt3-target/ 双 target 分层占用表（deps/incremental/build/release 四层 du）+ podman df 快照；②**L1 incremental 老化（--apply 可执行）**：`<target>/debug/incremental/` 目录清空 = 再生缓存零语义损失（rt3-target 14G + repo target 909M，预期回收 ≈14.9G 实测入档）；③**L2 deps 旧 fingerprint 残留 = 报告级禁执行**（RT-0 :14「本轮没有把任何文件判定为可安全删除」延续——deps 层 50G 主产物零删除，判定面留报告附录）；④**btrfs 面 = sudo 步骤登记留用户**（`btrfs balance start`/细粒度 usage/scrub 均需 root——本会话 `btrfs filesystem usage` 无 root 警告实录「run as root」，balance 执行步骤写进 DISK-GOVERNANCE 文档，本会话零 sudo 命令）；⑤**CARGO_TARGET_DIR 双 target 布局文档化**（repo `target/` 4.8G 在 /media/emo + 批测 `/home/emo/rt3-target` 64G 在 /home 独立盘——分盘归因 + 环境变量用法词形进文档） | 用户权限命令（du/rm incremental/podman/df）零 sudo 可执行；btrfs balance 需 sudo = 病理修复面非应用面，登记即交付（NATIVE-FIRST :194「分别计量不用单一总数代替归因」词形）；RT-0 :71「不执行 cargo clean」= A/B 前禁词形，本批 A/B 用独立 rt5-probe 不触主 target，cargo clean 不在本批任何步骤 |
| D3 | **fmt chore 不并入本批（独立 chore 批登记观察池）**：实测 `cargo fmt --all -- --check` 全仓存量红 = **119 文件 / 935 diff hunks**（与 WSW-5-6-BATCH-REPORT §E.5 :118「约 120 文件」口径吻合；分布 workflow-engine 23 / intel-db 21 / verification-engine 11 / research-engine 7 / wsw-runtime 10 / mcp-server 8 / domain 5 / context-broker 4 / …）；**链尾十六钉交集 4 文件**（crates/agent-engine/tests/migration_idempotent.rs、crates/rp-mcp-parity/tests/caller_auth.rs、crates/wsw-runtime/tests/exchange_persist.rs、crates/wsw-runtime/tests/fuzz_job.rs——WSW-5-6-BATCH-REPORT A.2 :30-49 十六清单成员，fmt-dirty 逐一 grep 实测）+ **跨轨禁改面命中 3 文件**（crates/wsw-runtime/src/sender.rs = WSW-7 §2.2「逐字节零 diff」面、crates/wsw-message/src/diff.rs、crates/wsw-runtime/src/fuzz/persist.rs）→ 全仓 fmt = 触碰链尾硬钉文件重排 + WSW 轨零 diff 红线面，**与零链尾随更纪律直接冲突**；裁决 = 本批零 Rust src 改动零 fmt 触碰（延续「仅本批文件 rustfmt」口径——本批无 Rust 文件落点），fmt 全仓收敛登记为**独立 chore 批**（前置条件：链尾钉值面同步评审窗口 + 跨轨零 diff 红线临时豁免词形，届时须链尾十六钉 owner 会签） | 链尾十六钉 = 迁移链尾断言文件，随更语义已钉死「零随更除非链尾演进」（WSW-7-TASK D5 :21 词形）；fmt 重排会产生非随更 diff，淹没链尾审计信号；sender.rs/fuzz/ 逐字节零 diff = WSW-4/5+6/7 三批连续红线（WSW-7-TASK §2.2 :68-69 词形）——fmt 收敛批必须单独立项评审，禁止夹带 |
| D4 | **容器预算面 = 归属报表 + dangling-only 清理（禁全局 prune）**：①**归属报表脚本化**：镜像（37 个，9.597GB，10 活跃）/容器（5 个，11.74GB，2 活跃）/卷（13 个，568MB，5 活跃）三表分别计量 + 每镜像的容器引用/创建时间/尺寸归属列（NATIVE-FIRST :194「分别计量不用单一 podman system df 总数代替归因」词形）；②**执行面 = dangling-only**：4 个 `<none>:<none>` 无标签镜像（1.37GB×3 三天前 = WSW-7 S3 ZAP 构建尝试层 + 1.07GB 七天前 = rip-pg17 重建旧层，容器引用零命中 grep 实测，合计 ≈5.18GB）→ `podman image prune -f`（dangling-only）执行回收实测 before/after `podman system df` 入档；**禁无差别 `podman system prune -a --volumes`**（NATIVE-FIRST :195 原文词形）；③**零触碰面**：ubuntu/kali 两个运行中 Distrobox 容器与其镜像（RT-0 :16/:133「不应误算成 RIP 工具运行容器」词形）、3 个已停 rip-pg17 容器（rip-p17-restore/rip-pg17/rip-p17-ma3-test——MA-3 测试证据面，只报不删）、全部 13 个卷（410.7MB 可回收只报归属零删除）；④**GraphRoot = `/media/emo/containers/storage`**（~/.config/containers/storage.conf:4 实测——podman 对象与仓库同盘，镜像治理直接改变 /media/emo 占用，归因表必列）；⑤**告警阈值建议 = 按镜像族实测重定初稿**（11 个 containerfiles 族清单 + 当前在位镜像族实测数——NATIVE-FIRST :196「按实测镜像族重新定预算与告警阈值」词形，TOOL-RUNTIME-ADR :197 50GiB 标注历史值） | :195「预算触发先报告具体镜像/标签/容器/卷归属，再清理明确由 RIP 创建且已无引用的对象」= dangling-only 是唯一满足「明确无引用」的免审执行面；Distrobox/停容器/卷 = 归属未确认面零执行；ZAP 镜像面 = WSW-7 D1 选项 B 收口（构建外联坐死，WSW-7-BATCH-REPORT :5），rip-zap 构建层残留即 3×1.37GB dangling 主嫌疑——回收后记录为选项 B 环境复测输入 |
| D5 | **032 不立项（零迁移）**：RT-5 profile 改动 = Cargo.toml `[profile.*]` 值变更（非 schema/表结构面，Cargo.lock 399 恒定——profile 段不进 lock 实证词形）；磁盘治理 = docs/scripts 零迁移；migrations/001-030 零 diff + 链尾十六钉零随更；WSW-7 已以 031 候选号不立项（WSW-7-TASK D5 :21 词形），本批 032 同理不立项；若实施中坐实需新表 → 停批上报重新立项禁临场加迁移 | 迁移立项判据 = 需要新表/新列/新索引；本批全部交付面（报告/脚本/文档/两行 profile 值）零数据库面 |

## 1. 现状锚点（2026-11-23 本会话实测，A1-A12）

| 锚点 | 事实 | 出处 |
|---|---|---|
| A1 RT-5/RT-6 原文定义 | RT-5 行 :227（Cargo Profile A/B 与磁盘治理；交付 = profile 对比报告、debug=1 决策、缓存/清理流程；前置 = 与 WSW-1 分离、先 probe 后改配置）+ RT-6 行 :228（Podman 资源与镜像预算；交付 = 按镜像族/卷/容器归属的报表、阈值告警和安全清理流程；前置 = RT-0 基线）；验收 :241（A/B 报告含磁盘、时间、诊断能力和测试结果；没有未验证就改 Profile）/:244（清理只作用于归属 RIP 且无引用对象，禁全局 prune）；计量规则 :194-196（镜像/容器/卷/构建缓存分别计量；先报告归属再清理；50 GiB = 历史上限按镜像族重定） | docs/phase0/NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md:227-228,241,244,194-196 |
| A2 磁盘病理基线（RT-0） | target/debug/deps 约 69 GiB、其中 60.78 GiB = 无扩展名未 strip ELF（完整 .debug_info）= 磁盘问题首要对象 :11；审计时（2026-11-18，HEAD `4e7b4c69`）总可用约 185 GiB :41；同目标 10-17 组历史 fingerprint 残留 + Cargo 不自动回收旧 fingerprint :12-14；RT-5 建议 :71（独立 SSD 路径 debug=1 A/B，记录磁盘/冷热构建/测试时间/backtrace 诊断；不改主 Cargo.toml、不 cargo clean、不放 tmpfs）/:148（独立 target-dir 验证 debug=1）；Distrobox Ubuntu/Kali 非 RIP 容器 :16/:133 | docs/phase0/RT-0-RUNTIME-BASELINE-AUDIT.md:11,16,41,71,148,133 |
| A3 ROADMAP 立项词形 | :775（2026-11-18 本机实测 target/ 约 70 GiB、deps 约 69 GiB；vendor 约 386 MiB 不能误归因容器镜像；独立 probe target 做默认 Debug 与 debug=1 的磁盘/耗时/回溯诊断 A/B，再决定是否调整 [profile.dev]/[profile.test]；A/B 前不改 Profile、不 cargo clean、不删构建文件）+ :789（RT-5 = Cargo Profile A/B 与构建磁盘治理，有数据后再决定 Profile、不降低质量门）+ :790（RT-6 = Podman 存储预算与归属治理，镜像/容器/卷分别计量、只清理明确属于 RIP 且无引用的对象） | docs/phase0/PHASE-ROADMAP.md:775,789,790 |
| A4 Profile 现状 | 全仓唯一 profile 段 = 根 Cargo.toml:117-119（`[profile.release]` + 注释 + `strip = "debuginfo"`，Cargo.toml 全文 119 行）；25 个成员 crate Cargo.toml **零** `[profile.*]` 段（grep 实测）；dev/test/增量/lto/codegen-units/opt-level 全零显式设置（cargo 默认：dev debuginfo 全量 + incremental 开）；`.cargo/config.toml` 仅 vendored-sources 四行（:1-4，无 build/target-dir/profile 键） | Cargo.toml:117-119；.cargo/config.toml:1-4；grep 实测（crates/*/Cargo.toml 零命中） |
| A5 target 占用双布局（实测 2026-11-23） | repo `target/` = 4.8G（debug 4.8G = deps 3.7G + incremental 909M + build 155M；release 14M）；批测 `/home/emo/rt3-target` = 64G（debug 63G = deps 50G + incremental 14G + build 352M；release 303M；tmp 0）；RT-0 审计时 repo target 约 70 GiB（:18 表——已随重建/分盘收缩）；rt3-target 为 DI-3/WSW 系批测 `CARGO_TARGET_DIR` 承载位（DI-3-RT-4-TASK.md:76/:103 词形、WSW-7-TASK :111/:113-115 词形） | du 实测（本会话命令原文入报告）；docs/phase0/DI-3-RT-4-TASK.md:76,103；docs/rip3/wsw/WSW-7-TASK.md:111,113-115 |
| A6 「不为验收提速调高」断言面（profile 关联纪律） | crates/tool-engine/tests/core_utils_e2e.rs:11（「每工具族并发基线 1（FamilyLimiter 既有默认，不为验收提速调高）」头注）+ :530（断言词形）；PodmanHostConfig::default `family_concurrency_default: 1`（crates/tool-engine/src/adapter/podman_host.rs:69/:83/:104 红线 2 注）；MA-2-TASK D7 :19/:70（族基线不随 caller 帽缩放零改动词形）——**profile 关联面**：同一纪律类——性能旋钮（并发帽/profile 旗标）不得为缩短验收时长调高/调低；本批 profile 变更唯一目的 = 磁盘治理与构建成本（RT-0 :71 定义域），验收基线必须同水位零红 | crates/tool-engine/tests/core_utils_e2e.rs:11,530；crates/tool-engine/src/adapter/podman_host.rs:69,83,104；docs/rip3/MA-2-TASK.md:19,70 |
| A7 磁盘面（实测 2026-11-23） | `/media/emo`（btrfs，/dev/nvme0n1p1）：477G 总 / 199G 已用 / 277G 可用（42%）——RT-0 审计时 185 GiB 可用已改善；`btrfs filesystem usage`（无 root）：Device size 476.94GiB / allocated 309.93GiB / Used 198.19GiB / Data,single 305.91GiB used 64.23% / Metadata,DUP 877.69MiB；**无 root 警告实录**「cannot read detailed chunk info…run as root」（balance/细粒度 = sudo 步骤留用户）；`/home/emo`（/dev/nvme1n1p2 **独立盘**）：473G 总 / 229G 已用 / 239G 可用（49%）——rt3-target 64G 在此盘；vendor/ = 376M | df/btrfs/du 实测（命令原文入报告） |
| A8 Podman 面（实测 2026-11-23，podman 6.2.0） | `podman system df`：Images 37 个（10 活跃）9.597GB，可回收 6.078GB（63%）；Containers 5 个（2 活跃）11.74GB，可回收 67.39kB；Local Volumes 13 个（5 活跃）568MB，可回收 410.7MB（72%）；镜像清单：localhost/rip-nuclei:latest 935MB（53 分钟前 = WSW-7 S7 重建）、localhost/rip-pg17:latest 1.13GB、上游 docker.io/projectdiscovery/nuclei:latest 935MB、ghcr.io/emoeem/pkgbuild-builder 2.58GB、localhost/ubuntu-zcode:26.04 2.34GB、kali-rolling 125MB 等；**dangling 4 个** `<none>:<none>` = 1.37GB×3（3 天前）+ 1.07GB×1（7 天前）合计 ≈5.18GB，5 容器引用零命中；容器 5 个 = ubuntu（Up 3h）/kali（Up 3h）Distrobox + rip-p17-restore（Exited 0，6 天）/rip-pg17（Exited 135，5 天）/rip-p17-ma3-test（Exited 0，8h，digest 引用 sha256:229189e4）；GraphRoot = `/media/emo/containers/storage`（~/.config/containers/storage.conf:4）——与仓库同盘；~/rip-runtime 304K | podman system df/images/ps/volume ls 实测（命令原文入报告）；~/.config/containers/storage.conf:4 |
| A9 容器族与预算原文 | containerfiles/ 11 族 = rip-embed / rip-gov / rip-nuclei / rip-parser / rip-pg17 / rip-rerank / rip-tool-base / rip-tool-coreutils / rip-tool-source / rip-tool-webresearch / rip-zap（ls 实测）；FROM digest 钉：rip-nuclei :19（docker.io/projectdiscovery/nuclei@sha256:582d5546…）、rip-pg17 :18/:34（paradedb:pg17@sha256:7cb29053…）、rip-zap :32（ghcr.io/zaproxy/zaproxy@sha256:**64 零占位**——WSW-7 D1 选项 B 未实拉，WSW-7-BATCH-REPORT :5/:13/:15）；TOOL-RUNTIME-ADR :197（镜像与缓存总量初始预算 50 GiB）/ :217（容器存储占家目录磁盘 50 GiB 封顶）/ :234（可用磁盘低于 50 GiB 复审阈值）；NATIVE-FIRST :196（50 GiB = 历史预算上限按实测镜像族重定） | containerfiles/（ls 实测）；containerfiles/rip-nuclei/Containerfile:19；containerfiles/rip-pg17/Dockerfile:18,34；containerfiles/rip-zap/Containerfile:32；docs/phase0/TOOL-RUNTIME-ADR.md:197,217,234；docs/phase0/NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md:196 |
| A10 fmt 存量红（D3 实测面） | `cargo fmt --all -- --check` = 119 文件 / 935 diff hunks（2026-11-23 实测，对照 WSW-5-6-BATCH-REPORT §E.5 :118「约 120 文件」口径）；分布：workflow-engine src 17+tests 6 / intel-db src 14+tests 7 / verification-engine src 11 / research-engine 7 / wsw-runtime src 6+tests 4 / mcp-server 5+3 / domain 5 / context-broker tests 4 / tool-registry 3 / agent-engine 3+3 等；**链尾十六钉交集 4 文件**（migration_idempotent.rs / caller_auth.rs / exchange_persist.rs / fuzz_job.rs）+ **跨轨禁改面 3 文件**（wsw-runtime/src/sender.rs / wsw-message/src/diff.rs / wsw-runtime/src/fuzz/persist.rs）grep 实测 | 本会话 cargo fmt 实测（文件清单入报告附录）；docs/rip3/wsw/WSW-5-6-BATCH-REPORT.md:118,30-49 |
| A11 WSW-7 收口态（本批前置基线） | HEAD `2eaa19fe`（7a82804a WSW-7 收口 + c962a8d7/2eaa19fe CI 修复批在链）；`git status --porcelain` = 0 行；Cargo.lock 399 / reqwest=0；migrations = 30；workspace 1315 passed 零红（WSW-7-BATCH-REPORT.md:51/:115 收口口径）；selftest_wsw7.sh 301 行在位（apps/server/）；apps/server/selftest_wsw56.sh 零改动回归 42/0 | git log/status 实测；docs/rip3/wsw/WSW-7-BATCH-REPORT.md:5,51,115；apps/server/selftest_wsw7.sh |
| A12 脚本与文档落点现状 | scripts/ 现有 11 个脚本（calibrate_est_tokens.py / local_range.py / local_range_template.yaml / p4_4_bench_seed.py / rip_cli_worker.py / rip_etl_status.py / rip_inventory.py / rip_kb_search.py / rip_new_tool.py / rtis_kb_worker.py / sk2_tongui_catalog.py）——**零 target/podman 治理脚本**；docs/phase0/ 无 RT-5/RT-6 专档（RT-0/RT-1/RT-2/RT-3/DI 系在位）；docs/README.md 索引节为 additive 登记位（WSW-7-BATCH-REPORT §9 先例） | ls scripts/ / ls docs/phase0/ 实测 |

## 2. 范围

### 2.1 允许改动（落点清单）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `Cargo.toml` | 修改（**条件落点**） | D1④：A/B 三条件全过 → `[profile.dev]` 段落 `debug = 1`（两行级值变更；test profile 继承 dev 不另落段）；任一不过 → **零 diff** + 报告登记「维持默认」决策 | S3 |
| `scripts/disk_gc.sh` | 新建 | D2/D4：零 sudo 治理脚本——`report`（默认 dry-run：双 target 四层 du 表 + podman df + dangling 归属表）/ `gc-incremental`（L1：--apply 显式执行 incremental 老化，before/after du 入 stdout）/ `report-deps`（L2：deps 旧 fingerprint 报告级，零删除）/ `podman-prune-dangling`（D4②：--apply 显式执行 dangling-only image prune，before/after system df；禁 -a --volumes 词形硬编码拒绝） | S4 |
| `docs/phase0/RT-5-6-DISK-GOVERNANCE.md` | 新建 | D2/D4：磁盘治理文档——CARGO_TARGET_DIR 双 target 布局（repo 4.8G @ /media/emo + rt3-target 64G @ /home 独立盘）与用法词形、分层清理策略 L0-L2 定义、btrfs sudo 步骤登记（balance/scrub/细粒度 usage——命令原文 + 执行时点建议，留用户）、podman 归属治理流程（归属报表 → dangling-only → 停容器/卷评审面）、告警阈值建议初稿（按镜像族实测重定——NATIVE-FIRST :196 词形） | S5 |
| `docs/phase0/RT-5-6-BATCH-REPORT.md` | 新建 | 批次报告：A/B 对比数字表（磁盘/冷构建/hot rebuild/诊断能力四列 A·B 双面）+ debug=1 决策记录 + 磁盘回收数字（incremental ≈14.9G 实测 + dangling ≈5.18GB 实测）+ 坏消息单列 + 观察池登记（D3 fmt chore 批 + RT-7 CI 面）| S6 |
| `docs/README.md` | additive | 索引登记两份新档（WSW-7 §9 先例） | S6 |

### 2.2 不修改

- `Cargo.toml` 除 D1④ 条件两行外零 diff：`[profile.release]`（strip/lto/opt-level/codegen-units）零触碰（无 A/B 数据——NATIVE-FIRST :241 词形）；`Cargo.lock` 零 diff（399 恒定，profile 段不进 lock）；`vendor/` 零 diff；`.cargo/config.toml` 零 diff。
- `crates/**` 全部源码与测试零 diff（零 Rust src 改动批——fmt 存量红 119 文件零触碰，D3；core_utils_e2e.rs:530 与 podman_host.rs:69/:83 族基线语义零改动，A6）；`migrations/` 001-030 零 diff + 链尾十六钉零随更（D5）；`web/**`、`integrations/rtis/**`、`apps/server/` 既有 selftest 零 diff（selftest_wsw4.sh 28 陈旧钉维持已知红——禁顺手改测试钉值）。
- `scripts/` 既有 11 脚本零 diff（A12）；新增落点仅 `scripts/disk_gc.sh`。
- 生产库零触碰：`migrations/platform*.db` 与 `platform-dryrun.db` 全程只读指纹（selftest_wsw56.sh :623-628 先例词形）。
- 宿主零安装零升级：零新装软件、零 sudo 命令（D2④——btrfs balance 只登记不执行）；tmpfs target 方案零引入（RT-0 :71 禁词形）。
- Podman 零触碰面（D4③）：ubuntu/kali Distrobox 容器与镜像、3 个已停 rip-pg17 容器、13 个卷、rip-nuclei/rip-pg17/上游 nuclei 等带标签镜像；禁 `podman system prune -a --volumes` 全局面（NATIVE-FIRST :195）。
- `~/rip-runtime/`（304K）与 `~/.dsh/`（RTIS 真源）零触碰；rt3-target `deps/` 50G 主产物零删除（D2③ L2 报告级）。

## 3. 实施步骤（S1→S6 串行，每步过门再进）

1. **S1 门禁基线核验**：`git status --porcelain` = 0 + HEAD = `2eaa19fe`；`grep -cE '^\[\[package\]\]' Cargo.lock` = 399；`ls migrations/*.sql | wc -l` = 30；A1-A12 锚点词形复核；基线快照入档——`du -sh target/ /home/emo/rt3-target` + 四层明细 + `podman system df` + `df -h /media/emo /home/emo`；`CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast` **现状全量一次基线**（预期 1315 passed 零红延续；如有红先停批上报）；**门 = 基线数字表落报告底稿**。
2. **S2 Profile A/B probe（独立 target-dir，主 Cargo.toml 零改动）**：probe 词形核实——`CARGO_PROFILE_DEV_DEBUG=1` 环境变量生效实测（构建日志 debuginfo 级别对照 + 抽样产物 `readelf -S | grep debug_info` 双面）+ test profile 继承 dev 词形核实（`cargo test` 同 probe 变量下构建对照）；probe target = `/home/emo/rt5-probe`（/home 独立盘 239G 可用）；A/B 实测矩阵：A 面 = 默认（unset）vs B 面 = `CARGO_TARGET_DIR=/home/emo/rt5-probe-b CARGO_PROFILE_DEV_DEBUG=1`，同命令 `cargo build --offline --workspace --tests` 冷构建（time + du）+ 第二次增量热构建（time）+ 测试代表面 `cargo test --offline -p tool-engine -- --test-threads=1`（时间对照）+ 诊断能力（`RUST_BACKTRACE=1` 下 panic 输出行号可用性 + `readelf -S` debug 段抽样——RT-0 :71「backtrace/诊断能力」词形）；probe 目录用后清理（`rm -rf /home/emo/rt5-probe*`——A/B 数据入档后 probe 产物零保留面）；**门 = A/B 数字表落报告（磁盘 GiB / 冷构建 s / 热构建 s / 诊断能力四列）**。
3. **S3 debug=1 决策**：D1④ 三条件判定（磁盘节省 ≥40% + workspace 全量零红 + 诊断能力可接受）→ 全过 = Cargo.toml 落 `[profile.dev]\ndebug = 1` + `cargo build --offline --workspace` + 全量测试复验零红；任一不过 = Cargo.toml 零 diff + 「维持默认」+ 复测条件词形登记；**门 = 决策记录入报告 + 落点后 `git diff Cargo.toml` ≤4 行或 0 行**。
4. **S4 磁盘治理脚本**：scripts/disk_gc.sh 落盘（D2 三层 + D4 dangling 分面；dry-run 默认、`--apply` 显式、禁 -a --volumes 词形硬编码拒绝分支）；`bash scripts/disk_gc.sh report` exit 0 实测 + `gc-incremental --apply` 执行（rt3-target/debug/incremental 14G + target/debug/incremental 909M before/after du 实测入档——预期回收 ≈14.9G）；L2 report-deps 输出附录入报告（零删除）；**门 = 回收数字实测 + report 模式幂等 exit 0**。
5. **S5 Podman 归属治理**：归属报表实跑（镜像 37 归属表 + 容器 5 + 卷 13——每镜像容器引用/创建时间/尺寸列）；dangling 4 镜像归属双证（引用零命中 + 创建时间线对齐 rip-* 重建活动）→ `podman image prune -f` 执行 + before/after `podman system df` 入档（预期回收 ≈5.18GB）；ubuntu/kali/停容器/卷零触碰反证（`podman ps -a` 五容器 before/after 一致 + 卷 13 个不变）；**门 = 回收数字实测 + 零触碰面 before/after 逐项一致**。
6. **S6 文档与收口**：RT-5-6-DISK-GOVERNANCE.md + RT-5-6-BATCH-REPORT.md 落盘 + docs/README.md 索引；§5 全部命令实跑（workspace 全量一次——预期零红延续）；`git status --porcelain` 只含 §2.1 落点核对（crates/ / migrations/ / vendor/ / Cargo.lock / web/ / apps/server/ 零 diff 实证）；显式 add 本批清单（禁 add -A——本批只落文件不 commit）。

## 4. 完成标准（判据表；验收 = 命令实跑回显）

| # | 判据 | 验收 |
|---|---|---|
| 1 | **A/B 对比数字表**（禁拍脑袋）：磁盘 / 冷构建 / 热构建 / 诊断能力四列，A·B 双面实测 | 报告含数字表（单位 GiB / s，probe target-dir 路径与命令原文逐条入档）+ probe 产物清理反证（`ls /home/emo/rt5-probe*` = No such file） |
| 2 | **debug=1 决策记录**（NATIVE-FIRST :241「没有未验证就改 Profile」反证） | 决策 = 三条件逐项判定词形（数字支撑）；落点时 `git diff Cargo.toml` 两行级且仅 [profile.dev] 段；维持默认时 Cargo.toml 零 diff + 复测条件登记 |
| 3 | **磁盘回收数字（target 面）** | gc-incremental --apply 实测 before/after du（预期 ≈14.9G：rt3-target 14G + repo 909M）；`ls target/debug/incremental` 复建后再生（后续构建可用性反证） |
| 4 | **Podman 回收数字（dangling 面）** | `podman image prune -f` before/after system df（预期 ≈5.18GB / Images 37→33）；镜像总数 33 + 可回收面下降数字入档 |
| 5 | **归属报表与零触碰面** | 归属表三张（镜像/容器/卷分别计量——:194 词形）；`podman ps -a` 五容器 before/after 一致 + `podman volume ls` 13 个不变 + ubuntu/kali 容器 Up 态未断（Distrobox 零触碰反证） |
| 6 | **脚本可运行** | `bash scripts/disk_gc.sh report` exit 0 幂等；无 --apply 时零删除动作（dry-run 默认反证）；`--podman-prune` 无 `--apply` 时拒绝执行；`-a --volumes` 词形出现在脚本 = 硬编码拒绝分支存在（grep 反证） |
| 7 | **Profile 构建验证** | 落点（或维持默认）后 `CARGO_TARGET_DIR=/home/emo/rt3-target cargo build --offline --workspace` exit 0 + workspace 全量测试零红（1315 基线延续） |
| 8 | **零迁移与族基线语义不动** | migrations 001-030 零 diff + Cargo.lock 399 + vendor 零 diff + 链尾十六钉零随更 + `git status --porcelain crates/` = 0 行（含 core_utils_e2e.rs / podman_host.rs——A6 断言面零改动） |
| 9 | **生产库只读指纹** | 五文件 `sqlite3 "file:$f?mode=ro" "PRAGMA user_version"` = 30 逐文件回显（selftest_wsw56.sh :623-628 先例） |
| 10 | **fmt 存量红零扩大** | `cargo fmt --all -- --check` 文件计数 ≤119（本批零 Rust 文件落点；新建 shell/doc 不入 fmt 面——D3） |
| 11 | **文档面** | DISK-GOVERNANCE（双 target 布局 / L0-L2 / sudo 步骤登记 / 归属流程 / 阈值建议五节齐）+ BATCH-REPORT（数字表 + 决策 + 回收数字 + 坏消息单列）+ docs/README.md 两行索引 |
| 12 | **回归与收口** | workspace 全量一次零红 + `cargo deny check advisories && cargo deny check licenses` 双检过 + `RP_WSW_BIN=/home/emo/rt3-target/debug/rp-wsw bash apps/server/selftest_wsw7.sh` 零改动回归 PASS/SKIP 分开计数 exit 0 + `git status --porcelain` 只含 §2.1 落点 |

## 5. 测试面（收口命令）

```bash
# 门禁基线（S1）
git status --porcelain | wc -l          # 0
git log --oneline -1                    # 2eaa19fe（WSW-7 收口链在位）
grep -cE '^\[\[package\]\]' Cargo.lock  # 399
ls migrations/*.sql | wc -l             # 30
# Rust 全量（2026-11-23 纪律：workspace 一次必须跑；基线 = WSW-7 收口 1315 passed 零红）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast
# 静态门（零 Rust src 改动批——全量 clippy 一次证明 profile/环境变更不引入新告警）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check > /tmp/fmt_after.txt 2>&1; grep -c "^Diff in" /tmp/fmt_after.txt   # ≤935 hunks（存量红零扩大，A10/D3）
# deny / lock 红线
cargo deny check advisories && cargo deny check licenses
grep -cE '^\[\[package\]\]' Cargo.lock  # 399（S1 基线对比恒定）
# Profile A/B probe（S2——数字入报告；probe 目录用后清理）
CARGO_TARGET_DIR=/home/emo/rt5-probe-a cargo build --offline --workspace --tests        # A 面（默认）
CARGO_TARGET_DIR=/home/emo/rt5-probe-b CARGO_PROFILE_DEV_DEBUG=1 cargo build --offline --workspace --tests   # B 面（debug=1）
du -sh /home/emo/rt5-probe-a /home/emo/rt5-probe-b                                      # 磁盘列
ls /home/emo/rt5-probe* 2>&1 | tail -1                                                   # 收口后 = No such file or directory
# 治理脚本（S4/S5）
bash scripts/disk_gc.sh report                                                          # dry-run 报表 exit 0
bash scripts/disk_gc.sh gc-incremental --apply                                          # before/after du 实测
bash scripts/disk_gc.sh podman-prune-dangling                                           # 无 --apply 拒绝
bash scripts/disk_gc.sh podman-prune-dangling --apply                                   # dangling-only prune（预期 Images 37→33）
podman system df                                                                        # before/after 对比行
podman ps -a --format "{{.Names}} {{.Status}}" | wc -l                                  # 5（零触碰反证）
podman volume ls --format "{{.Name}}" | wc -l                                           # 13（零触碰反证）
grep -n "prune -a" scripts/disk_gc.sh                                                   # 零命中（禁全局 prune 反证）；拒绝分支词形另行 grep
# 生产库只读指纹（S6）
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db migrations/platform-governance.db migrations/platform-search.db; do sqlite3 "file:$f?mode=ro" "PRAGMA user_version"; done   # 逐文件 30
# 零 diff 反证（S6）
git status --porcelain -- crates/ migrations/ vendor/ Cargo.lock web/ integrations/rtis/ | wc -l   # 0
git status --porcelain | wc -l                                                           # 仅 §2.1 落点（≤5 行）
# 既有回归（零改动）
RP_WSW_BIN=/home/emo/rt3-target/debug/rp-wsw bash apps/server/selftest_wsw7.sh           # PASS/SKIP 分开计数 exit 0
```

基线记录（本任务书写作时点 2026-11-23 实测）：

- HEAD `2eaa19fe` 干净树（WSW-7 收口 7a82804a + CI 修复批在链）；migrations = 30；Cargo.lock 399 / reqwest=0。
- target 双布局：repo `target/` 4.8G（deps 3.7G + incremental 909M）@ /media/emo；`/home/emo/rt3-target` 64G（deps 50G + incremental 14G + build 352M）@ /home 独立盘。
- /media/emo 277G 可用（42% used）；/home 239G 可用（49% used）——RT-0 病理时点（185 GiB 可用）已改善，治理批目的 = 流程固化非抢救。
- podman 37 镜像 9.597GB（dangling 4 个 ≈5.18GB 零引用）+ GraphRoot `/media/emo/containers/storage` 与仓库同盘。
- fmt 存量红 119 文件 / 935 hunks（A10）——本批零触碰。

## 6. 红线（八条，违反任一即停批上报）

1. **先 probe 后改配置**（NATIVE-FIRST :227/:241）：A/B 数字落地前主 Cargo.toml 零 diff；probe 载体 = 环境变量 + 独立 rt5-probe target-dir；决策 = D1④ 三条件写死判定，禁拍脑袋落点；`[profile.release]` 全段（strip/lto/opt-level/codegen-units）零触碰（无数据不验证 release 面）。
2. **「不为验收提速调高」纪律延续（profile 关联面）**：profile 变更唯一目的 = 磁盘治理与构建成本（RT-0 :71 定义域），**不得以缩短验收时长为目的调低质量**——落点后 workspace 全量测试必须同水位零红（1315 基线延续）；族基线语义零改动（core_utils_e2e.rs:11/:530 断言 + podman_host.rs:69/:83 `family_concurrency_default = 1` + :104 红线注零 diff）；测试/诊断能力降级（backtrace 失效等）触发 D1④ 否决分支。
3. **生产库零触碰**：`migrations/platform*.db` 五文件全程只读指纹（mode=ro）；零写面零迁移面（032 不立项——D5）。
4. **磁盘动作零 sudo 化 + 宿主零安装**：本会话零 sudo 命令（btrfs balance / 细粒度 usage / scrub / check = sudo 步骤只登记文档化留用户——D2④）；零宿主软件新装零升级（REDLINES 红线 8 同型）；tmpfs target 方案零引入（RT-0 :71 禁词形）；target 治理零 sudo 全覆盖（incremental 老化 = 用户权限再生缓存）。
5. **Podman 清理边界**（NATIVE-FIRST :195 原文）：只清理明确归属 RIP 且零容器引用的 dangling 对象（执行前归属双证：引用零命中 + 创建时间线对齐）；**禁无差别 `podman system prune -a --volumes`**（脚本硬编码拒绝）；ubuntu/kali Distrobox 容器与镜像零触碰（RT-0 :16/:133 非 RIP 词形）；3 个已停 rip-pg17 容器只报不删；13 个卷零删除（410.7MB 只报归属）；带标签镜像（rip-nuclei/rip-pg17/上游 nuclei 等）零删除。
6. **零迁移 + 032 不立项**：migrations/001-030 零 diff；Cargo.lock 399 恒定零 diff（profile 段不进 lock）；vendor/ 零 diff；链尾十六钉零随更；禁顺手改测试钉值（selftest_wsw4.sh 28 陈旧钉维持已知红）；坐实需新表 → 停批上报重新立项禁临场加迁移。
7. **fmt chore 不并入**：119 文件存量红零触碰零扩大（本批零 Rust 文件落点；新建 shell/doc 不入 fmt 面）；`crates/**` 全目录零 diff（含 sender.rs/scope.rs/wsw-message/fuzz 跨轨禁改面——WSW-7 §2.2 :68-69 词形延续）；fmt 全仓收敛 = 独立 chore 批（观察池登记——D3：链尾硬钉重排与零链尾随更纪律冲突，须单独立项评审禁夹带）。
8. **边界纪律（观察池不进）**：RT-7 CI 面零改动（发布策略核验独立批——:229）；CI 质量门零缩减（:242 词形）；deps 层 50G 判定零执行（L2 报告级——D2③）；`~/rip-runtime/`、`~/.dsh/`、vendor 面零触碰；禁 commit/add -A/push（本批只落文件，工作树留主会话审查收口——S6 显式 add 仅登记清单）。

## 7. 回滚

- 文件级 revert：Cargo.toml `[profile.dev]` 两行 / scripts/disk_gc.sh / 两份文档 / docs/README 索引行随 `git checkout/checkout HEAD --` 或 revert 逐项撤销（本批零 commit——主会话审查收口，revert 面 = 工作树丢弃）。
- incremental 老化零回滚面（再生缓存：下次构建自动重建，唯一代价 = 首次增量构建时间）。
- podman dangling 镜像删除**不可逆**：前置归属报表 + 镜像族重建路径兜底（rip-nuclei = containerfiles/rip-nuclei/Containerfile :19 digest 钉 `podman build` 复建；上游 nuclei = docker.io/projectdiscovery/nuclei:latest 拉取面；ZAP = 选项 B 未建成零损失面）——删除仅限 dangling（无标签无引用），零回滚风险面登记。
- Cargo.lock/vendor/crates/migrations/web 零 diff 无回滚面；生产库/宿主/Distrobox 容器零操作无生产回滚面；btrfs balance 未执行零回滚面（sudo 步骤留用户——执行与否由用户裁量）。

## 8. 交付物

1. **A/B 对比报告**：磁盘 / 冷构建 / 热构建 / 诊断能力四列数字表（A 默认 vs B debug=1，probe 命令原文逐条入档）+ debug=1 决策记录（三条件判定词形；落点或维持默认双形态）——RT-5 交付「profile 对比报告、debug=1 决策」:227 对照。
2. **Cargo.toml 条件落点**：`[profile.dev] debug = 1` 两行级（三条件全过时）或零 diff + 维持默认登记（任一不过时）；`[profile.release]` 零触碰实证。
3. **scripts/disk_gc.sh**：report（默认 dry-run）/ gc-incremental（--apply）/ report-deps（L2 报告级）/ podman-prune-dangling（--apply）四模式；禁全局 prune 词形硬编码拒绝分支；report 模式 exit 0 幂等实测。
4. **磁盘回收数字**：incremental 老化 before/after du（预期 ≈14.9G）+ podman dangling prune before/after system df（预期 ≈5.18GB / Images 37→33）——RT-6 交付「安全清理流程」:228 对照。
5. **归属报表**：镜像 37 / 容器 5 / 卷 13 三张归属表（容器引用/创建时间/尺寸列；Distrobox 与停容器与卷零触碰面标注——:194 分别计量词形）。
6. **docs/phase0/RT-5-6-DISK-GOVERNANCE.md**：双 target 布局与 CARGO_TARGET_DIR 用法 / L0-L2 分层策略 / btrfs sudo 步骤登记（balance/scrub 命令原文 + 执行建议，留用户）/ podman 归属治理流程 / 告警阈值建议初稿（按镜像族实测重定——:196 词形）五节。
7. **docs/phase0/RT-5-6-BATCH-REPORT.md + docs/README.md 索引**：判据 1-12 全部数字命令回显入报告 + 坏消息单列（A/B 不达阈值记录 / dangling 归属存疑项 / probe 环境限制）。
8. **下一批输入（观察池登记，本批零实现）**：①fmt 全仓收敛独立 chore 批（D3——119 文件 + 链尾十六钉 4 交集 + 跨轨禁改面 3 交集，须链尾钉值面同步评审窗口 + 跨轨零 diff 豁免词形）；②RT-7 CI 与发布策略核验（:229——原生构建矩阵 + 完整测试/Clippy 保持，CI 面零改动延续）；③deps 层旧 fingerprint 判定批（L2 报告附录承接——RT-0 :12-14 词形，需逐文件安全判定流程立项）；④btrfs balance 执行（sudo 步骤文档承接——用户裁量执行时点）；⑤镜像族告警阈值定稿（DISK-GOVERNANCE 初稿 → 按族实测数据定值——:196 词形）；⑥ZAP 镜像面复测（WSW-7 选项 B 环境恢复后——dangling 回收记录为复测输入）。
