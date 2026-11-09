# Phase 2 阶段说明与拓扑设计：Tool VM 与 Podman Runtime（P2-0 交付物）

- 日期：2026-11-09
- 状态：已定稿（本阶段批次 P2-0 的交付物；批次内部实现细节按 ROADMAP §1.2 冻结规则可在各批开工时微调，批次边界与完成判据见 §6）
- 定位：PHASE-ROADMAP §1.2 冻结纪律「每个 Phase 开工前按参考计划 §24 模板出阶段说明」在 Phase 2 的执行件。§24 模板出处：/home/emo/Downloads/RIP-完整项目实施计划.md 2296 行起，十节为：目标、范围、非目标、现状、设计、实施批次、风险、验收、回滚、交付物。
- 执行基准：docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md（下称「计划」）§14 Phase 2（763 到 772 行）与 §18（915 到 926 行）；docs/phase0/TOOL-RUNTIME-ADR.md（下称「ADR」）§3、§3.1、§5、§6、§7；docs/phase0/PHASE-ROADMAP.md（下称「ROADMAP」）§3；docs/phase0/TOOL-CONTRACT-DRAFT.md（下称「TOOL-CONTRACT」）相关裁定。
- 红线遵守声明：本文为设计文档。撰写过程只新增本文件，未修改任何源文件，未写 `migrations/` 下任何 `platform*.db`，未在宿主机安装任何依赖，未启动任何虚拟机（本阶段红线：一条 VM 写操作都不执行，见 §3.1）。文中「现状」一节的运行时结论全部来自本会话只读实跑命令，逐条记录于 §4.4。

---

## 1. 目标

把「AI 不允许随意污染宿主机」落实为架构约束（计划 §18 第 2 条，920 行）：本阶段交付一条真实可用的受控工具执行通道，以及守住它的心跳与故障恢复面。

**本阶段只实现宿主机 rootless Podman 通道**，依据 ADR §3 决定第 1 条（ADR:93）原文：「第一步（本决定生效即执行）：新增 Runtime 类型 `podman_host`（宿主 rootless Podman 分容器），作为过渡 Runner 接入 Tool Engine」。落到本阶段就是两件事（对应 ROADMAP 车道 C，ROADMAP:57）：

1. **Tool Gateway**：Runner 注册、心跳探测、RuntimeNode.health 落库、WAITING_FOR_RUNNER 判定接通状态机（ROADMAP P2-4 的宿主侧内容）。
2. **podman_host Runtime Adapter**：计划 §5.4 容器约束清单逐条落地、文件交换与 Artifact 登记、资源限制真实生效、故障恢复测试（ROADMAP P2-5 全部内容与 ADR §5/§6/§7 的落地）。

VM 目标拓扑**不推翻但本阶段不实现**：按 ADR §3 原文「目标拓扑不变：专用 VM + VM 内 rootless Podman 分容器（计划 §5.1），不推翻、不降级」（ADR:87）。VM 部分写成设计落盘于本文 §5.2，ROADMAP P2-1/P2-2/P2-3/P2-6 的 VM 执行面挂起，解除条件逐条摘录 ADR §3.1 原文（见 §3.2）。

## 2. 范围

本阶段允许修改或新增的模块（全部在 research-platform 仓库内，独立 commit，逐批可回滚）：

| 落点 | 内容 | 批次 |
|---|---|---|
| `crates/runtime-gateway`（新增） | Runner 注册、心跳探测、runtime_node 落库与查询、WAITING_FOR_RUNNER 判定（ROADMAP P2-4 命名沿用） | P2-1 |
| `migrations/007_runtime_node.sql`（编号落库前实查下一个可用号，ROADMAP §1.4 规则） | runtime_node 表，字段对齐计划 §12:696（id、kind、endpoint、health、version），幂等 DDL | P2-1 |
| `crates/tool-engine` | 新增 `src/adapter/podman_host.rs`（容器执行面）；引擎侧 Runner 可用性消费 | P2-2/P2-3 |
| `schemas/tool-manifest/v1.json` + `crates/tool-registry` | runtime.type 词表追加 `podman_host`（现词表七值不含它，§4.4 实测）与分支必填字段；校验九条第 4 条（image digest 固定）对 podman_host 同款生效 | P2-2 |
| `crates/domain` | RuntimeNode 等新类型（如需；2.0 新实体唯一合法落点，盘点 48 行纪律） | P2-1 |
| `deploy/` 或 `scripts/` | rip-gateway systemd user 单元文件与编排脚本（先例 rp-workbench.service，ADR §1.3 实测） | P2-1/P2-5 |
| 首个容器族 `Containerfile` 与自研负载脚本 | 仓库只保留 Containerfile 与配置；镜像与容器属运行时数据，任务容器用后即删，镜像总量受 ADR §7 预算约束 | P2-5 |
| `docs/phase2/` | 本文档 + 各批次验收报告（参考计划 §24 批次验收报告模板） | 全程 |
| 各 crate 契约测试与负向测试 | 约束清单逐条、资源基线逐项、WAITING_FOR_RUNNER、幂等重放、越界拒绝 | 全程 |

不修改：`apps/server` 既有端点行为（只读口径不变）、RTIS 本体（红线：只 Adapter 接入）、`workers/python` 既有 Worker Protocol v1 协议、M0 到 M8 已交付能力、`platform.db` 与 `platform-dryrun.db`（红线：不写生产库，一切库操作走开发库副本与测试内存库）。

## 3. 非目标

### 3.1 本阶段明确不做

1. **不启动任何虚拟机，不执行任何 VM 写操作**。ROADMAP P2-1（会话级存储池自建 + 云镜像选型落盘）、P2-2（VM 首启与基线快照）、P2-3（VM 内 rootless Podman 与 subuid/subgid）、P2-6（podman_vm Runtime Adapter）的执行面**全部挂起**：`virsh pool-define-as`、`virt-install`、`virsh start`、`virsh snapshot-create` 等命令一条都不跑；云镜像不下载；`~/rip-runtime` 下不建 VM 池目录。
2. 不实现镜像缓存策略的 VM 面（ROADMAP P2-7 中依赖 P2-6 的部分）；宿主侧的镜像 digest 固定与 50 GiB 预算管理纳入本阶段（ADR §7）。
3. 不做 VM 离线演练（ROADMAP P2-8 的 VM 面）；宿主 Runner 离线与重连语义（WAITING_FOR_RUNNER、幂等重放）纳入本阶段：计划 §5.6 的语义对本地 podman 通道同等生效（ADR §6 原文：「过渡期 Runner 是本地 podman（心跳即本机命令可用性检查）」）。
4. 不开放 remote_runner 登记（TOOL-CONTRACT D-TC-01：Schema 校验器继续直接拒绝）。
5. 不引入 docker 作为 Runner（ADR §3.2：docker 在位仅记录，全程不用）。
6. 不推翻两步走决策：`podman_host` 的过渡定位不变，VM 达成 ADR §3.1 充分条件后按 ADR §3 第 2 条降级为本地开发与应急通道，不再承载生产工具运行。
7. 不做 Phase 3 的 Tool Packs 与八族空白补齐（ROADMAP §4 范围）；本阶段只交付通道与守门面。

### 3.2 VM 执行面挂起与解除条件（ADR §3.1 原文逐条摘录）

ROADMAP P2-1/P2-2/P2-3/P2-6 挂起。挂起不是取消：目标拓扑不推翻、不降级（ADR:87 原文见 §1）。解除挂起走 ADR §3.1（97 到 107 行）的两类条件，**原文逐条摘录如下**：

**充分条件（达成即执行切换）**：

> VM 端到端冒烟全部通过，即计划 Phase 2 验收五项达成：真实工具可从 RIP API 派发到 VM 运行；宿主机没有新增工具依赖；容器不能访问宿主机任意目录；输出可登记 Artifact；VM 离线不会导致任务状态错乱（计划 §15 Phase 2 原文）。前置子项包括：云镜像准备、VM 以会话级 libvirt 启动（qemu:///session 实测可用）、VM 内 rootless Podman 配置（VM 内 subuid/subgid 配置属 Phase 2 工作项，待验证）、Gateway 心跳与文件交换通道打通、会话级存储池自建（现池 tmp 指向 /tmp 为 tmpfs 16G，实测不可用于 VM 镜像，需 `virsh pool-define-as` 在家目录自建，待验证）。

**提前触发条件（任一命中即启动 VM 通道建设，不得等充分条件自然达成）**：

> 1. 登记第一个不受信第三方工具（如 TOOL-INVENTORY §4.3 的 TPscan，许可证核实后准入）时，该工具必须跑在 VM 内，不落 `podman_host`。
> 2. 某工具的执行配置超出宿主 rootless 可表达范围：需要真实浏览器环境（Browser Automation 族补齐，参考 TELEGRAM-ADMIN-UI-REUSE.md §2.5 视觉回归的浏览器需求）、需要特权网络操作或特殊内核能力、需要与宿主完全隔离的运行时。
> 3. `podman_host` 发生越出任务目录与容器存储目录的污染事件（容器逃逸出预期边界），立即暂停新增 `podman_host` 工具登记并加速迁移。

两类条件的效果不同：充分条件达成即执行切换（podman_host 降级）；提前触发条件任一命中即**解除挂起、启动 VM 通道建设**（按 §5.2.5 的 S1 到 S4 挂起批次顺序执行，设计已在本文落盘，不重开评审）。两类条件均未命中期间，挂起状态维持，`podman_host` 只运行已登记且来源可信的工具（ADR §3.2 过渡期纪律）。

## 4. 现状

### 4.1 代码与协议面（Phase 1 已交付，commit 962cfa9，工作区干净）

- `crates/tool-engine`：Engine（submit/drive）、ToolRun 状态机 11 个合法状态对（`src/state.rs:17`），其中 WAITING_FOR_RUNNER 相关跃迁已冻结：QUEUED 到 WAITING_FOR_RUNNER（第 3 行）、WAITING_FOR_RUNNER 到 PREPARING（第 4 行）；`src/adapter/python_worker.rs`（790 行）是唯一真实 Adapter；`mock_runner.rs` 承载 Phase 1 的四路径契约测试。
- `crates/domain`：`ToolRunStatus` 八态齐备，WAITING_FOR_RUNNER 语义注释即计划 §5.6 原文口径（`crates/domain/src/tool.rs:16`）。
- WAITING_FOR_RUNNER 的判定来源目前是 Phase 1 Mock 开关，尚无真实心跳。这是 Phase 2 Gateway 要接通的缺口（TOOL-CONTRACT:198 原文：「WAITING_FOR_RUNNER 的判定依赖 RuntimeNode.health（计划 §12:696），VM 心跳机制属 Phase 2 Tool Gateway 范围」）。
- RuntimeNode 实体只存在于信封字段 `environment.runtime_node_id`（`crates/tool-engine/src/envelope.rs:81`）与 python_worker 的硬编码 `host-cachyos`（`src/adapter/python_worker.rs:782`）；**无 runtime_node 表**（本会话 grep migrations 与 crates 实测无命中），计划 §12:696 实体待落库。
- `migrations/005_tool_contract.sql`：tool_run（34 行起）与 artifact（78 行起）两表在位，submit_key 幂等列与 artifact sha256/role 词表齐备；`006_agent_data_plane.sql` 为 agent 两表。当前最大迁移编号 006，007 空闲（落库前再实查）。
- `schemas/tool-manifest/v1.json`：runtime.type 词表七值 `podman_vm`/`python_worker`/`native_rust`/`mcp_client`/`http_api`/`cli_adapter`/`remote_runner`（schema 64 到 65 行），**不含 `podman_host`**（grep 实测 0 命中）；podman_vm 分支字段规范在 TOOL-CONTRACT 表（81 行）：image 必须 digest 固定、可选 user/readonly_rootfs/mounts/env，是 podman_host 分支的同构起点。
- `manifests/tools/`：首批 28 份（14 工具，json/yaml 双份），全部 python_worker 与薄包装类型。
- 质量门与 CI 同命令（`.github/workflows/ci.yml` 实测）：`cargo test --offline --workspace`、`cargo clippy --offline --workspace --all-targets -- -D warnings`、`python -m unittest discover -s tests -v`。

### 4.2 运行时面（宿主机，本会话只读复核，输出见 §4.4）

- podman 6.1.4，Rootless=true，GraphRoot=`/home/emo/.local/share/containers/storage`，overlay 驱动：宿主 rootless 容器立即可用（ADR §1.3 结论本会话复核成立）。
- **宿主 podman 实例与用户自用容器共用**：本会话 `podman ps` 实测有 3 个用户自用容器在跑（ubuntu、kali、wb-proxy）。RIP 工具容器必须以 `rip-tool-*` 前缀命名并打专属 label，Gateway 的清理逻辑只处理自有标识，绝不触碰用户容器（风险 1，§7）。
- subuid/subgid：`emo:100000:65536` 两行在位，`--userns=keep-id` 的映射前提成立（其与具体镜像组合的行为回归仍属 ADR 待验证项 5，P2-2 实测）。
- 无免密 sudo：`timeout 5 sudo -n true` 退出码 1。全链路零安装、零 sudo 是硬约束（PC2-1）。
- VM 栈在位但未动：`virsh uri` 返回 `qemu:///session`（会话级可用，PC2-2）；libvirtd active；`/dev/kvm` crw-rw-rw-（PC2-4）；会话级唯一存储池 tmp 指向 `/tmp`（tmpfs，不可放 VM 镜像，PC2-3）；无既有 VM（`virsh list --all` 空）。
- 根分区可用 256G（ADR 撰写时 259G），高于 50 GiB 镜像预算阈值（PC2-5）；`pacman -Q` 基线 2415 行（本会话实测，作为「宿主机没有新增工具依赖」的对照基线）。
- systemd user 管理器在位（rp-workbench.service active 先例，PC2-6）；docker 在位不用（PC2-8）。

### 4.3 待验证项（承接 ROADMAP §14 V 表）

- VM 相关四项全部随挂起顺延，不因挂起失效：V-5 会话级存储池自建（阻塞 S1）、V-6 VM 端到端全流程（阻塞 S2/S3/S4）、V-7 VM 内 subuid/subgid（阻塞 S3）、V-8 `--userns=keep-id` 与具体镜像组合（宿主面部分由本阶段 P2-2 实测，VM 面部分随 S4）。
- 本阶段新增待验证项（随对应批次验证，不预先断言结论）：rootless podman 在本机 user 级 cgroup 委派下 `--cpus`/`--memory` 的真实生效性（风险 3，P2-3 行为测试裁决）；Gateway 心跳阈值与 systemd user 单元长稳性（P2-1）。

### 4.4 本会话复核记录（只读实跑，2026-11-09）

| 命令 | 输出摘要 |
|---|---|
| `podman --version` | `podman version 6.1.4` |
| `podman info --format '{{.Host.Security.Rootless}} {{.Store.GraphRoot}} {{.Store.GraphDriverName}}'` | `true /home/emo/.local/share/containers/storage overlay`，退出码 0 |
| `podman ps` | 3 个用户自用容器（ubuntu、kali、wb-proxy），退出码 0 |
| `timeout 5 sudo -n true` | `sudo: 需要密码`，退出码 1 |
| `virsh uri` | `qemu:///session`，退出码 0 |
| `systemctl is-active libvirtd.service` | `active` |
| `virsh -c qemu:///session pool-list --all` | 仅 tmp 池（活动，自动开始） |
| `virsh -c qemu:///session list --all` | 空（无既有 VM） |
| `ls -l /dev/kvm` | `crw-rw-rw- 1 root kvm 10, 232` |
| `grep '^emo:' /etc/subuid /etc/subgid` | 两行 `emo:100000:65536` |
| `df -BG --output=avail /` | 256G |
| `pacman -Q \| wc -l` | 2415 |
| `grep -c podman_host schemas/tool-manifest/v1.json` | 0（词表不含 podman_host） |
| `grep -rn runtime_node migrations/ crates/`（限 sql/rs） | 无 runtime_node 表 |

未执行项：未启动任何虚拟机、未写任何数据库、未安装任何依赖、未修改任何源文件（本文件除外）。

## 5. 设计

### 5.1 本阶段实现面：podman_host 通道（ADR §3 决定第 1 条）

过渡拓扑（与 ADR §4.1 同构，收敛到本阶段实际交付面）：

~~~text
CachyOS 宿主机（emo，无 sudo）
├── RIP 平台面（本阶段不动）
│   ├── rp-workbench.service（systemd user，已运行）
│   ├── rip-gateway.service（新建，systemd user 单元，承载 Tool Gateway）
│   └── migrations/platform.db（生产库，只出不进）
└── 工具执行面（podman_host，rootless，rip-tool-* 命名空间）
    ├── ~/rip-runtime/workspaces/<run_id>/{in,out,tmp}（文件交换工作区）
    └── rip-tool-<family> 容器（每任务一个，--rm 用后即删）
~~~

#### 5.1.1 模块与接口

1. **crates/runtime-gateway（新增，ROADMAP P2-4 命名）**：
   - 心跳探测：过渡期 Runner 是本地 podman，心跳即本机命令可用性检查（ADR §6 原文口径）。探测方式为低频 `podman info` 短超时探活，阈值草案：间隔 5s、连续 3 次失败判 degraded、再 3 次判 offline；不 tight-poll（间隔下限遵守 1s 纪律），实现期写测试后冻结。
   - runtime_node 落库：迁移 `007_runtime_node.sql`（编号落库前实查），字段对齐计划 §12:696（id、kind、endpoint、health、version）；health 词表沿用 `tool_version.health` 四值 unknown/ok/degraded/broken（TOOL-CONTRACT D-TC-07 同款，不新增状态）。
   - WAITING_FOR_RUNNER 判定接通：Runner offline 时新派发 run 走 QUEUED 到 WAITING_FOR_RUNNER（状态机第 3 行，`state.rs:23`），心跳恢复后 WAITING_FOR_RUNNER 到 PREPARING（第 4 行）。**不伪装成工具失败**（计划 §5.6:336 原文语义）。
   - Runner 注册：python_worker（宿主进程池，复用 P1-6 产物）与 podman_host（本地 rootless podman）注册为同一 RuntimeNode 抽象的两个节点；心跳实现按 kind 分派。
2. **crates/tool-engine `adapter/podman_host.rs`（新增）**：
   - 容器创建：`podman run --rm`，默认 `--network none`（除非 Manifest 执行配置显式声明并经 Policy Resolver 生成，工具不得自行改变网络规则，ADR §4.3）、`--read-only`、`--userns=keep-id`（产物归属宿主用户，ADR §5 第 4 条）、`--pids-limit`/`--cpus`/`--memory` 按 §5.1.3 基线、容器名 `rip-tool-<run_id 短码>` 并打 `rip.runtime=rip-tool` label。
   - 镜像 digest 强校验：PREPARING 阶段 `podman inspect` 对账 Manifest 内 `@sha256:<64hex>`，tag 引用被 Registry 与 Adapter 双重拒绝（TOOL-CONTRACT 校验九条第 4 条同款）。
   - 挂载白名单：只允许挂载 `~/rip-runtime/workspaces/<run_id>/` 之下的 `in/`（只读）、`out/`（可写）、`tmp/`（tmpfs 封顶）。Adapter 侧路径归一化后校验，白名单外任何挂载请求直接拒绝（含 podman/docker socket、家目录本体、SSH agent、浏览器配置、`platform*.db`，即计划 §5.4:308 到 309 与 ADR §5 第 5 条的敏感清单）；负向测试覆盖。
   - 帧协议：stdin 一行 JSON 请求、stdout 一行 JSON 结果（D-TC-03 默认帧，与 worker 分帧一致，不新造传输层）。
   - 超时与协作式取消：平台侧看门狗到 budget 上限强制结束容器（TIMED_OUT）；取消走 RUNNING 到 CANCELLED（状态机第 12 行，协作式语义与 D-TC-04 一致）；取消与超时后回收容器并删 `tmp/`，`out/` 已产出部分按 partial Artifact 登记（ADR §5 第 3 条），不静默丢弃。
   - 输出收集：run 结束对 `out/` 全部产物计算 sha256 登记 artifact 表（005 已有表，role 词表 primary/attachment/stdout/stderr/log）；stdout/stderr 也入 artifact（005 表注释口径：结构化协议行以外的一律入本表）。
3. **Schema 与 Registry 适配**：`schemas/tool-manifest/v1.json` 的 runtime.type 词表追加 `podman_host`；新增 podman_host 分支必填字段，与 podman_vm 同构（image digest 固定、entrypoint，可选 user/readonly_rootfs/mounts/env，TOOL-CONTRACT 81 行同款）；Registry 校验九条对 podman_host 逐条适配（digest 固定、license、permission 等判据不变）。词表追加属向后兼容扩展，不破坏已登记 28 份 Manifest。
4. **Gateway 与 Engine 接缝**：Engine 提交路径在选 Runner 时消费 Gateway 的节点可用性；接口签名在 P2-1 定稿并写契约测试（Phase 1 先例：P1-4/P1-5 接口先冻结再并行）。

#### 5.1.2 容器约束清单逐条落地（计划 §5.4:304 到 317，逐条有测试）

| # | 计划 §5.4 约束 | podman_host 落地方式 | 测试判据 |
|---|---|---|---|
| 1 | 容器默认非 root 用户 | Manifest user 字段或镜像默认非 root 用户 | 容器内 `id -u` 非 0 |
| 2 | 根文件系统只读，写入目录单独挂载 | `--read-only`，写入仅 `out/`/`tmp/` | 写根路径失败 |
| 3 | 每次任务独立临时工作目录 | `workspaces/<run_id>/tmp`，任务结束删除 | 结束后目录不存在 |
| 4 | 只挂载当前任务所需输入输出目录 | 挂载白名单校验（§5.1.1 第 2 条） | 越界挂载请求被拒 |
| 5 | 不挂载家目录、SSH Agent、浏览器配置、系统目录 | 同上白名单 + 敏感路径黑名单断言 | 黑名单请求被拒 |
| 6 | 不挂载 Podman/Docker Socket | Adapter 拒绝一切 socket 路径 | socket 请求被拒 |
| 7 | 默认不使用 privileged | 不传 `--privileged` | inspect 断言 Privileged=false |
| 8 | 关闭不必要 capabilities | `--cap-drop=all`，按需最小 `--cap-add` 且须 Manifest 声明 | inspect 断言 |
| 9 | 设置 CPU、内存、PID、磁盘和执行时间上限 | §5.1.3 基线逐项注入 | §5.1.3 行为测试 |
| 10 | 网络由执行配置决定，工具不能任意改变 | 默认 `--network none` | 容器内无网络 |
| 11 | 镜像固定版本和 digest，更新先验证再切换 | PREPARING digest 强校验 + 登记 | tag 引用被双拒 |
| 12 | 任务结束清理临时容器与目录 | `--rm` + tmp 删除 | 容器与 tmp 不留存 |
| 13 | 宿主与 VM 之间只开 Gateway 通道 | 随 VM 批次挂起（§5.2）；宿主侧等价约束：容器默认不发布端口 | inspect 断言无 Ports |

#### 5.1.3 资源限制基线（照抄 ADR §7，ADR 184 到 199 行）

默认基线如下，执行配置可收紧；放宽必须经 Tool Policy Resolver 裁决并记录（计划 §5.4「设置 CPU、内存、PID、磁盘和执行时间上限」的具体化）：

| 维度 | 基线 | 依据与说明 |
|---|---|---|
| CPU | 每容器 2 核（`--cpus 2`） | 默认值，按工具族可下调 |
| 内存 | 2 GiB 默认；浏览器族上限 4 GiB | 默认值，防单容器拖垮宿主 |
| PIDs | `--pids-limit 256` | 防 fork 型失控 |
| 执行时长 | 默认 600000 ms，可下调不可上调 | 与 Worker budget 上限一致（worker_runtime.py 实测上限 600000 ms，TOOL-INVENTORY §1.1） |
| 输出大小 | 64 MiB 截断信封 | 与 Worker budget max_output_bytes 一致（同上） |
| 并发 | 每工具族默认 1 | 对齐项目低并发纪律（AGENTS.md：默认单线程或低并发） |
| 任务 tmp | tmpfs 挂载封顶 1 GiB | 超限任务改走显式磁盘工作区并留痕 |
| 镜像与缓存总量 | 初始预算 50 GiB | 本机可用 259G（实测），预留 VM 镜像与宿主余量；超出先清理再评审 |
| 根文件系统 | 容器根只读，写入仅显式挂载目录 | 计划 §5.4 原文 |
| 网络 | 默认关闭，按执行配置开放 | 计划 §5.4 |

照抄说明：表内容与 ADR:188 到 199 逐字一致（含 259G，为 ADR 撰写会话实测值）；本会话复核为 256G，仍高于 50 GiB 阈值（ADR 复审条件 4 联动）。

**真实生效的验证口径**：不是「命令带了参数」而是行为测试（P2-3 批次判据）。每项基线构造失败路径：pids 超限用容器内 fork 型负载观测终止；执行时长超限触发 TIMED_OUT 且容器被回收；输出超 64 MiB 触发截断信封（复用 P1-5 截断语义测试模式）；tmpfs 封顶用受控写满观测失败；CPU/内存若因 rootless cgroup 委派限制无法注入（风险 3），按「pids-limit + 超时看门狗 + tmpfs 封顶」兜底组合如实记录，不得虚报生效。

#### 5.1.4 文件交换与 Artifact（ADR §5 六步落在宿主通道）

工作区布局 `~/rip-runtime/workspaces/<run_id>/{in,out,tmp}`：输入登记 Artifact 后只读挂载进 `in/`；工具只在任务目录读写；结束由 Gateway 对 `out/` 全量 sha256 登记后归档，`tmp/` 用后即焚；超时与取消场景产出标注 partial 照常登记；Agent 与工具不接触宿主任意绝对路径（ADR §5 第 1 条原文口径）。幂等键沿用 Worker Protocol v1 的 submit_key 机制（005 表已落列），文件交换层不重复发明（ADR §5 第 7 条）。

#### 5.1.5 故障恢复（ADR §6 落在宿主通道）

Runner 离线：run 进入 WAITING_FOR_RUNNER，不伪装成工具失败；重连（心跳恢复）后按 Run ID + submit_key 幂等续走，不产生重复副作用。超时强制取消并回收容器、清理 tmp。Gateway 常驻挂 systemd user 单元（先例 rp-workbench）；Gateway 自身异常经审计事件留痕（audit-engine Emit，Phase 1 先例），不新增 JSONL 分叉。磁盘自愈：超 50 GiB 预算先清悬空镜像与孤儿容器再告警，不自动删任务工作区（ADR §6 末条口径）。

### 5.2 VM 目标拓扑设计（本阶段不实现，挂起）

本节是设计落盘，不是实现承诺；执行条件见 §3.2。四个组成部分与 ROADMAP 挂起批次一一对应。

#### 5.2.1 qemu:///session 会话级连接、家目录自建存储池（对应 ROADMAP P2-1）

- 全部 libvirt 操作走 `qemu:///session` 会话级连接（本会话 `virsh uri` 实测返回该 URI；PC2-2：系统级 `qemu:///system` 因无 sudo 不可用，会话级完全绕开 root 依赖；不依赖 quickemu，缺件且红线禁装）。
- 存储池自建：现会话级唯一池 tmp 指向 `/tmp`（tmpfs 16G，ADR §1.3），不可放 VM 镜像。P2-1 用 `virsh pool-define-as` 在家目录（设计落点 `~/rip-runtime/vm-pool`，实际路径该批定稿）自建池；云镜像选型落盘后下载并对账。完成判据（ROADMAP P2-1 判据原文）：「`virsh pool-define-as` 在家目录建池成功，卷下载后 sha256 对账通过（消解 ADR 待验证项 3）」。
- VM 名称沿用 ROADMAP §3.4 验收命令既用名 `rip-tool-vm`。

#### 5.2.2 qemu:///session 首启与基线快照（对应 ROADMAP P2-2）

- `virt-install` 于 `qemu:///session` 创建并首次启动 VM，KVM 加速（`/dev/kvm` crw-rw-rw- 本会话实测，PC2-4），无需软件模拟降级。
- 完成判据（ROADMAP P2-2 判据原文）：「`virsh domstate` 返回 running；基线快照可回滚（对应参考计划 §17 场景 4 的前置）」。基线快照即 VM 镜像的回滚点（ADR §6：「VM 镜像同理保留可回滚快照」）。

#### 5.2.3 VM 内 rootless Podman 与 subuid/subgid（对应 ROADMAP P2-3）

- VM 内新建普通用户并配置其 subuid/subgid（VM 内是新用户环境，配置步骤待验证，ADR §12.2）。
- 完成判据（ROADMAP P2-3 判据原文）：「VM 内用户配置 subuid/subgid 后 `podman ps` 退出码 0；`--network none` 冒烟容器跑通（消解 ADR 待验证项 2）」。
- 文件交换沿用 ADR §5 六步：Gateway 把任务文件映射或复制到 VM 任务目录；宿主与 VM 之间只开放 Tool Gateway 所需通信通道（计划 §5.4:317 末条）。

#### 5.2.4 podman_vm Runtime Adapter（对应 ROADMAP P2-6）

- 完成判据（ROADMAP P2-6 判据原文）：「真实工具从 RIP API 派发到 VM 内容器运行并登记 Artifact（计划 §14:772 验收第 1 条）」。
- 实现形态与 podman_host Adapter 同构：§5.1.2 约束清单、帧协议、digest 强校验、超时与协作式取消、Artifact 登记全部平移，差异只在宿主层：容器命令经 VM 内执行通道下发，心跳从本地探活换成 VM 心跳。这正是 ADR §8 正面结论「两步约束同构，迁移时只换宿主层，Tool Manifest、Artifact 纪律、资源基线、网络基线全部平移，不返工」的落点。
- VM 内执行通道选型（VM 内轻量 SSH 配会话级 user 模式端口转发，或 VM 内 agent 反连 Gateway）留待解除挂起时定稿，不预先承诺。

#### 5.2.5 VM 阶段基线与挂起批次清单

VM 阶段资源基线同一张表（§5.1.3，ADR §7 是两步共用的基线）；VM 磁盘落 50 GiB 镜像预算内，低于阈值触发 ADR 复审条件 4；工具容器 `--network none` 语义不变。挂起批次与解除条件：

| 挂起批次 | 对应 ROADMAP | 内容 | 解除条件 |
|---|---|---|---|
| S1 | P2-1 | 会话级存储池自建 + 云镜像选型落盘 | §3.2 充分条件启动建设路径或提前触发条件任一命中 |
| S2 | P2-2 | VM 首启（qemu:///session，KVM）+ 基线快照 | 同上，且依赖 S1 |
| S3 | P2-3 | VM 内 rootless Podman + subuid/subgid + 容器冒烟 | 同上，且依赖 S2 |
| S4 | P2-6 | podman_vm Runtime Adapter 端到端 | 同上，且依赖 S3 与 Gateway（P2-1 批次产物） |

连带说明：ROADMAP P2-7/P2-8 的 VM 面（镜像缓存 VM 面、VM 离线演练）随 S4 后解锁；其宿主侧等价物（镜像 digest 固定、Runner 离线恢复测试）在本阶段 P2-3/P2-4 先行覆盖，不留缺口。

## 6. 实施批次

本阶段共 6 批（P2-0 到 P2-5）；P2-0 即本文档。每批完成判据即该批验收命令通过并出批次验收报告（参考计划 §24 批次模板）。

| 批次 | 内容（对应关系） | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P2-0（本批，已完成） | 阶段说明与拓扑设计（本文档，§24 十节） | 十节齐备；VM 挂起与解除条件落盘；ADR §7 基线照抄；无长横线 | 起点 |
| P2-1 | Gateway 骨架：runtime_node 迁移（编号落库前实查）+ 心跳探测 + RuntimeNode.health 落库 + WAITING_FOR_RUNNER 判定接通 + systemd user 单元（ROADMAP P2-4 宿主侧） | 人为停 Runner 后新派发 run 进入 WAITING_FOR_RUNNER 而非 FAILED（计划 §5.6:336 原文语义）；心跳恢复后回 PREPARING；`cargo test --offline -p runtime-gateway` 过 | 【串行】起点 |
| P2-2 | podman_host Adapter 基础执行面：Schema 词表追加 podman_host + Registry 适配 + 容器创建 + 约束清单第 1 到 8、10 到 12 条 + 帧协议 + 挂载白名单 + keep-id 产物归属（ROADMAP P2-5 前半） | 约束逐条负向契约测试；真实容器命令在本机 rootless podman 跑通；产物属主为宿主用户 | 【串行】依赖 P2-1 |
| P2-3 | 资源限制真实生效 + 输出收集 + Artifact 登记：§5.1.3 基线逐项行为测试 + partial Artifact + 截断信封（ROADMAP P2-5 后半 + ADR §7） | 每项基线有失败路径测试；`out/` 产物 sha256 与 artifact 表对账一致 | 【串行】依赖 P2-2 |
| P2-4 | 故障恢复：Runner 离线与重连 + 幂等重放 + 超时取消回收 + tmp 清理（ADR §6 宿主面） | 取消后容器与 tmp 不留存；重放无重复副作用；离线期 run 状态不乱、恢复后不错报 FAILED | 【串行】依赖 P2-3 |
| P2-5 | 端到端收口：首个容器族 Containerfile + 真实容器化工具从派发到 Artifact 全链 + §8 验收命令全过 + 批次验收报告（ROADMAP P2-5 验收判据） | 真实工具经 RIP 派发到宿主容器跑通并登记 Artifact（ROADMAP P2-5 判据原文口径）；§8 全部命令按预期退出 | 【串行】依赖 P2-1 到 P2-4 |

挂起批次 S1 到 S4 见 §5.2.5（对应 ROADMAP P2-1/P2-2/P2-3/P2-6）。本阶段完成后 Runner 全景：python_worker（Phase 1）、podman_host（本阶段），podman_vm 维持设计挂起态；三方共存到 VM 通道解锁后按 ADR §3 第 2 条切流。

## 7. 风险

1. **宿主 podman 实例与用户自用容器共用**（§4.2 实测 3 个自用容器）：Gateway 清理逻辑误伤用户容器是本阶段最高危面。缓解：容器名 `rip-tool-*` 前缀 + `rip.runtime=rip-tool` label 双重过滤，清理函数只处理自有标识并写负向测试（对用户容器标识断言不触碰）；禁用系统级 prune。
2. **共享内核攻击面**（ADR §8 负面第 1 条）：rootless 消除特权提升面但不消除内核面，过渡期隔离弱于 VM 是已知代价；由 §3.2 提前触发条件 3 兜底（越界污染事件即暂停新增登记并加速迁移）。
3. **rootless 下资源限制的真实生效性**：`--cpus`/`--memory` 依赖 user 级 cgroup 委派，本机委派可用性待验证（§4.3）。这是本阶段最大技术不确定项。缓解：P2-3 以行为测试裁决；不可用则降级为 pids-limit + 超时看门狗 + tmpfs 封顶兜底组合并在验收报告如实记录，不虚报。
4. **心跳误判抖动**：阈值与迟滞（连续缺席计数）在 P2-1 定稿并测试；间隔下限 1s，不 tight-poll；误判只产生 WAITING_FOR_RUNNER（可恢复态），不产生 FAILED（终态），损害有界。
5. **负载镜像供应链风险**：资源测试负载脚本自研并随 Containerfile 入仓库，基础镜像 digest 固定；不引入第三方压测镜像。
6. **双 Runner（python_worker 与 podman_host）测试矩阵扩大**（ADR §8 负面第 2 条的宿主内版本）：Gateway 用统一 RuntimeNode 抽象承载两类节点，python_worker 心跳复用现有进程探测语义，不另起一套协议。
7. **磁盘增长**：镜像拉取属运行时数据不属安装（ADR §3.2 口径），总量受 50 GiB 预算约束；P2-3 起验收含 `df` 阈值检查，低于 50G 触发 ADR 复审条件 4。
8. **VM 挂起期的流程债**：提前触发条件命中后 S1 到 S4 需要连续执行窗口（云镜像下载等外部依赖）。缓解：设计已落盘（本文 §5.2），解锁即开工；按 ROADMAP §3.5 风险 1 口径，VM 首批失败不阻塞宿主车道，`podman_host` 通道保持可用。

## 8. 验收

阶段验收命令（P2-5 收口时全跑；各批次判据命令随批次验收报告落盘）。文中方括号为占位符，执行时按实际值替换。

~~~bash
cd /home/emo/code/pensting/research-platform

# 0. 质量门（与 CI 同命令，ci.yml 实测原文）
cargo test --offline --workspace
cargo clippy --offline --workspace --all-targets -- -D warnings

# 1. 文档自检（P2-0）
test -f docs/phase2/PHASE2-DESIGN.md && echo DOC_OK
grep -P '[\x{2014}\x{2013}]' docs/phase2/PHASE2-DESIGN.md; echo "dash_grep_exit=$?"   # 期望退出码 1（零命中）

# 2. 前提复核（只读，随时可跑；ROADMAP §3.4 同款）
timeout 5 sudo -n true; echo "exit=$?"                                               # 期望 1
podman info --format '{{.Host.Security.Rootless}} {{.Store.GraphRoot}}'              # 期望 true 与家目录
virsh uri                                                                            # 期望 qemu:///session（VM 面挂起，不跑任何 virsh 写操作）
df -BG --output=avail / | tail -1                                                    # 期望 ≥ 50

# 宿主机没有新增工具依赖（计划 §14:772 验收第 2 条）检查口径（ROADMAP §3.4 同款）：
# Phase 2 全程对照 pacman -Q 输出无变化（基线 2415 行，§4.4 实测）且无全局 pip/npm 安装；
# 镜像拉取属运行时数据不属安装（ADR §3.2 口径）。

# 3. WAITING_FOR_RUNNER（P2-1 判据，命令形式随批次定稿，形如）
cargo test --offline -p runtime-gateway waiting_for_runner

# 4. 容器越界负向检查（验收第 3 条宿主面）：派发一个尝试读宿主任意路径的容器化工具 run，
#    期望挂载白名单拒绝或读取不到；socket 与敏感路径挂载请求被 Adapter 拒绝（P2-2 测试）。

# 5. Artifact 登记（验收第 4 条，只读对账开发库副本，不触生产库）
python3 -I -c "import sqlite3; c=sqlite3.connect('file:<开发库副本路径>?mode=ro',uri=True); print(c.execute('select run_id,status,tool_id from tool_run order by rowid desc limit 3').fetchall())"
sha256sum <artifact_uri 指向的文件>                                                   # 与 artifact.sha256 列一致

# 6. 运行时清理面复核（不碰用户容器）
podman ps -a --filter label=rip.runtime=rip-tool --format '{{.Names}}'               # 只列 rip-tool-* 自有容器
~~~

计划 §14:772 五项验收在本阶段的达成口径：

| 验收项（计划原文） | 本阶段达成方式 | 状态 |
|---|---|---|
| 真实工具可从 RIP API 派发到 VM 运行 | 派发到宿主 rootless 容器运行（P2-5 全链） | 宿主面达成；VM 面挂起，如实标注 |
| 宿主机没有新增工具依赖 | 全程 pacman -Q 无变化口径（见上） | 达成 |
| 容器不能访问宿主机任意目录 | 挂载白名单负向测试（P2-2/P2-5） | 达成 |
| 输出可登记 Artifact | P2-3/P2-5 sha256 对账 | 达成 |
| VM 离线不会导致任务状态错乱 | 宿主 Runner 离线等价（P2-4，WAITING_FOR_RUNNER + 幂等重放） | 宿主面达成；VM 字面演练挂起，如实标注 |

## 9. 回滚

- **代码**：本阶段每批次独立 commit，`git revert <批次 commit>` 即整体撤销该批；revert 不触及 M0 到 M8 与 Phase 1 交付物（红线），tool-engine 既有行为由回归测试守护。
- **迁移**：007（编号落库前实查）为幂等 DDL，回滚按 crates/database 迁移执行器的既有回退流程走开发库副本；生产 `platform*.db` 全程未写入（红线），无生产回滚面。
- **运行时清理**（顺序执行，label 过滤保证不碰用户容器）：
  ~~~bash
  podman rm -f $(podman ps -aq --filter label=rip.runtime=rip-tool)
  podman rmi $(podman images -q --filter label=rip.runtime=rip-tool)
  rm -rf ~/rip-runtime/workspaces/*
  systemctl --user disable --now rip-gateway.service
  ~~~
- **Schema**：runtime.type 词表追加属向后兼容（新增枚举值），回滚还原 `schemas/tool-manifest/v1.json` 与 validate.rs 对应分支及其测试；已登记 podman_host 工具条目随开发库副本回滚，不触生产库。
- **VM 面**：本阶段未执行任何 VM 操作，无 VM 回滚面；S1 到 S4 解锁后的回滚方式随该批设计补充（基线快照可回滚本身是 S2 的完成判据）。

## 10. 交付物

- **代码**：`crates/runtime-gateway`、`crates/tool-engine/src/adapter/podman_host.rs`、`crates/domain` 类型扩展（如需）、`crates/tool-registry` 校验适配、`migrations/007_runtime_node.sql`（编号落库前实查）、`schemas/tool-manifest/v1.json` 词表扩展、`deploy/`（rip-gateway systemd user 单元与编排脚本）、首个容器族 `Containerfile` 与自研负载脚本。
- **文档**：本文档（`docs/phase2/PHASE2-DESIGN.md`）；P2-1 到 P2-5 各批次验收报告（`docs/phase2/` 下，按参考计划 §24 批次验收报告模板，含命令原文与输出摘要）。
- **测试**：契约测试与负向测试（§5.1.2 约束清单逐条、§5.1.3 基线逐项失败路径、WAITING_FOR_RUNNER、幂等重放、越界拒绝、用户容器不触碰）。
- **数据**：无生产库写入（红线）；开发库副本上的 runtime_node 表与测试 run/artifact 行。
