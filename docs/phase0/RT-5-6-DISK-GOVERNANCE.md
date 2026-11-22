# RT-5/6 磁盘与容器预算治理 · 运行手册

| 项 | 值 |
|---|---|
| 状态 | 生效中（初稿；镜像族告警阈值定稿见 §5 与观察池⑤） |
| 日期 | 2026-11-23 |
| 依据 | NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md :194-196,227-228,241,244；RT-0-RUNTIME-BASELINE-AUDIT.md :11,14,16,41,71,148；PHASE-ROADMAP.md :775,789-790；TOOL-RUNTIME-ADR.md :197,217,234；RT-5-6-TASK.md D2/D4/D5 |
| 载具 | `scripts/disk_gc.sh`（零 sudo；dry-run 默认；`--apply` 显式） |
| 红线 | 全程零 sudo；禁 `podman system prune -a --volumes` 全局面；禁 tmpfs target（RT-0 :71）；deps 层 50G 判定 = 报告级零删除（L2）；Distrobox/停容器/卷零触碰 |

---

## §1 CARGO_TARGET_DIR 双 target 布局与用法

本仓存在两个并存的 cargo target 目录，分属两块物理盘，治理动作必须分别归因（NATIVE-FIRST :194「分别计量不用单一总数代替归因」词形）：

| target 目录 | 物理盘 | 2026-11-23 实测 | 角色 |
|---|---|---|---|
| `<repo>/target/` | /media/emo（btrfs，/dev/nvme0n1p1，477G 总 / 277G 可用） | 4.8G（deps 3.7G + incremental 909M + build 155M；release 14M） | 裸 `cargo` 命令默认落点（.cargo/config.toml 未设 target-dir 键） |
| `/home/emo/rt3-target` | /home（/dev/nvme1n1p2 **独立盘**，473G 总 / 239G 可用） | 64G（deps 50G + incremental 14G + build 352M；release 303M） | 批测 `CARGO_TARGET_DIR` 承载位（DI-3/WSW 系批词形），deps 50G = 磁盘病理首要对象（RT-0 :11 口径） |
| `/home/emo/rt5-probe*` | /home 同上 | 19G / 13G（A/B probe 各一，用后即清——RT-0 :71「独立、稳定 SSD 路径」词形） | Profile A/B probe 专用，**禁**复用主 target（D1③） |

用法词形：

```bash
export CARGO_TARGET_DIR=/home/emo/rt3-target   # 批测/收口门统一落点（Rust 命令另加 --offline）
cargo test  --offline --workspace              # 走 rt3-target
cargo build                                    # 裸命令走 repo target/（小改动本地构建）
```

- **tmpfs target 方案零引入**（RT-0 :71 禁词形）：target 必须落在独立 SSD 路径，不落内存盘。
- **A/B probe 纪律**：probe 用独立 `rt5-probe*` 目录 + `CARGO_PROFILE_DEV_DEBUG=*` 环境变量，主 Cargo.toml 在 A/B 数据落地前零改动（NATIVE-FIRST :227/:241「先 probe 后改配置」「没有未验证就改 Profile」）；probe 数据入档后 `rm -rf /home/emo/rt5-probe*` 清理。
- **双 target 分盘归因**：/media/emo 是 btrfs（仓库 + podman GraphRoot 同盘）；/home 是独立盘。清 /home 的 rt3-target 只缓解 /home 水位；btrfs 面动作见 §3。

## §2 分层清理策略 L0-L2（全部零 sudo）

载具：`bash scripts/disk_gc.sh <模式>`（dry-run 默认；`--apply` 才动删）。

| 层 | 模式 | 动作 | 语义 | 2026-11-23 实测 |
|---|---|---|---|---|
| L0 | `report` | 双 target 四层 du 表 + df + podman system df + dangling 归属表（引用核查随行） | 只读报表，exit 0 幂等 | 基线 4.8G + 64G；podman Images 37/9.597GB |
| L1 | `gc-incremental --apply` | 清空 `<target>/debug/incremental/` | **再生缓存零语义损失**（下次构建自动重建，唯一代价 = 首次增量构建时间）；cargo 不自动回收旧 incremental（RT-0 :12-14 词形） | rt3-target 13648MiB + repo 923MiB = **回收 14571MiB**（≈14.2GiB） |
| L2 | `report-deps` | deps 层旧 fingerprint 残留报表（同名前缀多 hash 变体视图 + 层内最大文件） | **报告级禁删除**——本轮延续 RT-0 :14「没有把任何文件判定为可安全删除」，deps 主产物零删除；判定面留批次报告附录 | rt3-target deps 50G / 4877 文件，单前缀最多 28 hash 代次（libtool_engine） |
| btrfs | （本文档登记，零执行） | balance / scrub / 细粒度 usage | 需 root = 病理修复面非应用面，步骤见 §3，执行与否由用户裁量 | 无 root 警告实录「cannot read detailed chunk info…run as root」 |

L1 执行纪律：脚本内置 cargo/rustc 进程守卫（检测到在途构建即拒绝 `--apply`）；`report`/`report-deps`/dry-run 模式零删除动作。

L2 未来立项判据（观察池③承接）：需逐文件安全判定流程（fingerprint 代次 → 当前 lock 指纹在用性比对），**禁**按前缀数量拍脑袋删。

## §3 btrfs 面 · sudo 步骤登记（留用户执行，本批零 sudo）

以下命令均需 root，本会话零执行（D2④）；按需裁量执行时点。

```bash
# 1) 细粒度 usage（无 root 只能看到 Device/allocated 概数，detailed chunk info 需 root）
sudo btrfs filesystem usage /media/emo

# 2) balance（data chunk 碎片回收；先 dry-run 语义的 usage 判读再决定 usage 值）
sudo btrfs balance start -dusage=50 /media/emo    # 温和档，首选
sudo btrfs balance start -dusage=75 /media/emo    # 激进档，温和档收益不足时
sudo btrfs balance status /media/emo              # 进度查看（balance 可长时间占用 IO）

# 3) scrub（数据校验，例行健康面，与容量治理无直接关系）
sudo btrfs scrub start -B /media/emo              # -B 前台一次跑完
sudo btrfs scrub status /media/emo
```

执行时点建议：
- balance：磁盘水位显著抬升后（如 /media/emo 可用跌破 200G）或每月例行；避开构建/测试高峰（balance 占 IO）。
- 先 `usage` 判读 Data,single used% 与 chunk 碎片度，再定 `-dusage` 档位；本机 2026-11-23 无 root 口径实测 Data,single 305.91GiB used 64.23%（RT-5-6-TASK A7）。
- scrub：每月一次即可；与 balance 不同日执行。

## §4 Podman 归属治理流程（dangling-only；禁全局面 prune）

归属三表（镜像/容器/卷分别计量，NATIVE-FIRST :194 词形）由 `bash scripts/disk_gc.sh report` 产出；细粒度归属列（每镜像的容器引用/创建时间/尺寸）见 RT-5-6-BATCH-REPORT.md §S5。

执行序列（安全清理流程，RT-6 交付 :228 词形）：

1. **归属报表先行**：`disk_gc.sh report` → dangling 清单逐 ID 标注引用命中态。
2. **归属双证**：①引用零命中（常规容器 `podman inspect -f '{{.Image}}'` + external working container 命名词形 `<id>-working-container`——`podman inspect` 对 external 容器报 no such object，不可依赖）；②创建时间线对齐 RIP 构建活动（如 WSW-7 S3 ZAP 构建尝试窗）。
3. **dangling-only 执行**：`disk_gc.sh podman-prune-dangling --apply`（内部 `podman image prune -f`；引用命中即 REJECT exit 2）。**最终裁判 = podman 引擎自身**——被 external 容器引用的 dangling 由引擎静默跳过/拦截，脚本不绕过。
4. **before/after 入档**：`podman system df` 双快照 + 物理 `df -h /media/emo`（GraphRoot `/media/emo/containers/storage` 与仓库同盘——镜像治理直接改变 /media/emo 水位）。

零触碰面（红线 5 / D4③，违反即停批）：
- ubuntu/kali 两个 Distrobox 容器及其镜像（RT-0 :16/:133「非 RIP 工具容器」词形）；
- 已停 rip-pg17 系容器（MA-3 测试证据面，只报不删）；
- 全部卷（零删除——2026-11-23 可回收 410.7MB 只报归属）；
- 带标签镜像（rip-nuclei/rip-pg17/上游 nuclei/pkgbuild-builder/ubuntu-zcode/alpine/python/debian/kali-rolling）；
- **禁** `podman system prune -a --volumes` 全局面（脚本硬编码拒绝分支：独立 `-a` 旗标或 `--volumes` 词形 → REJECT exit 2）。

2026-11-23 实测纪要（后续治理沿用此判例）：
- dangling 4 个（3×1.37GB + 1.07GB，名义 ≈5.18GB）→ 实删 3 个（零引用双证通过），剩余 2 个（77.9MB + 1.07GB）被 orphaned buildah working container 引用（`78aa746f2843-working-container` / `d9726c30e0ae-working-container`，Storage 态，`podman ps -a` 不可见）→ 零强删，登记复测条件。
- 名义尺寸 ≠ 物理回收：3×1.37GB 名义实收物理 ≈2GB（共享 base 层去重）——**回收数字以物理 df 与 `podman system df` SIZE 双口径入档**。
- working container 清理属独立评审面：`podman ps -a --external` 定名 → 逐个确认归属 → 移除 → 复跑 dangling prune（观察池⑦承接，本批零执行）。

## §5 告警阈值建议初稿（按镜像族实测重定；NATIVE-FIRST :196 词形）

历史值（TOOL-RUNTIME-ADR）：:197 镜像与缓存总量初始预算 **50 GiB**；:217 容器存储占家目录磁盘 50 GiB 封顶；:234 可用磁盘低于 50 GiB 复审阈值。50 GiB = 历史上限，按实测镜像族重定如下（**初稿**——定稿入观察池⑤）。

containerfiles/ 11 族（rip-embed / rip-gov / rip-nuclei / rip-parser / rip-pg17 / rip-rerank / rip-tool-base / rip-tool-coreutils / rip-tool-source / rip-tool-webresearch / rip-zap）在位实测（2026-11-23 清理后）：

| 面在位 | 实测 | 说明 |
|---|---|---|
| RIP 镜像族在位 | rip-nuclei 935MB + rip-pg17 1.13GB + 上游 nuclei 底座 935MB ≈ **3.0GB** | rip-zap = 选项 B 未建成零在位（WSW-7 D1）；其余 8 族零在位 |
| 非 RIP 底座 | pkgbuild-builder 2.58GB + ubuntu-zcode 2.34GB + kali 125MB + alpine/python/debian ≈0.21GB ≈ **5.2GB** | Distrobox 两镜像非 RIP（RT-0 :16 词形），单独归因 |
| podman 全面合计 | **8.198GB**（Images df SIZE，清理后） | df TOTAL 17（含中间层） |

初稿阈值（触发动作 = 先报告归属再清理，NATIVE-FIRST :195 词形）：

| 阈值 | 值 | 动作 |
|---|---|---|
| RIP 镜像族预算 | **10 GiB**（≈3.3× 当前在位，容纳 11 族全量建成 + 上游底座） | 超限 → 按族归属报表评审，只清无引用对象 |
| podman 全面预算 | **15 GiB**（Distrobox/底座计入） | 超限 → 归属报表 + dangling-only 执行 |
| podman 全面告警线 | **12 GiB**（80%） | 触发 `disk_gc.sh report` 审视 |
| 复审线 | /media/emo 可用 **< 50 GiB**（:234 词形沿用） | 停批上报复审存储策略 |
| 构建缓存（target 面） | 双 target 合计 **80 GiB** 观察线（当前 68.8G：64G + 4.8G − 治理后余量） | 触发 → L1 老化先行；L2 判定批立项 |

定稿条件：11 族全量建成实测后按族复核（观察池⑤）。
