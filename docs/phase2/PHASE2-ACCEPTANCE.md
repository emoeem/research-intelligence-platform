# Phase 2 阶段验收记录：podman_host 通道（P2-8 收口交付物）

- 日期：2026-11-10
- 范围：ROADMAP P2-8 的 podman_host 面（故障恢复演练 + 阶段收口）；VM 执行面维持挂起（docs/phase2/PHASE2-DESIGN.md §3.1，S1 到 S4 未解锁）。
- 执行基准：计划 §14 Phase 2（763 到 772 行）与 §5.6（333 到 343 行）；TOOL-RUNTIME-ADR §3、§3.1、§5、§6、§7；PHASE2-DESIGN §5、§6。
- 红线遵守声明：本文为验收记录，内容全部来自本会话实际执行的命令与测试；撰写过程未在宿主机安装依赖（pacman -Q 快照 2415 行全程无变化，见 §6），未写 migrations/ 下任何 platform*.db，未启动任何虚拟机，未修改生产 RTIS 行为。
- 载具：`crates/runtime-gateway/tests/recovery_drill.rs`（演练 1/2/3/4/5 自动化，3 用例全过，退出码 0）+ 本文档 §3 的 shell 实录。

---

## 1. 计划 §14:772 五项验收对照（podman_host 面达成口径）

| 计划验收项（原文） | 达成方式与证据 | 状态 |
|---|---|---|
| 真实工具可从 RIP API 派发到 VM 运行 | 宿主面：rip.smoke.native 经 tool-engine 派发链到宿主 rootless 容器 SUCCEEDED（podman_host_e2e 5/5）；VM 面：挂起，如实标注 | 宿主面达成，VM 面挂起 |
| 宿主机没有新增工具依赖 | pacman -Q 快照 2415 行全程 diff 为空（P2-5 起至本批收口三次对照）；镜像构建/拉取属运行时数据（ADR §3.2 口径） | 达成 |
| 容器不能访问宿主机任意目录 | 挂载白名单由 Adapter 构造保证 + inspect 逐条断言（podman_host_e2e）+ 白名单外路径容器内不可达实录（本文 §3 演练 3） | 达成 |
| 输出可登记 Artifact | out/ 产物 sha256 与 artifact 表对账一致（podman_host_e2e smoke 用例）；partial 场景照常登记（podman_host_budget partial 用例） | 达成 |
| VM 离线不会导致任务状态错乱 | 宿主 Runner 离线等价演练（本文 §3 演练 1/2：WAITING_FOR_RUNNER + submit_key 幂等重放）；VM 字面演练随 VM 面挂起 | 宿主面达成，VM 面挂起 |

## 2. Runner 全景状态（ADR §3 第 2 条）

| Runtime | 定位 | 现状（本阶段收口时点） |
|---|---|---|
| python_worker | 既有 Python Workers 宿主派发器（Phase 1 P1-6） | 在用，进程池探测心跳接入 Gateway |
| podman_host | 过渡 Runner（ADR §3 决定第 1 条）：宿主 rootless Podman 分容器 | 在用：P2-4a 数据面 + P2-4b 心跳判定 + P2-5 容器执行面 + P2-7 缓存与基线 + 本批演练收口 |
| podman_vm | 目标拓扑（计划 §5.1） | 设计落盘（PHASE2-DESIGN §5.2），执行面挂起（S1 到 S4），词表身份保留（domain::RuntimeNodeKind::PodmanVm 与 runtime_node CHECK 域） |
| native_rust / mcp_client / http_api / cli_adapter | 计划 §6 表 P1 | 未接线，词表在位 |
| remote_runner | 计划 §6:368 P2 | Schema 与 Registry 双重拒绝，未开放 |

共存口径：podman_host 过渡与 podman_vm 目标并存到 VM 通道解锁后的切换评审（ADR §3 第 2 条：充分条件达成即切换，podman_host 降级为本地开发与应急通道）。

## 3. 五项演练记录（命令原文 + 输出摘要）

### 演练 1：runtime 不可达（停心跳目标）→ WAITING_FOR_RUNNER

- 载具：`cargo test --offline -p runtime-gateway --test recovery_drill drill1` （SwitchProbe healthy=false 模拟心跳停摆，health 落库 broken）。
- 结果（测试断言实录）：dispatch 后 `tool_run.status = WAITING_FOR_RUNNER` 而非 FAILED；`runtime_node.health = 'broken'`；两次 `probe_and_resume` 均立即返回 `RunnerUnavailable`（不卡死，全程 < 5s），run 原位保持。
- 口径出处：计划 §5.6:336「VM 离线时任务进入 WAITING_FOR_RUNNER，不伪装成工具失败」；跃迁对为 tool-engine state.rs 第 3 行（QUEUED → WAITING_FOR_RUNNER）。

### 演练 2：恢复后按 submit_key 幂等重放

- 载具：同测试 drill1 后半段（SwitchProbe 翻转 healthy=true）。
- 命令原文（测试内执行）：
  - `SELECT run_id FROM tool_run WHERE submit_key = 'rec-rn-rec-wait'` → 恰 1 行（重放先查后派，不新建行）；
  - `probe_and_resume` → PREPARING（状态机第 4 行）；
  - `drive_resumed` → transcript `WAITING_FOR_RUNNER → PREPARING → RUNNING → SUCCEEDED`；
  - `SELECT COUNT(*), MAX(status) FROM tool_run WHERE submit_key = ?` → `1 | SUCCEEDED`；
  - `replay_json.submit_key` 透传核对一致；
  - 终态后二次 `probe_and_resume` → 引擎拒绝（终态不可迁出），状态稳定 SUCCEEDED。
- 结果：无重复 run 行、无重复副作用、无状态错乱。口径出处：计划 §5.6:337-338（Run ID + 幂等提交键，重连后查询已完成与运行中任务）。
- 让步声明（如实记录）：引擎 submit 层不按 submit_key 去重（仅 1..512 长度校验，request.rs:190），幂等重放的正确性依赖派发器「先按 submit_key 查既有 run 再续跑」的编排纪律；worker 层结果缓存（worker_runtime.py）在 python_worker 通道继续生效。引擎层唯一性约束列为后续加固项（见 §7）。

### 演练 3：容器越界负向检查

- 载具：`cargo test --offline -p runtime-gateway --test recovery_drill drill3`（podman 门禁内）。
- 挂载面断言（Adapter 等价 flags 活容器 inspect）：Binds 恰 2 条，源路径全部在工作区 `rn-rec-boundary/{in,out}` 之下；`platform.db`、`.rp-workbench-token`、`sock`、`/var/run`、`/run/podman` 五类敏感名零出现。
- 白名单外宿主路径不可达（命令原文）：

  ~~~bash
  podman run --rm --network none --read-only --userns=keep-id --cap-drop ALL \
    -v <ws>/in:/task/in:ro -v <ws>/out:/task/out:rw \
    localhost/rip-tool-base:smoke@sha256:84877bf6...e87 \
    sh -c 'ls /home/emo/migrations/platform.db /home/emo/.rp-workbench-token /run/podman/podman.sock /var/run/docker.sock 2>&1; echo PROBE_EXIT=$?'
  ~~~

  输出摘要：

  ~~~text
  ls: cannot access '/home/emo/migrations/platform.db': No such file or directory
  ls: cannot access '/home/emo/.rp-workbench-token': No such file or directory
  ls: cannot access '/run/podman/podman.sock': No such file or directory
  ls: cannot access '/var/run/docker.sock': No such file or directory
  PROBE_EXIT=2
  ~~~

- 只读输入挂载不可写（命令原文同款，`touch /task/in/escape`）：输出 `touch: cannot touch '/task/in/escape': Read-only file system`。
- 结论：白名单外宿主路径在容器内不可达，敏感面零出现，只读面拒绝写入。口径出处：计划 §14:772 验收第 3 条 + §5.4 挂载约束。

### 演练 4：磁盘自愈流程（ADR §6）

- 阈值判定实录：`df -BG --output=avail /` → `244G`，高于 50 GiB 阈值 → `heal_required=false`（记录性通过；低于阈值时流程为「先清悬空镜像与孤儿容器，再告警」）。
- 清理流程实录（测试内执行）：`podman import`（无 tag）造真实悬空镜像 → `podman images --filter dangling=true -q` 可见（计数 = 基线 + 1）→ `podman rmi -f <id>` 移除 → 悬空计数恢复基线；孤儿容器按 `label=rip.runtime=rip-tool` 清单化（本批为空）。
- 容量复核：`podman system df` → Images TOTAL 14、SIZE 2.775GB、RECLAIMABLE 336.8MB (12%)，远低于 50 GiB 预算（ADR §7「镜像与缓存总量」行）；Containers 3 个为用户自有，RIP 面零驻留。
- 演练纪律声明：测试内只移除本演练创建的悬空镜像（Drop 守卫保证断言失败也不泄漏）；生产策略按 ADR §6 清全部悬空镜像与孤儿容器后再告警。

### 演练 5：演练后临时容器与临时镜像清零

- `podman ps -a --filter label=rip.runtime=rip-tool -q | wc -l` → `0`；
- `podman images --filter dangling=true -q | wc -l` → `0`；
- 镜像列表基线：仅余批次交付物 `localhost/rip-tool-base:smoke`（digest `sha256:84877bf60e8a768c08f5b7a9b334eac48231b70f8b1c248008723590f79dc81a`，与 tool-manifest.yaml/json 及测试常量三处一致），P2-7 重建产生的 3 个悬空中间镜像已于上批清理。

## 4. ADR §3.1 迁移条件达成度评审输入

**充分条件（达成即执行切换）五项子项现状**：

| 子项（ADR §3.1 原文要点） | podman_host 面现状 | VM 面差距（S 批次） |
|---|---|---|
| 云镜像准备 | 不适用（宿主面无 VM） | S1：云镜像选型下载 + sha256 对账（V-6 前置） |
| VM 以会话级 libvirt 启动（qemu:///session） | 宿主侧连接可用性维持实测（qemu:///session，P2-0 复核） | S2：virt-install 首启 + 基线快照 |
| VM 内 rootless Podman 配置 | 不适用 | S3：VM 内 subuid/subgid + `podman ps` 冒烟（V-7） |
| Gateway 心跳与文件交换通道打通 | 心跳面已达成（P2-4b + 本批演练 1/2）；文件交换面宿主通道已达成（P2-5 in/out/tmp + Artifact） | VM 通道执行面（S4）与 VM 内文件通道验证 |
| 会话级存储池自建 | 不适用 | S1：`virsh pool-define-as` 家目录建池（V-5） |

**计划 Phase 2 验收五项**：宿主面全部达成（本文 §1）；VM 字面三项（派发到 VM 运行、VM 离线演练）挂起。充分条件整体达成度：宿主等价面已就绪，VM 面四项待 S1 到 S4 解锁后补齐，切换评审不得在 VM 冒烟通过前启动。

**提前触发条件监控状态（任一命中即解除挂起）**：

1. 不受信第三方工具（TPscan 等）登记：尚未发生；一旦准入必须落 VM（监控中，登记批次强制核对）。
2. 执行配置超出宿主 rootless 可表达范围（真实浏览器/特权网络/特殊内核能力）：尚未发生；Browser Automation 族评估排在 Phase 3 P3-8。
3. podman_host 越界污染事件：未发生（演练 3 负向检查全部通过）；发生即暂停新增 podman_host 登记并加速迁移。

## 5. 批次全景与质量门

| 批次 | 交付物 | 验证 |
|---|---|---|
| P2-0 | PHASE2-DESIGN.md（十节，VM 挂起与解除条件） | 文档自检 + 无长横线 |
| P2-4a | domain::node + migrations/007 runtime_node | cargo test domain/database 双次幂等 |
| P2-4b | crates/runtime-gateway（心跳/健康落库/WAITING 判定接通） | wait_runner 4/4 |
| P2-5 | adapter/podman_host.rs + 冒烟镜像族 + Schema/Registry 词表扩展 | e2e 5/5（inspect 逐条 + keep-id 对账） |
| P2-7 | 镜像族 README + FamilyLimiter + 缓存策略 + 基线注入 | budget 6/6（缓存零拉取/partial/截断） |
| P2-8（本批） | recovery_drill 3/3 + 本文验收记录 | 见 §3；工作区质量门收尾 |

质量门（本批收尾实测）：`cargo test --offline --workspace` 与 `cargo clippy --offline --workspace --all-targets -- -D warnings` 全绿（计数见 §6）；`cargo fmt --all -- --check` 通过。

## 6. 红线核查记录（本批收口时点实测）

- `pacman -Q | wc -l` = 2415，与本阶段起点快照 diff 为空（宿主机零安装）。
- `git status --short -- migrations/`：无新增迁移（007 已入库）；platform.db 与 platform-dryrun.db 零改动。
- `podman ps -a --filter label=rip.runtime=rip-tool -q | wc -l` = 0；悬空镜像 0；镜像列表仅余用户自有镜像 + rip-tool-base:smoke 交付物。
- 全部新增/修改文件 `grep -P '[\x{2014}\x{2013}]'` 零命中。
- 未启动任何虚拟机；未修改生产 RTIS 行为；docker 全程未用。

## 7. 遗留与后续加固项（如实声明）

1. 引擎层 submit_key 唯一性未强制（演练 2 让步声明）：后续可在 submit 入口增加同键活跃 run 查重，或在 005 表加部分唯一索引（活跃态），属 tool-engine 加固项，不阻塞本阶段验收。
2. Gateway 常驻编排（rip-gateway systemd user 单元与守护循环）未落地：心跳当前为派发/恢复按需探测 + 600s 间隔守卫；编排属 P2-4c/Phase 5 Runtime 页数据面前置项。
3. VM 面 S1 到 S4 全部挂起：解除条件见 PHASE2-DESIGN §3.2（ADR §3.1 原文摘录）；本文 §4 为解锁后的评审输入。
4. CI（GitHub runner）无 podman 保证（V-9 待验证）：容器用例自动跳过保持 CI 绿，容器端到端保持本地执行。
