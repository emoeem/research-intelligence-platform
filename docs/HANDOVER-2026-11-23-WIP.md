# 交接文档 · 2026-11-23 上午（Phase 6b 尾段在途，RIP3 主队列剩 3 项）

> 接手人必读：本文 + 记忆指针 `rip3-progress-pointer`（`~/.zcode/cli/memories/projects/pensting-4a408e0536a2c35f/memory/`）+ 红线固化稿 `docs/rip3/wsw/WSW-CONTINUATION-REDLINES.md`。状态仲裁：`git log` + 本文 + 实测命令。

## 一、总账（十七批全 CI 绿，HEAD=`deba165b` WIP 备份）

| 批次 | commit | 说明 |
|---|---|---|
| RT-3 Policy Resolver | `b53562a6` | 144 格矩阵 + N1-N10 + 缝 5 |
| MA-1 Caller 多密钥鉴权 | `fdd593f6` | 026 两表 + rp-admin 五子命令 |
| MA-2 并发派发引擎 | `76e0f559`+`d18b7dca` | 027 四表 + 三帽 + FamilyLimiter |
| CB-1 上下文折叠 | `ae23a9d8` | 零迁移（落 013 表）+ 缓存六桶归因 |
| WSW-4 Fuzzer | `d664ceef` | 028 两表 + JobRunner 四闸 |
| MA-3 协作协议+知识供体 | `86ea8530`+`d4827a4a`+`0f2922cb` | 029 + MCP 七工具 + PG V0007 |
| MA-4 SSE 推送 | `f1be3f4e` | SSE 三形态 cursor + web caller 列 + 四形态端到端 |
| DI-3/RT-4 迁移整合 | `aafb87e9` | 29 份 Manifest 全 assign + Resolver 接 dispatch |
| WSW-5+6 WS 工作台 | `d62e8b87` | 030 两表 + Decoder/Comparer/Passive |
| WSW-7 Scanner 集成 | `7a82804a`+`c962a8d7`+`2eaa19fe` | Nuclei/RTIS/ZAP 三 Adapter + zap 入册 |
| RT-5/6 构建预算 | `f8d38107` | A/B probe 维持默认 + 14.5GB 回收 |
| WSW-8 Browser Evidence | `dca07555` | chromiumoxide vendor 399→417 + 真实截图 |
| RT-7 CI/发布矩阵 | `f4c324b`+`8b79bd26` | timeout×4 + toolchain 钉 + 陈旧钉三件 |
| WSW-9+总 RELEASE | `8fb2d4c7`+`e5084326`+`1b5836f3` | RELEASE_2026-11-23_WSW-MVP（十条件绿 6/黄 4/红 0） |
| Phase 6a Report IR | `7ee11904` | SARIF 对齐 schema 冻结 + comrak vendor 417→432 |
| **Phase 6b（WIP）** | **`deba165b`** | **S1-S5 已落：轻量 PDF 四钉 vendor→440 + PDF/DOCX/SVG/QC 渲染器 + 46 测试绿；S6-S8 在途** |

**里程碑**：MA 轨收官（四形态全闭环）/ WSW 轨收官（总 RELEASE 入库）/ workspace 零基线红时代 / vendor+deny 政策实证（连续拦下 docx-rs、typst、6 个 DOCX crate 共 8 个漏洞链依赖）。

## 二、在途：Phase 6b 尾段（S6-S8）

- **已完成（`deba165b` 内）**：轻量 PDF 链四钉 exact 落库（`pdf-writer 0.15.0` + `subsetter 0.2.6` + `skrifa 0.42.1` + `crc32fast 1.5.5`，lock 432→440，vendor +7 目录）；`render/{pdf,docx,svg}.rs` + `fonts.rs` + `ooxml.rs` + `qc.rs` + `schemas/sarif/` 官方 schema 资产 + 4 测试文件（report-engine **46 passed**：6A 26 + 6B 20）
- **剩余步骤**：
  1. `migrations_pg/V0008__reports.sql`（reports 表：`report_id` rep- 前缀 PK / `task_id` / `report_ir` jsonb / `format_exports` jsonb / `created_at` + `idx_reports_task`——03-DOMAIN-MODEL.md:132-133 逐列；refinery 纪律 `intel-db/src/lib.rs:133-140` 唯一执行器）
  2. `crates/intel-db/src/report_repo.rs`（document_repo 同构；report-engine 纯函数零 I/O 不破）
  3. `apps/server/selftest_p6b.sh`（V-9 守卫预挂）
  4. 收口门：`cargo test --offline --workspace` 全量（**440 新基线**）+ clippy --workspace + deny 双检（advisories 须 ok）+ `cargo vendor --locked` tracked diff=0
  5. `docs/phase0/PHASE-6B-BATCH-REPORT.md` + `docs/README.md` 登记 → 显式 add → commit → push → CI
- **任务书**：`docs/phase0/PHASE-6B-TASK.md`（225 行 D1-D8/A1-A18/S1-S8/12 判据）

## 三、剩余批次（Phase 6b 收口后）

| 批次 | 规模 | 内容 |
|---|---|---|
| **Phase 7** | 小 | Finding→donor-submit 桥（MA-3 供体链已落）+ 失效链接/旧 POC 巡检 |
| **Phase 8** | 中 | 1k/10k/100k 语料合成器 + 九面基准报告（P4-4 基准载具已在） |

Phase 8 收口 = **RIP3 主队列走完**。

## 四、观察池（不阻塞主队列）

fmt chore 独立批（943 hunks/120 文件存量红，与链尾硬钉 4 文件+跨轨禁改 3 文件交集——禁夹带）/ ZAP 镜像 + nuclei 钉值镜像恢复批（源下架/不可回取，恢复路径在 WS​W-7 报告）/ btrfs balance（需 sudo，`docs/phase0/RT-5-6-DISK-GOVERNANCE.md`）/ 用户文档四类缺口 / Windows·macOS 未实测 / UI 六面 / 性能缺测三项 / orphaned container ≈1.15GB / api_key.last_used_at 写入面 / native·python 通道级 caller 帽 / SIGKILL 兜底收敛。

## 五、环境事实（本阶段新增，接手必读）

1. **vendor+deny 政策**（取代「Cargo.lock 399 恒定」）：新依赖必须 `cargo vendor` 全量入库 + deny 双检；lock 基线演进史 **399→417（WSW-8）→432（6a）→440（6b WIP）**；收口门写法 = lock 与 HEAD diff 允许 + vendor 完整性门 + deny。
2. **侦察必答项**：vendor 候选**必跑 deny advisories probe**（6a 停批教训——advisory DB 时点可查未查，docx-rs zip 链 6 条 RUSTSEC error 级停批；typst 链同类停批；DOCX 六候选全停→手写 OOXML）。
3. **CI 环境敏感三型**（新 e2e 一律防）：①系统包滚动（ffprobe/Chromium 版本漂移——probe 加 `--version` 可用性验证）②快机器预算检查点竞争（不用 `from_nanos(1)` 型假设——sleep wrapper 强制路径）③snap 壳假信号。另：assert 带 outcome 细节 / 测试 mod 置文件尾（items-after-test-module）。
4. **CI 面**：rust-toolchain.toml 钉 1.99.1 + timeout×4 + actions v5/v6；**绝对钉禁进 CI yml**（399 钉次日陈旧教训）；`rust-toolchain.toml` 在位时默认工具链 release 构建会漂移 binary_sha256 终钉——`RUSTUP_TOOLCHAIN=stable` 可字节级复现（manifest 钉值核对用此）。
5. **磁盘**：构建一律 `CARGO_TARGET_DIR=/home/emo/rt3-target`（/media/emo btrfs 近满盘 reserve_bytes 冻结病理，见记忆 `media-emo-btrfs-full-freeze`）；`scripts/disk_gc.sh` 可回收 incremental + dangling。
6. **配额**：09:00 周期重置；耗尽 → WIP 备份（前缀 `WIP 备份：`）→ 恢复后接续（本次 `deba165b` 实证）。
7. **并发上限 3**：workflow mc=2；大批实施走 Agent 主会话路径（workflow 长 turn 静默挂起三次实证）；代理撞墙恢复时给「已完成成果清单勿重做」可续。
8. gh api 间歇 EOF / SSH 推送间歇失败——重试 3-5 次，**push 重试必须用退出码判据**（`| tail && break` 会把 tail 成功当 push 成功）。

## 六、复现与接手命令

```bash
cd /media/emo/code/pensting/research-platform
git log --oneline -3
# 预期：deba165b WIP 备份（Phase 6b S1-S5）+ 7ee11904 Phase 6a + ...

git status --short          # 预期：空（WIP 已提交）
grep -c '^\[\[package\]\]' Cargo.lock   # 预期：440
export CARGO_TARGET_DIR=/home/emo/rt3-target
cargo test -p report-engine --offline -- --test-threads=1   # 预期：46 passed
bash apps/server/selftest_p6a.sh        # 预期：25/0（Phase 6a 面）
export GH_TOKEN=$(cat ~/.config/gh/pat); gh run list -L 2   # 预期：最近 run success
```

接手 Phase 6b 尾段：读 `docs/phase0/PHASE-6B-TASK.md` §3 S6-S8，按第二节清单执行；未提交工作树先 `git status` 核（WIP 已入库，直接接着做）。
