# 交接文档 · 2026-11-23 下午（Phase 6b 已收口，RIP3 主队列剩 Phase 7/8）

> 接手人必读：本文 + 记忆指针 `rip3-progress-pointer`（`~/.zcode/cli/memories/projects/pensting-4a408e0536a2c35f/memory/`）+ 红线固化稿 `docs/rip3/wsw/WSW-CONTINUATION-REDLINES.md`。状态仲裁：`git log` + 本文 + 实测命令。

## 一、总账（十八批，Phase 6b 收口）

Phase 6b 之前十七批见 `docs/HANDOVER-2026-11-23-WIP.md`（历史档）。本批四个 commit：

| commit | 内容 |
|---|---|
| `deba165b` | WIP 备份（S1-S5：轻量 PDF 四钉 vendor + render/{pdf,docx,svg} + fonts/ooxml/qc + SARIF schema + 4 测试文件） |
| `2a6ba872` | S6-S8 收口（V0008__reports.sql + report_repo 四函数 + reports_pg 4 测试 + selftest_p6b 36/36 + selftest_p6a 连带收口 + 批次报告 + WIP 态 fmt 61 hunks 自净） |
| `8068e764` + `e36dccf1` | CI 修复一：qc_pdf 正例 CJK 字体面分环境计（runner 无 CJK 字体 → 三项中文判据 SKIP 分计）+ 登记行补注 |
| `3842a4bf` + `d3b695d9` | CI 修复二：clippy 四条真 lint 定点修 + 批次报告坏消息 8（本地门管道吞退出码假成功教训） |

**Phase 6b 终态**：lock 432→440 / vendor 406→414（8 包 ≈9.0M，deny 零增行）/ SARIF 官方 schema 资产（sha256 c3b4bb2d）/ V0008 reports 五列冻结 / 七签名钉定 / workspace 1389 passed 零红 / fmt 943 恒定 / selftest_p6b 36、selftest_p6a 25 全绿 / CI 绿（HEAD `d3b695d9`）。

## 二、Phase 7 起点（下一批）

- **范围**（PLAN :672）：Finding → 候选知识（donor-submit 桥，MA-3 供体链已落）+ Evidence 溯源 + 上游更新监测 + 失效链接检查 + 旧 POC 巡检 + 自动质量巡检 + 维护报告。
- **可复用底座**：`report_engine::qc_pdf/qc_docx/qc_sarif`（质量巡检质检面）+ `report_engine::render_*`（维护报告导出）+ `intel_db::report_repo`（报告台账）+ `rp-kc donor-submit`（知识供体）。
- **前置契约**（6b 交付）：reports PG 形状冻结（PHASE-6B-BATCH-REPORT.md §6.1）/ 渲染七签名（§6.2）/ 质检三面 :545 映射（§6.3）——Phase 7 按此消费，禁改形。
- **Phase 8**：1k/10k/100k 语料合成器 + 九面基准（P4-4 基准载具已在）。Phase 8 收口 = RIP3 主队列走完。

## 三、环境事实增量（本批新增，接手必读）

1. **clippy/门禁退出码禁管道**：`cargo clippy ... | tail; echo $?` 取到的是 tail 退出码——本地假绿一次（Phase 6b CI 修复二根因）。一律 `> /tmp/x.log 2>&1; echo $?`。
2. **CI 环境敏感第④型：字体面差**——runner 无 CJK 字体，任何中文断言前做 skrifa cmap face 探测；本机预演 = `unshare -r -m` + bind-mount 只含 LiberationSans 的目录 + tmpfs 盖掉 ~/.local/share/fonts。
3. **skrifa 必须带 std**：`default-features=false, features=["std"]`（裸剥 read-fonts no_std 缺 libm 三错）。
4. **report_repo PG 类型**：`jsonb_build_object($2::text, $3::jsonb)` + 传 `serde_json::Value`（String→Jsonb WrongType）。
5. **集成测试迁移就绪**：run_migrations 内挂 `skip_unless_pg`，勿依赖测试字母序。
6. 其余环境事实（CARGO_TARGET_DIR=/home/emo/rt3-target、vendor+deny 政策、并发上限、SSH 推送间歇 ECONN/DNS 污染 `57.155.223.240`——重试 3-6 次 + `git ls-remote`/gh api 双验）见 WIP 档与记忆。

## 四、复现与接手命令

```bash
cd /media/emo/code/pensting/research-platform
git log --oneline -6          # 预期：d3b695d9 … 3842a4bf … e36dccf1 … 8068e764 … 2a6ba872 … deba165b
git status --porcelain | wc -l        # 预期：0
grep -cE '^\[\[package\]\]' Cargo.lock # 440
ls vendor/ | wc -l                     # 414
export CARGO_TARGET_DIR=/home/emo/rt3-target
cargo test -p report-engine --offline -- --test-threads=1   # 46 passed
cargo clippy --offline --workspace --all-targets -- -D warnings > /tmp/c.log 2>&1; echo $?   # 0
bash apps/server/selftest_p6b.sh       # PASS=36 SKIP=0 FAIL=0
bash apps/server/selftest_p6a.sh       # PASS=25 SKIP=0 FAIL=0
export GH_TOKEN=$(cat ~/.config/gh/pat); gh run list -L 2    # 预期：最近 run success
```

接手 Phase 7：读 `docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md` :672 + 本档第二节 + `docs/phase0/PHASE-6B-BATCH-REPORT.md` §6（前置契约）。
