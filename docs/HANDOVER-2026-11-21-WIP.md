# 交接文档 · 2026-11-21 深夜（RT-3 实施收尾在途）

> 接手人必读：本文是 2026-11-21 23:5x 时点的断点交接，接续入口 = 本文档 + 记忆指针 `rip3-progress-pointer`（~/.zcode/cli/memories/projects/pensting-4a408e0536a2c35f/memory/）+ 仓库内固化稿 `docs/rip3/wsw/WSW-CONTINUATION-REDLINES.md`。状态仲裁：git log + 本文档 + 实测命令为准。

## 一、总账（全部 CI 绿，本地=远端 `948b05bc`）

| 批次 | commit | CI run | 交付 |
|---|---|---|---|
| WSW-0 | 4e7b4c69 | 44205550588 | 审计×4 + hudsucker 选型 + POC + vendor 397 包 |
| WSW-1+WSW-2 基础面 | 077a8dd4 | 44293771053 | wsw-message（023 wsw_body_blob）+ wsw-runtime 原语 |
| RT-0.5 | f4f3ebcf | 44342084987 | 宿主能力目录 16 项 + Provider 契约六文档 + rt05-probes 五份取证 |
| RT-1 | 12258e86 | 44365444603 | native_rust adapter（allowlist/env 白名单/超时击杀/统一信封）+ DI-0 ADR |
| DI-1 | 659ed1e8+2cef9439 | 44394568183 | document-router 三通道 PDF/Office/图像 + 负例矩阵 |
| WSW-2 主体 | 16401fcb | 44573324921 | MITM 运行时 + WswAuthority IP SAN（D4）+ 024 拦截队列 + Scope 闸 + 6 截图 |
| SEC-0 | 76c6feb6 | 44589236396 | rusqlite 0.41.1（SQLite 3.54.1 过 RUSTSEC-2026-0459 修复线）+ deny ignore 三条 + vendor 重写 |
| DI-2 | 0339a1eb | 44616753166 | OCR/媒体/归档三通道扩全（chi_tra 显式报错/7z 两阶段四闸） |
| WSW-3 | 36638fe5 | 44648573212 | History+Repeater 主体（025_wsw_exchange + 游标分页 + sender.rs 重放引擎） |
| WSW-3 实拍追加 | a6aea6a4 | 44759852926 | 脱敏展示投影 + e2e 21→32 + 截图 7 张 |
| WSW-3 修订批 | 6aa95bb6 | （同上两 run 后绿） | 跳数帽双源 min + Location RFC 3986 全谱 + 滞留行 stale 显性化 |
| WSW-3 报告回填 | 061cfdef | 44759852926 后 | E2 终态 + 行号实勘勘误 |
| SEC-0 遗留 docs | 339f1d00 | — | deny 复查日期行尾化 |
| MA-0 规划 | 47301cd4 | — | 多代理/多 AI 四形态规划（MA-1~4） |
| MA 参考一/二/三 | 5301a192 / 8f5e5c49 / 948b05bc | — | dsh-agent-teams R1~R9 + mnemon/billion-context 对比 + CB-1 立项 |

WSW-3 性能实测（已达标）：代理增量 0.91ms / History API p95 8.8ms / 10091 rows/s / e2e 32 断言 / wsw-runtime 108 项 / web 124 项。

## 二、在途：RT-3 Runtime Policy Resolver（断点详情）

- 工作流 `dwfrun-2dd4d1b0`（已 TaskStop——实施员 turn 被 Provider 静默挂起 3h19m，workflow 驱动不可靠，已改主会话 Agent 路径接管）
- **工作树在途产物（未提交，接手者续用勿重做）**：
  - `crates/tool-engine/src/policy.rs`（1598 行，决策面主体）+ `lib.rs` 挂载
  - `crates/tool-engine/tests/policy_resolver_matrix.rs`（144 格决策矩阵 + N1-N10 降级拒绝）与 `policy_resolver_manifests.rs`（29 份 Manifest 快照）——**已实跑 24 passed / 0 failed exit 0**（决策矩阵部分）
  - `crates/workflow-engine/src/bin/rp-workflow.rs`（缝 5 venv 接线，RT-2 五缝登记的 RT-3 S5 授权面）+ `tests/seam5_venv_wiring.rs`
  - `docs/phase0/RT-3-TASK.md` / `RT-3-ADR-POLICY-RESOLVER.md`（D1-D10）/ `RT-3-BATCH-REPORT.md`（草稿）
  - `docs/README.md`（RT-3 三行索引）
- **接手剩余步骤（按序）**：
  1. 跑完剩余门禁：`cargo test -p tool-engine --offline -- --test-threads=1`（全量）+ `cargo test -p workflow-engine --offline -- --test-threads=1` + `cargo clippy -p tool-engine -p workflow-engine --all-targets --offline -- -D warnings` + `bash -c "cargo deny check advisories && cargo deny check licenses"` + lock 与 HEAD 一致 + `git status --porcelain manifests` 为空
  2. 批次报告定稿（RT-3-BATCH-REPORT.md E 节填实测数字）
  3. 显式 add（逐文件：tool-engine policy.rs+lib.rs+两测试、workflow-engine rp-workflow.rs+seam5 测试、docs/phase0/RT-3×3、docs/README.md）→ commit 中文长描述 → push（重试 3）→ `export GH_TOKEN=$(cat ~/.config/gh/pat); gh run watch <run_id> --exit-status`
  4. 尾洞决策：**下一批 = MA-1（最高优先，用户点名）**，其后 MA-2 → CB-1（用户点名上调）→ WSW-4/MA-3 → DI-3/RT-4 → WSW-5~9 与 MA-4 交替

## 三、接手会话开工序（复制即用）

1. `git status -sb && git log --oneline -3`（预期：RT-3 在途产物 9 项未提交）
2. 跑完本文 §二.1 门禁（机器状态见 §四）
3. 门禁全绿 → §二.2/3 收口；门禁红 → 只修代码不改测试钉值，两次无新信息换方法
4. RT-3 收口后开 MA-1：参照 `docs/rip3/MULTI-AGENT-ACCESS-PLAN.md` + 独立小 workflow run（mc=2，拓扑模板见 RT-2/RT-3 草稿 `research-platform/.zcode/workflow-drafts/RT-3-Runtime-Policy-Resolver.dwf.ts`）

## 四、机器与环境事实（本日新增，接手必读）

1. **账户子代理并发上限 = 3**：workflow 驱动起跑扇出 ≥3 报 "Model creation failed"；CreateWorkflow/AmendWorkflow 一律 mc=2。Agent 主会话工具是可靠回退（RT-3 收尾即由此完成）。
2. **workflow 长 turn 不可靠**：实施员类长 ask 会被 Provider 静默挂起（3h19m 零计费实录）——大批实施改用 Agent 主会话路径，workflow 只承载设计/评审/报告等短 turn。
3. **多会话共存**：用户可能同时开多个 ZCode 会话（本日实测 kali distrobox 另一会话在 voicefox 仓库跑 feature 组合测试矩阵，load 300+）。**绝不杀非本会话的构建进程**；本会话门禁改 run_in_background 长窗口。
4. **磁盘 98% 满（11G 可用）**：target/ 70GiB 级。孤儿构建进程清理实录：pkill -9 -u emo rustc/cargo（可再生构建，两次共清 86 个，磁盘回 1.3G）。RT-5（Cargo Profile A/B + 清理）已在排期。
5. **cargo-deny 0.20.2**：`check` 子命令无 `--manifest-path`（exit 2）——门禁一律 `bash -c "cd <repo> && cargo deny check ..."`。
6. **lock 门禁语义**：计数与 `git show HEAD:Cargo.lock` 对比（绝对钉值已废——399↔400 三次拉锯教训）。
7. gh：keyring 失效兜底 `export GH_TOKEN=$(cat ~/.config/gh/pat)`；api.github.com 间歇 EOF 重试 2-3 次；SSH push 间歇断连重试 3 次。
8. 迁移链头：SQLite 025 / PG V0006；新迁移 026+ 单行 ALTER 纪律；生产 platform*.db 零触碰。
9. 配额：GLM 1005 = 唯一停止条件（WIP 快照"前缀 WIP 备份："；平台配额恢复后 run 自行恢复已两次实证）。
10. RTIS 只走 rt.py；POC 正文零复制；性能门每 UI 批强制（首屏<3s/交互<1s/p95<300ms/代理增量<50ms）。

## 五、路线全景（批间连续推进，尾洞/收口即开下一批）

```
RT-3 收口 → MA-1 → MA-2 → CB-1 → WSW-4 ⇄ MA-3 → DI-3/RT-4 → WSW-5/6 → MA-4 → WSW-7 → RT-5/6 → WSW-8 → RT-7 → WSW-9 → WSW 总 RELEASE → Phase 6 → 7 → 8
```

MA 轨规格：docs/rip3/MULTI-AGENT-ACCESS-PLAN.md；设计输入：docs/rip3/MA-DSH-REFERENCE.md（R1~R9 + mnemon 七机制 + billion-context B1~B4）。

## 六、复现

```bash
cd /media/emo/code/pensting/research-platform
git status -sb && git log --oneline -3   # 预期：main 与远端一致（948b05bc），RT-3 在途 9 项
export GH_TOKEN=$(cat ~/.config/gh/pat) && gh run list -L 3   # 预期：最新三 run 全 success
```
