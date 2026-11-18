# WSW 续作红线与启动机制（固化稿）

- 文档类型：评审修订批固化稿（2026-11-18）——替代原仓库外全程手册 `/media/emo/code/pensting/WSW-CONTINUATION-WORKFLOW.md` 的 §7 红线节与启动机制
- **丢失与恢复结论**：原手册在磁盘上不存在（`ls` 与全盘 `find -maxdepth 6` 零命中）；git 全历史零命中（`git log --all --name-only -- '*WSW-CONTINUATION*'` 仅命中 HANDOVER-RIP2/3-CONTINUATION.md）；会话 rollout（`~/.zcode/cli/rollout/` 仅存本 dwfrun 两文件）零命中——**原文不可恢复**。
- 本档性质：**非原文恢复**。条款全部从仓库内可考来源逐条固化并标注出处；原手册"九条红线"精确原文无从对照，后续批次发现外部口径与本档冲突时，以 PLAN / ADR / 批次任务书为准并回改本档。
- 引用面：`.zcode/workflow-drafts/WSW-0-审计与-POC-收口.dwf.ts:2` 与记忆指针 `rip3-progress-pointer.md`（:3、:14）原指向已失效路径，本次修订已同步改指本档。

---

## 一、批次循环（原手册"八步循环"的可考形态）

来源：`.zcode/workflow-drafts/WSW-0-审计与-POC-收口.dwf.ts:341`（"按八步循环展开实施→定向测试→实拍→批次报告→收口 CI"）+ WSW-PLAN §十三 :783（十步流程）。原手册精确八步列表不可恢复，下表为两处可考来源的并集，每批次按此循环：

1. **实施**（先最小设计：分析 → 核对来源 → 最小设计 → 实现，WSW-PLAN §十三 :783）；
2. **定向测试**（本批 crate `cargo clippy -p … --all-targets` + `cargo test -p …`，draft :192 范式）；
3. **实拍**（真实链路留证，命令原文 + 关键回显进批次报告）；
4. **批次报告**（惯例十一节 A-K，坏消息单列，数字实测带来源，不编造未发生的测试数字，draft :249-250）；
5. **收口**（`git status --porcelain` 核对 + 显式 add 本批文件清单 + 中文长描述 commit，draft :265）；
6. **push + watch CI 至终态**（`gh run watch --exit-status`，终态前不得返回，draft :265）；
7. **CI 红 → 修复批至多三轮**（`gh run view --log-failed` 定位；红不进下一批，draft :272、:293-322）；
8. **收账**（下一批开工，或按配额先 WIP 备份再等重置，指针 :14、:18）。

## 二、九条红线（每批次全程有效，逐条带可考出处）

| # | 红线 | 出处 |
|---|---|---|
| 1 | **离线 vendor 红线**：新增 Rust 依赖必须 `cargo vendor` 全量入库并过 deny 门（deny.toml `[licenses]` fail-closed），禁破坏 `.cargo/config.toml` vendored-sources 的 --offline 构建 | 00 §1.4；draft :177 |
| 2 | **Rust-First**：核心代理、HTTP 消息模型、重放和 Fuzzer 调度禁用 Python/Node 实现 | WSW-PLAN §6.1 :444（协议与网络核心=Rust 行）、§十二 :760；draft :176 |
| 3 | **不碰生产库 / 不直写 RTIS**：`migrations/platform*.db` 不写（增量迁移只在开发库副本与测试内存库执行）；漏洞事实只走 rt.py / RTIS MCP，零直写零复制 | WSW-PLAN §十二 :763；`crates/intel-db/src/lib.rs:10`；`migrations/022_coverage_matrix.sql:17-19` |
| 4 | **不建第二套事实体系**：禁第二套数据库事实源、Workflow Runtime、Agent Runtime、Tool Registry | WSW-PLAN §十二 :761；02 矩阵 §2.3 反查 |
| 5 | **CI 红不进下一批**：watch 至终态，修复批至多三轮 | draft :272、:293-322 |
| 6 | **测试红只修代码不改测试钉值**（除非坐实钉值过时并如实记录） | draft :283 |
| 7 | **同一失败方法最多两次**：第二次无新信息就换等效路径并如实记录，不许谎报 PASS | draft :176 |
| 8 | **宿主机零系统级安装**：不用 sudo，依赖走 vendor 入库 / 容器面 | draft :176 |
| 9 | **收口纪律与配额**：显式 `git add` 本批文件清单（禁 add -A；.auto 等会话杂物不入库；用户素材按批显式入库）；配额见底先 WIP 备份再等重置 | draft :265；指针 :14、:18 |

## 三、新会话启动词模板（原 §7 启动词的可考形态，按 draft :4-5 开工核实段与指针 :14-18 组装）

```text
仓库根 /media/emo/code/pensting/research-platform。继续 RIP3 WSW 批次推进：
1) 开工核实：git log --oneline -1 取 HEAD；gh run list --limit 1 核对 CI 终态；
   读 docs/HANDOVER-RIP3-CONTINUATION.md（跨会话交接入口）与
   docs/rip3/wsw/WSW-CONTINUATION-REDLINES.md（八步循环 + 九条红线）。
2) 找到当前批次任务书（docs/rip3/wsw/WSW-N-TASK.md；无任务书时先按八步循环落
   任务书再实施），按红线全程推进：实施 → 定向测试 → 实拍 → 批次报告 →
   收口 CI 至绿；CI 红不进下一批，修复批至多三轮。
3) 每批 commit/push 自走（批次自动入库已获用户授权，指针 :16），批间不等确认；
   配额见底先 WIP 备份。
```

- 进度与配额指针：`~/.zcode/cli/memories/projects/pensting-4a408e0536a2c35f/memory/rip3-progress-pointer.md`（单大批次 25~35M tokens，每 09:00 周期推一阶段，指针 :18）。
- 两份 WSW 规格必读（指针 :16）：`docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md`（Figma 基准 + 禁平行设计系统）与 `docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md`（Rust-First + 禁第二套 Runtime/Registry + 安全边界）。
