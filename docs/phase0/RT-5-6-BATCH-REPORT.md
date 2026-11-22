# RT-5/6 合并批验收报告 · Cargo Profile A/B + 磁盘治理 + Podman 资源与镜像预算

| 项 | 值 |
|---|---|
| 批次 | RT-5 + RT-6 合并批（构建与容器预算批；零迁移零 Rust src 改动） |
| 日期 | 2026-11-23 |
| 任务书 | [RT-5-6-TASK.md](RT-5-6-TASK.md)（D1-D5 / A1-A12 / S1-S6 / 判据 12 条 / 八红线） |
| 基线 HEAD | `2eaa19fe`（WSW-7 收口 7a82804a + CI 修复批在链；工作树 1 行 = 任务书自身未跟踪，落稿态） |
| 收口结论 | **debug=1 维持默认（Cargo.toml 零 diff）**：A/B 磁盘节省 34.08% < 40% 阈值（D1④ 条件 1 否决）；磁盘面回收 L1 incremental **14571MiB** + repo target 净缩 0.8G；podman dangling 实删 3 个（物理 +2GB），2 个被 orphaned working container 引用零强删（fail-safe 判例入档） |
| 收口门 | workspace 全量 `cargo test --offline --workspace --no-fail-fast` = **1315 passed 0 failed**（基线精确延续）+ clippy -D warnings 0 error + deny 双检 ok + fmt 935/119 零扩大 |
| 工作树 | 5 行 = §2.1 落点 4 面（见 §1）+ 任务书自身；**零 git add/commit**（工作树留主会话审查收口） |

---

## §1 改动文件清单（路径 + 行数）

| 文件 | 类型 | 行数 | 说明 |
|---|---|---|---|
| `scripts/disk_gc.sh` | 新建 | 214 行 | D2 三层 + D4 dangling 四模式治理脚本（零 sudo；dry-run 默认；全局面词形硬编码拒绝分支） |
| `docs/phase0/RT-5-6-DISK-GOVERNANCE.md` | 新建 | 118 行 | 运行手册五节（双 target 布局 / L0-L2 / btrfs sudo 登记 / podman 归属流程 / 阈值初稿） |
| `docs/phase0/RT-5-6-BATCH-REPORT.md` | 新建 | 本档 | 判据 1-12 实测对照 + 坏消息 + 观察池 |
| `docs/README.md` | additive 修改 | +2 行（93 行总） | 索引登记两份新档（RT-5/6 行插入目标与设计表，WSW-7 §9 先例） |
| `Cargo.toml` | **零 diff** | 0 行 | D1④ 否决分支——维持默认（`git diff Cargo.toml` = 0 行实测） |

工作树终态 `git status --porcelain`（4 行 + 本档落盘后 5 行）：

```
 M docs/README.md
?? docs/phase0/RT-5-6-DISK-GOVERNANCE.md
?? docs/phase0/RT-5-6-TASK.md      （任务书自身，侦察落稿态未跟踪，非本批产出）
?? docs/phase0/RT-5-6-BATCH-REPORT.md
?? scripts/disk_gc.sh
```

零 diff 反证：`git status --porcelain -- crates/ migrations/ vendor/ Cargo.lock web/ integrations/rtis/` = **0 行**；`git diff Cargo.toml` = **0 行**（`[profile.release]` strip 全段零触碰实证）；Cargo.lock `[[package]]` = **399**；migrations = **30**。

> 环境纪律注：任务书 S6 写「显式 add 本批清单」，本会话按上级纪律收紧为**零 git add/commit/push**——工作树留主会话审查收口，清单即上表。

## §2 S1 门禁基线（命令实测）

| 命令 | 预期 | 实测 |
|---|---|---|
| `git status --porcelain \| wc -l` | 0（任务书词形）/ 1（任务书自身在位） | **1**（`?? docs/phase0/RT-5-6-TASK.md`——任务书落稿态，非脏树） |
| `git log --oneline -1` | 2eaa19fe | `2eaa19fe WSW-7 CI 修复批二…` |
| `grep -cE '^\[\[package\]\]' Cargo.lock` | 399 | **399** |
| `ls migrations/*.sql \| wc -l` | 30 | **30** |
| `du -sh target/ /home/emo/rt3-target` | 4.8G / 64G（A5） | **4.8G / 64G**（四层逐项吻合：deps 3.7G+50G / incremental 909M+14G / build 155M+352M / release 14M+303M） |
| `podman system df` | A8 口径 | Images 37/9.597GB（可回收 6.078GB）/ Containers 5/11.74GB / Volumes 13/568MB（可回收 410.7MB）——逐项吻合 |
| `df -h /media/emo /home/emo` | A7 | 277G / 239G 可用（42% / 49%）——逐项吻合 |
| `cargo fmt --all -- --check` | 119 文件 / 935 hunks（A10） | **935 hunks / 119 文件**（/tmp/rt5_fmt_before.txt 快照） |
| `CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast` | 1315 passed 零红 | **EXIT=0，1315 passed / 0 failed** |

## §3 S2 Profile A/B probe（D1③：环境变量 + 独立 rt5-probe target-dir；主 Cargo.toml 零改动）

命令原文（A/B 同命令，仅 env 差异）：

```bash
# A 面（现状默认）
t0=$(date +%s.%N); CARGO_TARGET_DIR=/home/emo/rt5-probe-a cargo build --offline --workspace --tests
# B 面（debug=1）
t0=$(date +%s.%N); CARGO_TARGET_DIR=/home/emo/rt5-probe-b CARGO_PROFILE_DEV_DEBUG=1 cargo build --offline --workspace --tests
# 热重建：同命令第二次；测试代表面：cargo test --offline -p tool-engine -- --test-threads=1（同 env）
```

rustc/cargo：rustc 1.99.1 (58cf8f9c 2026-10-13) / cargo 1.99.1（A 面 Compiling 345 包冷起）。

### 四列对比表

| 维度 | A 面（默认 debuginfo 全量） | B 面（debug=1） | Δ |
|---|---|---|---|
| **磁盘（du probe 目录）** | **19074 MiB**（deps 16689 / incremental 2158 / build 218） | **12574 MiB**（deps 10893 / incremental 1455 / build 218） | **节省 6500 MiB = 34.08%**（deps 层 34.7%） |
| **冷构建（cold）** | **62.4 s**（EXIT=0） | **50.8 s**（EXIT=0） | −11.6 s（−18.6%） |
| **热重建（hot incremental）** | **0.28 s**（EXIT=0，全缓存） | **0.20 s**（EXIT=0，全缓存） | −0.08 s |
| **诊断能力** | rp-wsw 222498032 B 含 .debug_info+debug_line；`RUST_BACKTRACE=1` 回溯帧全带 `at src/main.rs:行:列` | rp-wsw 135471848 B（−39.1%）；serde.o 324K→212K；**回溯帧 file:line 与 A 面逐行等价**（.debug_line 保留，line-tables-only 形态） | 行号回溯能力零损失；完整变量级 DWARF（调试器变量检视）B 面缺失 |

readelf 抽样（判据 1 双面入档）：

```
A 面 rp-wsw:  [35] .debug_info  [39] .debug_line   （体积 222498032）
B 面 rp-wsw:  [34] .debug_info  [38] .debug_line   （体积 135471848，段在但显著缩小）
A 面 serde.o: .debug_info + .debug_line（324K）   B 面 serde.o: 同段位（212K）
```

probe 词形核实两件：①`CARGO_PROFILE_DEV_DEBUG=1` 生效 = B 面产物体积/段尺寸全面缩小（rp-wsw −39.1%）；②test profile 继承 dev = B 面 tool_engine 测试二进制 48106040 B vs A 面 76965000 B（env 变量传导至 test 目标编译），`cargo test` 同 env 下 Finished `test` profile（test 继承 dev 的 debug=1）。

测试代表面（`cargo test --offline -p tool-engine -- --test-threads=1`）：A 60.8s 全绿（纯测试 41.1s）/ B 59.0s 全绿（纯测试 40.8s）——套件结果逐项一致（54+11+8+… 全 0 failed）。

probe 清理反证：`rm -rf /home/emo/rt5-probe-a /home/emo/rt5-probe-b` 后 `ls /home/emo/rt5-probe*` = **No such file or directory**。

## §4 S3 debug=1 决策记录（D1④ 三条件写死判定）

| 条件 | 阈值 | 实测 | 判定 |
|---|---|---|---|
| ① 磁盘节省 | ≥40% | **34.08%**（19074→12574 MiB） | **不成立** |
| ② workspace 全量测试零红 | 1315 基线延续 | 基线 1315 passed 零红；B 面测试代表面全绿 | 成立（未触发落点复验） |
| ③ backtrace 诊断能力 | 实测可接受 | 回溯帧 file:line 两面逐行等价 + .debug_line 双面在位（readelf 入档） | 成立 |

**决策 = 维持默认**：条件 ① 不成立（34.08% < 40%），按 D1④「任一不成立 → 主 Cargo.toml 零 diff + 报告登记维持默认 + 复测条件词形」执行。`git diff Cargo.toml` = 0 行（判据 2 双形态之维持默认形态）。

复测条件登记（任一满足时重新 A/B）：
1. **口径扩展**：以 rt3-target 累积态口径复测（当前 deps 50G 含多代次 fingerprint，debuginfo 占比高于单代次 probe 态——probe 口径 34.08% 系单代次冷构建的**下界**）；需先立 deps 判定批（观察池③）清至单代次后复测。
2. **工具链演进**：rustc 实装 `debug = "line-tables-only"` 显式词形或 profile 压缩面（split-debuginfo 等）演进后复测。
3. **阈值复核**：若磁盘压力回升（/home 可用 < 100G），阈值面随 TOOL-RUNTIME-ADR 复审重议。

> A/B 洞察入档：deps 层 saving 34.7% 主要来自 .debug_info 缩减；incremental 层 saving 32.6%（2158→1455MiB）；build 层零差（218=218）。B 面冷构建反快 18.6%（debuginfo 生成省时）。维持默认下 probe 数字已入档，后续批可直接引用。

## §5 S4/S5 磁盘与容器治理实测

### 5.1 L1 incremental 老化（`disk_gc.sh gc-incremental --apply`）

```
待清 13648MiB  /home/emo/rt3-target/debug/incremental
待清   923MiB  /media/emo/code/pensting/research-platform/target/debug/incremental
预期回收合计 14571MiB
已清 rt3-target：before=13648MiB after=0MiB 回收=13648MiB
已清 repo target：before=923MiB after=0MiB 回收=923MiB
```

**回收实测 = 14571MiB（≈14.2GiB）**（预期 ≈14.9G 达成，du 口径）。df 佐证：/home 229G→221G used（239G→247G avail）。再生反证：repo target 裸 `cargo check --offline -p wsw-message` 后 `ls target/debug/incremental/` 出现 wsw_message/database/tool_engine 等再生 fingerprint；rt3-target 收口全量测试后再生 425M。治理后 du：repo target 4.0G / rt3-target 51G。

L2 `report-deps`（报告级零删除）：rt3-target deps **50G / 4877 文件**，同名前缀多 hash 变体 top：libtool_engine 28 代次 / libdatabase 28 / database 26 / libtool_registry 25 / tool_engine 24 / wsw_runtime 21 / rp_wsw 21…；层内最大文件 rp_agent 137M / rp_plugin 91M / 各 e2e 载具 88M 级。repo deps 3.7G / 803 文件。全量附录在批次执行日志 `/tmp/rt5_gc_report_deps.log`（本档零删除延续 RT-0 :14 词形）。

### 5.2 Podman dangling-only（`disk_gc.sh podman-prune-dangling --apply`）

before/after：

| 口径 | before | after | Δ |
|---|---|---|---|
| `podman system df` Images TOTAL | 37 | **17** | −20（3 dangling + 中间层） |
| Images SIZE / RECLAIMABLE | 9.597GB / 6.078GB (63%) | **8.198GB / 4.679GB (57%)** | **−1.399GB** |
| 物理 `df -h /media/emo`（GraphRoot 同盘） | 199G used / 277G avail | **197G used / 279G avail** | **物理 +2GB** |
| dangling 数 | 4（名义 ≈5.18GB） | **2**（名义 ≈1.15GB） | 实删 3 |

实删 3 个（引用零命中 + 时间线双证通过）：`63c8f87076ed` / `08ea1a14ac1c` / `d4299b261398`（1.37GB×3，3 天前 = WSW-7 S3 ZAP 构建尝试层；创建窗与 WSW-7 批期 S3→S7 跨 3 天吻合，rip-nuclei 1h 前 = S7 重建）。**名义 4.11GB → 物理 2GB**：三镜像共享 base 层去重——回收数字以物理 df 与 df SIZE 双口径入档，名义尺寸不等于物理回收（判例）。

剩余 2 个零强删（fail-safe 判例）：`78aa746f2843`（77.9MB）被 external working container `35ecc1b90a56`（名 `78aa746f2843-working-container`）引用；`d9726c30e0ae`（1.07GB）被 `08e4458d48be`（名 `d9726c30e0ae-working-container`）引用。`podman ps -a` 不可见（Storage 态 external），首跑 `podman image prune -f` 引擎侧 loud 拦截（`Error: image used by 35ecc1b90a56…: image is in use by a container`），复跑静默跳过（exit 0 零删除）——**红线 5「零容器引用」条件不满足 = 零执行正确面**；引用核查口径已修入脚本（命名词形 `<id>-working-container` 收编，保守误报向安全侧）。

归属报表（三表分别计量；细表见执行日志 /tmp/rt5_ownership_images.tsv）：

- **镜像表（13 顶层）**：rip-nuclei 935MB（S7 重建 1h）/ rip-pg17 1.13GB（8d，引用 rip-p17-restore/rip-pg17/rip-p17-ma3-test 三容器）/ pkgbuild-builder 2.58GB / ubuntu-zcode 2.34GB（引用 ubuntu Distrobox）/ kali-rolling 125MB（引用 kali）/ 上游 nuclei 935MB / alpine 8.73MB / python:<none> 124MB / debian 78MB + dangling 面。零引用面 = 4 dangling（before 口径）。
- **容器表（5 个）**：ubuntu Up / kali Up（Distrobox 零触碰）+ rip-p17-restore Exited(0) 6d / rip-pg17 Exited(135) 5d / rip-p17-ma3-test Exited(0) 8h（MA-3 证据面只报不删）。
- **卷表（13 个）**：157.3MB / 137.4MB / 136.9MB / 136.4MB 四大卷 + 零星 0B-127B；410.7MB 可回收只报归属零删除。

零触碰反证（before/after 逐项一致）：`podman ps -a --format "{{.Names}} {{.Status}}"` diff = **零**（5 容器一致，Distrobox Up 态未断）；`podman volume ls` = **13 不变**；带标签镜像 diff = 仅 2 行 `<none>:<none>` 退出顶层清单（即实删 2 + 1 个新提升 dangling），8 个干净标签镜像全在位。

### 5.3 脚本面（`scripts/disk_gc.sh` 四模式）

| 命令 | exit | 结果 |
|---|---|---|
| `report` | 0（幂等复跑 0） | 双 target 四层 + df + podman df + dangling 归属标注 |
| `gc-incremental`（无 --apply） | 0 | DRY-RUN 零删除 |
| `gc-incremental --apply` | 0 | 14571MiB 实收（§5.1） |
| `report-deps` | 0 | L2 附录零删除 |
| `podman-prune-dangling`（无 --apply） | 0 | 归属双证 + DRY-RUN 拒绝执行 |
| `podman-prune-dangling --apply`（引用在位时） | 2 | `REJECT: dangling 镜像 … 被容器引用——红线 5 拒绝执行` |
| `report -a --volumes`（禁词形注入） | 2 | `REJECT: 命中全局面词形（--volumes）——硬编码拒绝` |
| `grep -n "prune -a" scripts/disk_gc.sh` | 1（零命中） | 禁全局 prune 词形零命中反证；拒绝分支词形 `硬编码拒绝` grep 命中（另行 grep 面） |

脚本带 in-flight 守卫（cargo/rustc 进程在跑时 `--apply` 拒绝）与 external 引用核查（2026-11-23 判例修入）。

## §6 判据 1-12 逐条状态

| # | 判据 | 状态 | 实测锚点 |
|---|---|---|---|
| 1 | A/B 对比数字表 + probe 清理反证 | **过** | §3 四列表（磁盘/冷构建/热重建/诊断，GiB·s 双面）+ `ls /home/emo/rt5-probe*` = No such file |
| 2 | debug=1 决策记录 | **过**（维持默认形态） | 三条件逐项判定词形（§4）；`git diff Cargo.toml` = 0 行 |
| 3 | 磁盘回收数字（target 面） | **过** | gc-incremental --apply before/after du = 14571MiB（13648+923）；再生双反证（§5.1） |
| 4 | Podman 回收数字 | **过**（部分，坏消息 §8-2） | before/after system df（37→17 / 9.597→8.198GB / 物理 +2GB）；dangling 4→2（预期 37→33 为 df TOTAL 口径误设——见 §8-2） |
| 5 | 归属报表与零触碰面 | **过** | 三表（§5.2）；5 容器 before/after diff 零 + 13 卷不变 + Distrobox Up 未断 |
| 6 | 脚本可运行 | **过** | report exit 0 幂等 + dry-run 零删除 + --apply 缺失拒绝 + 禁词形 REJECT exit 2 + `prune -a` 零命中（§5.3） |
| 7 | Profile 构建验证 | **过**（维持默认态） | `CARGO_TARGET_DIR=/home/emo/rt3-target cargo build --offline --workspace` exit 0（收口测试内建构建面）；全量 1315 passed 零红 |
| 8 | 零迁移与族基线语义不动 | **过** | migrations 30 零 diff + Cargo.lock 399 + vendor 零 diff + `git status --porcelain crates/` = 0 行（含 core_utils_e2e.rs / podman_host.rs 断言面零改动） |
| 9 | 生产库只读指纹 | **过**（词形修正，坏消息 §8-3） | 五文件 mode=ro 实测 `user_version` = 1/1/1/0/0（platform/dryrun/fullbatch=1，governance/search=0）；任务书「=30」为侦察误口径——30 系 selftest_wsw56.sh :59-60 **scratch DB** 判据 3 水位，生产库先例 ：628 恰为 `1`；mtime 全未动（11月6-7日）= 零写面实证 |
| 10 | fmt 存量红零扩大 | **过** | 935 hunks / 119 文件（与基线逐值相等；新建 shell/doc 不入 fmt 面） |
| 11 | 文档面 | **过** | DISK-GOVERNANCE 五节齐 + BATCH-REPORT 本档 + docs/README.md 两行索引 |
| 12 | 回归与收口 | **过**（1 项偏差单列 §8-4） | workspace 全量 1315 零红 + clippy -D warnings **0 error** + deny advisories/licenses 双 **ok** + selftest_wsw7 PASS=32 SKIP=4 **FAIL=2**（陈旧钉与本批零因果，worktree HEAD 纯净基线同值实证——§8-4）+ `git status --porcelain` 仅 §2.1 落点 |

## §7 收口命令清单（§5 逐条实测汇总）

| 命令 | exit | 关键输出 |
|---|---|---|
| `CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast`（基线 + 收口各一次） | 0 / 0 | 两次均 **1315 passed / 0 failed**（收口 93.4s，incremental 再生后） |
| `CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline --workspace --all-targets -- -D warnings` | **0** | `Finished dev profile … 0 error` |
| `cargo fmt --all -- --check` | 1（存量红语义） | 935 hunks / 119 文件 = 基线逐值相等（零扩大） |
| `cargo deny check advisories && cargo deny check licenses` | 0 / 0 | `advisories ok` / `licenses ok` |
| `grep -cE '^\[\[package\]\]' Cargo.lock` | 0 | **399**（S1 对比恒定） |
| `bash scripts/disk_gc.sh report` | 0 | 幂等（复跑 0） |
| `bash scripts/disk_gc.sh gc-incremental --apply` | 0 | 回收 14571MiB |
| `bash scripts/disk_gc.sh podman-prune-dangling` | 0 | DRY-RUN 拒绝执行 |
| `bash scripts/disk_gc.sh podman-prune-dangling --apply` | 0（首跑）→ 2（引用在位复跑） | 首跑实删 3 dangling + 引擎拦截 1；引用核查修复后 REJECT exit 2 |
| `podman system df` | 0 | 17 / 8.198GB / 4.679GB (57%) |
| `podman ps -a --format … \| wc -l` | 0 | **5**（零触碰反证） |
| `podman volume ls --format … \| wc -l` | 0 | **13**（零触碰反证） |
| `grep -n "prune -a" scripts/disk_gc.sh` | 1 | 零命中 |
| 五文件 `sqlite3 "file:$f?mode=ro" "PRAGMA user_version"` | 0 | 1 / 1 / 1 / 0 / 0（§6-9 词形修正） |
| `git status --porcelain -- crates/ migrations/ vendor/ Cargo.lock web/ integrations/rtis/ \| wc -l` | 0 | **0** |
| `RP_WSW_BIN=/home/emo/rt3-target/debug/rp-wsw bash apps/server/selftest_wsw7.sh` | **1** | PASS=32 SKIP=4 FAIL=2（陈旧钉 §8-4） |
| `ls /home/emo/rt5-probe*` | — | No such file or directory |

## §8 坏消息单列

1. **A/B 阈值未达**：磁盘节省 34.08% < 40%（D1④ 条件 1 否决）→ debug=1 不落点。probe 口径 = 单代次冷构建下界；rt3-target 累积态口径下 saving 预计更高（deps 50G 多代次 debuginfo），但 D1④ 未授权替代口径——按写死阈值执行维持默认。复测条件三条登记（§4）。
2. **dangling 预期口径两处偏差**：①预期「Images 37→33」按 df TOTAL 计——实测 37→17（dangling 删除连带中间层一并回收，prune 语义使然，实际回收优于预期词形）；②名义 ≈5.18GB 中实收物理 **+2GB**（3×1.37GB 共享 base 层去重，名义尺寸≠物理回收）。剩余 2 个 dangling（≈1.15GB 名义）被 orphaned working container 引用——本批零强删（红线 5），回收未全额达成。
3. **任务书判据 9 词形误设**：「五文件 user_version = 30」为侦察误口径——生产库真实水位 = 1/1/1/0/0（`selftest_wsw56.sh :628` 先例恰期望 platform.db=1；"30" 属 ：59-60 scratch DB 判据 3）。本批按先例真实口径执行 + mtime 零动实证，判据 9 实质（只读零写指纹）达成，词形偏差在此登记。
4. **selftest_wsw7.sh 收口后陈旧钉 2 FAIL**：`apps/server/selftest_wsw7.sh:292-293` 检查「`git diff Cargo.lock` = wsw-runtime 依赖块 3 行」——该 diff 已随 WSW-7 收口 commit `7a82804a` 落地恒为 0，HEAD 纯净基线同败（worktree `/tmp/rt5_wsw7_head` 复算 LOCK_DIFF=0/LOCK_DIFF_ALL=0 实证，worktree 已清）。与本批零因果（本批 Cargo.lock 零 diff）；性质同 selftest_wsw4.sh 28 陈旧钉「已知红」类；红线 6 禁顺手改钉值——登记观察池⑧留独立修复批。
5. **probe 环境限制**：panic 诊断能力载体 = /tmp 独立小 crate proxy（workspace 内零 panic 载具可用，crates/** 零 diff 纪律禁临时加测）；proxy 法（RUST_BACKTRACE=1 回溯帧行号对照）+ workspace 产物 readelf 双面取证，结论可信度已双锚。
6. **脚本初版缺陷（已修）**：`report-deps` 首版 `head` 早闭管道 × `pipefail` → exit 141；改 awk 窗口（NR≤N 读满不早闭）后 exit 0。归属报表首版整串匹配误标引用列 → 改逐容器精确比对。两缺陷均在收口前修复并复验。

## §9 未解决项与观察池登记（本批零实现）

| # | 项 | 承接 |
|---|---|---|
| ① | fmt 全仓收敛独立 chore 批（119 文件 + 链尾十六钉 4 交集 + 跨轨禁改面 3 交集，须链尾钉值面同步评审窗口 + 跨轨零 diff 豁免词形 + 链尾 owner 会签） | 任务书 §8① |
| ② | RT-7 CI 与发布策略核验（:229 原生构建矩阵 + 完整测试/Clippy 保持，CI 面零改动延续） | 任务书 §8② |
| ③ | deps 层旧 fingerprint 判定批（L2 报告附录承接：50G / 4877 文件 / 单前缀最高 28 代次——需逐文件安全判定流程立项） | 任务书 §8③ |
| ④ | btrfs balance/scrub 执行（sudo 步骤 §3 登记，用户裁量执行时点） | 任务书 §8④ |
| ⑤ | 镜像族告警阈值定稿（DISK-GOVERNANCE §5 初稿 → 11 族全量建成实测后定值） | 任务书 §8⑤ |
| ⑥ | ZAP 镜像面复测（WSW-7 选项 B 环境恢复后——dangling 回收记录为复测输入） | 任务书 §8⑥ |
| ⑦ | **本批新增**：orphaned buildah working container 清理评审面（7 个 Storage 态，`podman ps -a --external` 定名清单在案；确认归属后移除 → 复跑 dangling prune 预期再收 ≈1.15GB 名义） | §5.2 判例承接 |
| ⑧ | **本批新增**：selftest_wsw7.sh :292-293 收口后陈旧钉修复批（「Cargo.lock diff=3 行」检查改为条件化或按收口态重钉；须 WSW-7 owner 会签——与 selftest_wsw4 28 钉同族） | §8-4 承接 |

## §10 回滚面

- 文件级：`git checkout HEAD -- docs/README.md` + `rm scripts/disk_gc.sh docs/phase0/RT-5-6-DISK-GOVERNANCE.md docs/phase0/RT-5-6-BATCH-REPORT.md`（工作树丢弃即净回滚；零 commit 在链）。
- Cargo.toml：零 diff 无回滚面。migrations/vendor/crates/web 零 diff 无回滚面。
- L1 incremental：零回滚面（再生缓存，下次构建自动重建）。
- podman dangling 3 镜像删除不可逆：兜底 = rip-nuclei（containerfiles/rip-nuclei/Containerfile :19 digest 钉复建）/ 上游 nuclei（拉取面）/ ZAP（选项 B 未建成零损失）——删除对象全部为无标签无引用面，零业务回滚风险。
- 生产库/Distrobox/停容器/卷：零操作无回滚面；btrfs balance 未执行零回滚面。
