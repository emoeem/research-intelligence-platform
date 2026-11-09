# 更新说明 · 2026-11-10（RIP 2.0 Phase 2：Podman Runtime 与 Tool Gateway 起步 podman_host）

> 工具执行隔离的第一步落地：podman_host 过渡通道（宿主 rootless Podman 分容器）跑通从 RIP API 到容器执行、Artifact 登记的完整派发链，五项故障演练自动化并全过。runtime-gateway crate 进库，心跳、健康落库与 WAITING_FOR_RUNNER 语义接通，Runner 离线不再伪装成工具失败。VM 目标拓扑（podman_vm）按 ADR §3.1 有意挂起，解除条件落盘在 PHASE2-DESIGN，宿主面不阻塞首批工具执行。

| 项         | 值                                                                  |
| ---------- | ------------------------------------------------------------------- |
| 状态       | 已发布（Phase 2 收口）                                               |
| 日期       | 2026-11-10                                                          |
| 适用版本   | git `352b3df` 及之后                                                 |
| 相关       | docs/phase2/PHASE2-DESIGN.md、docs/phase2/PHASE2-ACCEPTANCE.md、docs/phase0/TOOL-RUNTIME-ADR.md |

## 元信息

| 项         | 值                                                                  |
| ---------- | ------------------------------------------------------------------- |
| 阶段交付   | git `352b3df`（2026-11-09 23:35 CST，批次 P2-0 到 P2-7：PHASE2-DESIGN、domain node、runtime-gateway、迁移 007）+ `aeb5bde`（23:43 CST，文档标准与 Phase 0/1 更新记录）+ 本发布提交（收口批次 P2-8：podman_host 适配器、e2e/budget/recovery_drill 测试、镜像族文档、验收记录） |
| 数据库迁移 | `007_runtime_node.sql`（352b3df 入库）：经 crates/database/tests/migrations_runtime_node.rs 在测试副本验证，本会话 workspace 实测 2 passed 且迁移测试双次幂等；生产 platform.db 未触及（git status migrations/ 零改动，platform*.db 在 .gitignore） |
| 自动化测试 | cargo test `--offline --workspace` 实测 passed=194 failed=0 ignored=0（Phase 1 时 workspace 全绿，专项 105；本阶段新增 runtime-gateway 与 podman_host 用例后增至 194）；worker unittest 实测 Ran 125 tests OK（skipped=11），与 Phase 1 持平 |
| 静态检查   | cargo clippy `--workspace --all-targets -D warnings` 零告警（exit 0）；cargo fmt `--all -- --check` 通过 |

以上计数均为本会话（2026-11-10 收口入库前）在仓库根实测：`cargo test --offline --workspace`、`cargo clippy --offline --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cd workers/python && python3 -m unittest discover -s tests -v`。

## 一、面向使用者的变化

1. **podman_host 执行通道**（crates/tool-engine/src/adapter/podman_host.rs）：工具在宿主 rootless Podman 分容器执行。安全默认值全部由 Adapter 构造侧保证：挂载白名单只含工作区 `in/`（只读）与 `out/`（读写）、`--network none`、`--read-only`、`--userns=keep-id`、`--cap-drop ALL`、镜像按 digest 锁定。容器内白名单外宿主路径不可达有负向演练实录（PHASE2-ACCEPTANCE.md §3 演练 3）。
2. **runtime-gateway crate**（crates/runtime-gateway）：Runner 心跳探测、runtime_node 健康落库、WAITING_FOR_RUNNER 判定与恢复编排。心跳停摆的 run 进入 WAITING_FOR_RUNNER 而非 FAILED（计划 §5.6 口径），恢复后按 submit_key 先查后派，不新建重复 run 行。
3. **migrations/007_runtime_node.sql**：runtime_node 表承载 Runtime 身份与健康状态；词表覆盖 ADR 全拓扑（python_worker、podman_host、podman_vm、native_rust、mcp_client、http_api、cli_adapter），remote_runner 在 Schema 与 Registry 双重拒绝（未开放）。
4. **容器镜像族**（containerfiles/rip-tool-base/）：Containerfile + 冒烟工具（smoke_tool.py）+ manifest 双份（tool-manifest.yaml/json）+ containerfiles/README.md 缓存与基线注入说明；镜像 digest 三处对账一致（manifest 与测试常量）。
5. **五项故障演练自动化**（crates/runtime-gateway/tests/recovery_drill.rs，3 用例全过）：runtime 不可达、恢复后幂等重放、容器越界负向检查、磁盘自愈、演练后临时容器与悬空镜像清零。命令原文与输出摘录在 PHASE2-ACCEPTANCE.md §3。

## 二、为什么这样做

- **podman_host 先行，VM 挂起**：ADR §3 决定第 1 条，VM 解锁条件（云镜像、会级 libvirt、VM 内 rootless Podman、文件通道）未达成前，首批工具不能等；宿主 rootless 容器先提供白名单隔离面。podman_vm 的词表身份已在 007 表 CHECK 域保留，切换评审条件写成文档（PHASE2-DESIGN §3.2），不是口头承诺。
- **WAITING_FOR_RUNNER 而非 FAILED**：Runner 离线是环境问题不是工具失败，伪装成失败会让重试策略与账目统计失真。跃迁 QUEUED 到 WAITING_FOR_RUNNER 进状态机表（tool-engine state.rs），不是特判。
- **挂载白名单在 Adapter 构造侧而非用户参数**：容器可访问什么由代码决定，不由调用方声明，越界面从入口关闭；inspect 逐条断言进 e2e 测试防回归。

## 三、已知取舍

1. **VM 字面验收挂起**：计划 §14:772 五项中「真实工具派发到 VM 运行」「VM 离线演练」两项在 VM 面未做，本阶段以宿主等价面达成并如实标注（PHASE2-ACCEPTANCE.md §1、§4）；S1 到 S4 解锁后补齐，切换评审不得在 VM 冒烟通过前启动。
2. **submit_key 引擎层唯一性未强制**：submit 入口仅 1..512 长度校验，幂等重放的正确性依赖派发器「先查后派」编排纪律（recovery_drill 演练 2 让步声明）；引擎层唯一约束列为后续加固项。
3. **Gateway 常驻编排未落地**：心跳当前为派发/恢复按需探测加 600s 间隔守卫，rip-gateway systemd user 单元属 P2-4c/Phase 5 前置项。
4. **CI 无 podman 保证**：GitHub runner 容器用例自动跳过保持 CI 绿，容器端到端保持本地执行（V-9 待验证项）。

## 四、回滚方式

- 代码回滚：`git revert <本发布提交>` 与 `git revert 352b3df`（runtime-gateway、podman_host、007 迁移随提交回退）。
- 迁移回退：007 为纯新增表（runtime_node），测试副本 `DROP TABLE runtime_node` 即可；生产 platform.db 未应用，无需回退。
- 容器面回滚：`podman rmi localhost/rip-tool-base:smoke` 移除交付镜像，宿主机零新增依赖（pacman -Q 快照 2415 行全程 diff 为空）。
