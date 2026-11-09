# TOOL-RUNTIME-ADR：RIP 工具执行的隔离拓扑（Phase 0 交付物 3）

- 日期：2026-11-09
- 状态：已接受（Accepted）
- 决策范围：RIP 工具执行面的隔离拓扑、网络边界、文件交换与 Artifact 纪律、故障恢复、资源限制基线
- 对应条目：`docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md` §16 第 3 条（本文档交付物定位）、§5（推荐基准）、§6（Runtime 类型表）、§15 Phase 2（验收标准）、§18（核心决策）
- 红线遵守声明：本文为决策文档，撰写过程未修改任何源文件，未写 `migrations/` 下任何 `platform*.db`，未在宿主机安装任何依赖，未启动任何虚拟机（Phase 0 红线）。所有运行时结论来自本会话实跑命令（见 §11 实测记录）。

---

## 1. 背景（Context）

### 1.1 决策动机

2.0 计划的核心决策是「工具实际运行在独立 VM 的 Podman 环境中，宿主机只负责平台本身，不承担 AI 临时安装依赖的工作」（计划 §18）。但工具执行面当前为零：

- `tool_execution` 表 0 行空结构，VM + Podman Runner、Tool Registry、Run/Artifact/Evidence 标准记录全部待建（CURRENT-ARCHITECTURE-INVENTORY.md §12 差距清单第 3 条）。
- Worker Protocol v1（5 方法 + budget + 幂等 + 取消）齐备，但全仓库无常驻宿主派发器，worker 只能被测试 harness 驱动（同上差距清单第 4 条，grep 实测命中 0 行）。
- 两份 Containerfile 只覆盖平台自身的构建与部署，不等于 2.0 工具运行架构（同文档 §9.2）。
- 工具资产侧已识别 12 组可登记资产，§4.2 十三工具族中八族空白（Web/HTTP、DNS、Network、Web Analysis、Source Code、Mobile/APK、Binary/Reverse、Browser Automation，TOOL-INVENTORY.md §7），补齐将引入大量异构第三方依赖（Python/Node/Java/Go 各版本），隔离拓扑必须先于工具引入定案。

### 1.2 本机硬约束

- 无免密 sudo：`timeout 5 sudo -n true` 输出「sudo: 需要密码」且退出码 1（本次实测）。
- 红线：不在 CachyOS 宿主机安装任何依赖或工具。

因此拓扑方案的第一筛选条件是：**零安装、零 sudo 即可落地**。

### 1.3 运行时实测结论（本次会话逐条实跑，命令与输出见 §11）

| 探测项 | 结果 | 对拓扑的含义 |
|---|---|---|
| podman 版本与 rootless | 6.1.4，Rootless=true，GraphRoot=`/home/emo/.local/share/containers/storage`，overlay 驱动，`podman ps` 退出码 0 | 宿主 rootless 容器立即可用，存储已在用户家目录内 |
| subuid/subgid | `emo:100000:65536` 两条已配置 | rootless 容器 uid 映射空间充足 |
| qemu/libvirt 工具链 | qemu-system-x86_64、qemu-img、libvirtd、virsh、virt-install 全部在位；quickemu 不存在 | VM 路线必须基于 qemu/libvirt，quickemu 缺件且红线禁装 |
| libvirt 连通性 | libvirtd active；`virsh uri` 返回 `qemu:///session` 退出码 0 | VM 可走用户会话级 libvirt，完全绕开无 sudo 限制 |
| 会话级既有资源 | 存储池 tmp（活动，指向 `/tmp`）；虚拟机列表为空 | 无既有 VM 可复用；`/tmp` 为 tmpfs（16G，实测），不适合放 VM 镜像，Phase 2 需自建家目录内会话级存储池 |
| /dev/kvm | crw-rw-rw-，读写双向可用 | 硬件加速可用，VM 性能可行 |
| 磁盘 | 根分区可用 259G | 足够容纳 VM 镜像与容器存储 |
| systemd user 管理器 | transient unit 创建成功；rp-workbench.service active（running）；整体 degraded 但 3 个失败单元与 RIP 无关 | 常驻 Gateway 可挂 user 级单元，有先例 |
| docker | 29.9.3 在位且当前用户可用 | 按既有策略仅记录不使用，不作为 Runner |
| sudo | 无免密 sudo（退出码 1） | 排除一切需要 root 的拓扑变体 |

待验证：VM 端到端全流程（云镜像准备、VM 启动、VM 内 rootless Podman 跑真实工具）从未执行，Phase 0 红线禁止本次启动新虚拟机。

---

## 2. 备选方案（Considered Options）

### 方案 A：工具直接安装在 CachyOS 宿主机

- 评估：隔离性低、依赖冲突高、宿主污染高（计划 §5.2 原判定）。本机加倍不可行：无 sudo 使多数工具链安装本身都做不到，且与红线「不在宿主机安装依赖」正面冲突。
- 结论：**否决**。docker（宿主面 29.9.3）同理不作为工具执行面。

### 方案 B：单巨型容器运行全部工具

- 评估：一个容器装下所有工具，重新制造依赖冲突（不同工具需要不同版本的 Python/Node/Java/Go 与系统库），一个工具升级影响全部工具（计划 §5.3 原判定）。隔离性中等，无法按工具族施加差异化资源与网络策略。
- 结论：**否决**。容器化方向保留，但必须按工具/工具族拆分。

### 方案 C：专用 VM 内 rootless Podman 分容器（计划 §5.1 推荐）

- 评估：隔离性最高（容器之上再有一层 VM 边界，宿主污染「很低」）。本机可行性：前置条件全部实测在位（§1.3 表），无需安装、无需 sudo。代价：端到端未验证（待验证标注），且属于计划 Phase 2 的完整工作量（云镜像选择与下载、VM 内 rootless Podman 配置、Gateway 心跳与文件交换通道、镜像缓存策略）；VM 镜像日常维护（升级、回滚、快照）是长期成本。
- 结论：**采纳为唯一目标拓扑**，但不作为当下第一步（理由见 §3）。

### 方案 D：宿主机 rootless Podman 分容器

- 评估：隔离性中高（rootless + subuid 100000:65536 映射 + 非特权用户 + 家目录内 overlay 存储，实测全部就绪），宿主污染面从「系统全局」收缩到 `~/.local/share/containers/storage` 与显式挂载的任务目录。零安装、零 sudo、立即可落地（`podman ps` 退出码 0 实测）。与方案 C 的容器运行约束（计划 §5.4 清单）可以逐条同构执行，迁移时约束不变、只换宿主层。弱点：与宿主共享内核，内核攻击面仍在，隔离边界弱于 VM；容器存储增长占用家目录磁盘。
- 结论：**采纳为起步执行拓扑（过渡）**。

### 附：每工具单独 VM

隔离性最高但维护成本「很高」（计划 §5.2 原判定），本机磁盘虽够（259G）但镜像维护人力不可持续。仅当未来出现需要内核级隔离或特殊设备访问的个别工具时作为特例标记，不作为通用拓扑。

### 对比表（本机口径，新增「当下可落地性」列）

| 方案 | 隔离性 | 维护成本 | 依赖冲突 | 宿主污染 | 当下可落地性 | 结论 |
|---|---|---|---|---|---|---|
| A 宿主机直装 | 低 | 长期高 | 高 | 高 | 不可落地（无 sudo + 红线） | 否决 |
| B 单巨型容器 | 中 | 中高 | 容器内仍冲突 | 低 | 可落地但否决 | 否决 |
| C 专用 VM 内 rootless Podman | 高 | 中 | 低 | 很低 | 前置条件全部在位，端到端待验证 | 目标拓扑 |
| D 宿主机 rootless Podman 分容器 | 中高 | 中 | 低 | 低（收缩到家目录存储） | **立即可落地（零安装实测）** | 起步拓扑 |

---

## 3. 决定（Decision）

**目标拓扑不变：专用 VM + VM 内 rootless Podman 分容器（计划 §5.1），不推翻、不降级。**

**执行分两步走：宿主机 rootless Podman 分容器起步（过渡 Runner），VM 端到端验收达成后迁移切换。**

具体裁定：

1. **第一步（本决定生效即执行）**：新增 Runtime 类型 `podman_host`（宿主 rootless Podman 分容器），作为过渡 Runner 接入 Tool Engine。容器运行约束逐条套用计划 §5.4 清单（rootless、非 root 容器用户、根文件系统只读、独立临时工作目录、只挂载任务目录、不挂 socket、非 privileged、capabilities 收敛、资源上限、digest 固定），这些约束不依赖 VM 存在，在宿主 rootless Podman 上同样成立。首批容器族按 §5.3 Tool Pack 原则拆分，先覆盖 TOOL-INVENTORY 已登记资产的运行需求与八族空白的补齐需求。
2. **第二步（迁移目标）**：按计划 Phase 2 建设 VM 通道，Runtime 类型 `podman_vm` 验收达成后成为默认 Runner；`podman_host` 降级为本地开发与应急通道，不再承载生产工具运行。
3. **Runtime 类型全景**：本决定只新增 `podman_host` 一个过渡类型；计划 §6 表中 `python_worker`（现有 3 worker，P0）、`native_rust`、`mcp_client`、`http_api`、`cli_adapter` 的定位不变。禁止把宿主机任意 Shell 当默认 Runtime 的红线（计划 §6 原文）在两步中同等生效。

### 3.1 迁移触发条件

**充分条件（达成即执行切换）**：

- VM 端到端冒烟全部通过，即计划 Phase 2 验收五项达成：真实工具可从 RIP API 派发到 VM 运行；宿主机没有新增工具依赖；容器不能访问宿主机任意目录；输出可登记 Artifact；VM 离线不会导致任务状态错乱（计划 §15 Phase 2 原文）。前置子项包括：云镜像准备、VM 以会话级 libvirt 启动（qemu:///session 实测可用）、VM 内 rootless Podman 配置（VM 内 subuid/subgid 配置属 Phase 2 工作项，待验证）、Gateway 心跳与文件交换通道打通、会话级存储池自建（现池 tmp 指向 /tmp 为 tmpfs 16G，实测不可用于 VM 镜像，需 `virsh pool-define-as` 在家目录自建，待验证）。

**提前触发条件（任一命中即启动 VM 通道建设，不得等充分条件自然达成）**：

1. 登记第一个不受信第三方工具（如 TOOL-INVENTORY §4.3 的 TPscan，许可证核实后准入）时，该工具必须跑在 VM 内，不落 `podman_host`。
2. 某工具的执行配置超出宿主 rootless 可表达范围：需要真实浏览器环境（Browser Automation 族补齐，参考 TELEGRAM-ADMIN-UI-REUSE.md §2.5 视觉回归的浏览器需求）、需要特权网络操作或特殊内核能力、需要与宿主完全隔离的运行时。
3. `podman_host` 发生越出任务目录与容器存储目录的污染事件（容器逃逸出预期边界），立即暂停新增 `podman_host` 工具登记并加速迁移。

### 3.2 过渡期纪律

- `podman_host` 只运行已登记且来源可信（自研或许可证核实）的工具；不受信工具一律等待 VM。
- 生产 RTIS 与 `platform*.db` 不进任何工具容器（含过渡与目标拓扑）；只读数据访问走已登记只读路径（scripts 三脚本 sqlite mode=ro，TOOL-INVENTORY §2 实测；RTIS 只做 Adapter 登记不改本体，TOOL-INVENTORY §5）。
- docker 全程不作为 Runner（在位仅记录，策略不变）。
- 宿主机不因 `podman_host` 产生任何新安装：镜像拉取（`podman pull`）属运行时数据不属依赖安装，与红线不冲突，但镜像总量受 §8 磁盘预算约束。

---

## 4. 拓扑与网络

### 4.1 过渡拓扑（第一步，当下生效）

~~~text
CachyOS 宿主机（emo, 无 sudo）
├── RIP 平台面
│   ├── rp-workbench.service（systemd user，已运行，实测 active）
│   ├── Tool Gateway（新建，systemd user 单元）
│   └── migrations/platform.db（生产库，只出不进）
└── 工具执行面（podman_host，rootless，存储在 ~/.local/share/containers）
    ├── rip-tool-python（Python 工具族）
    ├── rip-tool-web（Web/HTTP 族）
    ├── rip-tool-source（Source Code 族）
    ├── rip-tool-document（Document 族）
    └── rip-tool-browser（Browser Automation 族，VM 前仅登记不承载不受信负载）
~~~

### 4.2 目标拓扑（第二步，Phase 2 后）

~~~text
CachyOS 宿主机
├── RIP 平台面（不变）
└── Tool VM（qemu:///session 会话级 libvirt，KVM 加速实测可用）
    └── VM 内 rootless Podman
        ├── rip-tool-web / rip-tool-network / rip-tool-source ...
        ├── Tool Cache（镜像缓存）
        └── Task Workspaces + Artifact Staging
~~~

### 4.3 网络基线

- 工具容器默认无网络（`--network none`）；需要出网的工具在 Tool Manifest 执行配置中显式声明，由 Tool Policy Resolver 生成网络配置，工具不得自行改变网络规则（计划 §5.4 原文口径）。
- 一律不向工具容器挂载 Podman/Docker socket（计划 §5.4；docker 在位更要求此条严格执行）。
- 容器间默认不互通；同族容器如需协作走宿主回环上的显式声明端口。
- VM 阶段：宿主与 VM 之间只开放 Tool Gateway 所需通信通道（计划 §5.4 末条）。

---

## 5. 文件交换与 Artifact 纪律

原则沿用计划 §5.5 六步流程，落到两步拓扑上：

1. **工作区布局**：RIP 建立运行时根目录（建议 `~/rip-runtime/`），下分 `workspaces/<run_id>/`，内含 `in/`（输入，只读挂载）、`out/`（输出，可写）、`tmp/`（临时，用后即焚）。Agent 与工具不接触宿主任意绝对路径。
2. **输入登记**：文件进入工作区时即生成 Artifact ID、sha256、大小、来源元数据（计划 §5.5 第 2 步）。容器内以只读绑定挂载 `in/`。
3. **输出登记**：Run 结束由 Gateway 对 `out/` 全部产物计算 sha256 并登记 Artifact 后归档；`tmp/` 直接删除。部分产出（超时/取消场景）标注 partial 后照常登记，不静默丢弃。
4. **所有权对齐**：rootless Podman 下容器写入默认映射到 subuid 段，统一使用 `--userns=keep-id` 保持产物归属宿主用户（emo），避免产物变成映射 uid 导致后续流程不可读。此条依赖 subuid/subgid 实测就绪（`emo:100000:65536`）。
5. **敏感边界**：`migrations/` 下全部 `platform*.db`、`.rp-workbench-token`、SSH agent、浏览器配置、家目录本体一律不挂载进容器（计划 §5.4 原文清单）。
6. **证据链关联**：每个 Run 记录输入/输出 Artifact ID、工具版本、命令原文、退出码，满足计划 §3「所有工具执行产生标准化 Run、Artifact、Evidence 和日志」。Python worker 与 rtis 侧审计仍是 JSONL 分叉（架构盘点差距清单第 5 条），不在本 ADR 范围内解决，但 Artifact 登记必须携带 run_id 使两条审计轴可关联。
7. **幂等键贯穿**：Run 的幂等提交键沿用 Worker Protocol v1 的 `idempotency_key` 机制（worker_runtime.py 已实现结果缓存语义，TOOL-INVENTORY §1.1 实测），文件交换层不重复发明。

---

## 6. 故障恢复

对齐计划 §5.6，按两步拓扑落实：

- **心跳与状态**：Gateway 对 Runner 心跳探测。过渡期 Runner 是本地 podman（心跳即本机命令可用性检查）；VM 阶段为 VM 心跳。Runner 离线时任务进入 `WAITING_FOR_RUNNER`，不伪装成工具失败（计划 §5.6 原文）。
- **幂等重放**：Run ID + 幂等提交键，重连/重试后不产生重复副作用；VM 重连后查询已完成与运行中任务（计划 §5.6）。
- **超时与取消**：超 budget 强制取消，取消后回收容器并清理 `tmp/`；`out/` 已产出部分按 partial Artifact 登记。
- **镜像回滚**：镜像固定版本与 digest，更新先验证再切换，升级失败回滚旧 digest（计划 §5.4/§5.6）。VM 镜像同理保留可回滚快照。
- **宿主常驻服务**：Gateway 与过渡 Runner 编排挂 systemd user 单元（先例：rp-workbench.service active，实测）。user manager 当前 degraded 不构成阻断（transient unit 创建成功，3 个失败单元 app-deepseek-harness@...、elephant.service、zcode-guard.service 与 RIP 无关，实测）。
- **磁盘自愈**：镜像与缓存增长超预算时先清悬空镜像与孤儿容器，再告警；不低于 §8 磁盘阈值不自动删任务工作区。

---

## 7. 资源限制基线

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

---

## 8. 后果（Consequences）

### 正面

- 当下即可开工：零安装、零 sudo，`podman_host` 全部前置实测就绪，工具执行面从零解封，八族空白补齐不再被拓扑阻塞。
- 两步约束同构：过渡期执行的 §5.4 容器约束清单与 VM 阶段完全一致，迁移时只换宿主层，Tool Manifest、Artifact 纪律、资源基线、网络基线全部平移，不返工。
- 宿主污染面可控：容器存储收敛到家目录，生产库与 RTIS 被明确挡在容器面之外。
- 不推翻任何已交付能力：`python_worker`、Rust crates、RTIS 回滚机制、rp-workbench 均不动（红线与 M0-M8 保持）。

### 负面

- 过渡期隔离弱于目标：与宿主共享内核，rootless 只消除特权提升面，不消除内核攻击面；这是接受过渡方案的已知代价，由 §3.1 提前触发条件 1/3 兜底。
- 双拓扑并存期的实现与测试矩阵翻倍：Gateway 需同时支持两种 Runner 通道（至少到切换完成）。
- VM 镜像建设是 Phase 2 的实质工作量：云镜像选择下载、VM 内 Podman 与 subuid 配置、存储池自建，全部待验证，不可因「工具链在位」而低估。
- 容器存储占用家目录磁盘（预算 50 GiB 封顶缓解）。

### 中性

- quickemu 缺件无影响：路线基于 qemu/libvirt（实测在位），不依赖 quickemu。
- docker 在位不用：维持既有策略，仅记录。
- 已登记的纯标准库工具（workers 三件套、scripts 三脚本、rtk、RTIS kit）多数可继续走 `python_worker`/Adapter 通道，不强制全部容器化；容器化优先级属于 Tool Registry 阶段的排期决策，不在本 ADR。

---

## 9. 复审条件（Re-evaluation Triggers）

出现以下任一情况时重新评审本 ADR：

1. VM 端到端冒烟达成（§3.1 充分条件）：复审切换时点与 `podman_host` 退役计划。
2. §3.1 提前触发条件 1、2、3 任一命中。
3. podman 大版本升级（6.x 到 7.x）或宿主内核大版本变更，需重验 rootless 行为与 `--userns=keep-id` 语义。
4. 根分区可用磁盘低于 50 GiB（当前 259G，实测）：复审镜像预算与 VM 镜像落点。
5. 2.0 计划 §5/§6 修订或 Phase 2 验收标准变化。

---

## 10. 与三份 Phase 0 盘点文档的关系

- CURRENT-ARCHITECTURE-INVENTORY.md：本 ADR 落地后消解其差距清单第 3 条（工具执行面为零）的拓扑部分；第 4 条（Worker 宿主派发器缺失）由 Tool Gateway 建设消解，Gateway 即 worker 的第一个常驻派发器。
- TOOL-INVENTORY.md：12 组登记资产的运行通道按 §3 决定第 3 条分配；八族空白补齐所需异构依赖全部进容器，不进宿主。
- TELEGRAM-ADMIN-UI-REUSE.md：与运行拓扑无直接耦合；其 §2.5 视觉回归的浏览器需求是 Browser Automation 族与 §3.1 提前触发条件 2 的输入之一。

---

## 11. 实测记录（本会话执行，2026-11-09）

| 命令 | 输出（摘要） |
|---|---|
| `podman --version` | `podman version 6.1.4` |
| `podman info --format '{{.Host.Security.Rootless}} {{.Store.GraphRoot}} {{.Store.GraphDriverName}}'` | `true /home/emo/.local/share/containers/storage overlay` |
| `podman ps` | 退出码 0（列空） |
| `grep '^emo:' /etc/subuid /etc/subgid` | 两行 `emo:100000:65536` |
| `timeout 5 sudo -n true` | `sudo: 需要密码`，退出码 1 |
| `ls -l /dev/kvm` | `crw-rw-rw- 1 root kvm 10, 232`；`test -r`、`test -w` 均 YES |
| `command -v`（qemu-system-x86_64 / qemu-img / libvirtd / virsh / virt-install / quickemu / docker） | 前 5 项在位；quickemu NOT_FOUND；docker=/usr/sbin/docker |
| `systemctl is-active libvirtd.service` | `active` |
| `virsh uri` | `qemu:///session`，退出码 0 |
| `virsh -c qemu:///session pool-list --all` | 仅 `tmp` 池（活动，自动开始） |
| `virsh -c qemu:///session pool-dumpxml tmp` | target path = `/tmp` |
| `df -h /tmp` | tmpfs，16G 总量 |
| `virsh -c qemu:///session list --all` | 空（无既有虚拟机） |
| `df -BG --output=avail /` | 259G |
| `systemctl --user is-system-running` | `degraded` |
| `systemctl --user status rp-workbench.service --no-pager -n 0` | active (running)，enabled |
| `systemd-run --user --scope --unit=rt-adr-probe.scope true` | 成功创建 transient unit |

## 12. 待验证清单

1. VM 端到端全流程（云镜像、启动、VM 内 rootless Podman 跑真实工具）：Phase 0 红线禁止本次启动虚拟机，全部留待 Phase 2 首个执行项。
2. VM 内 rootless Podman 的 subuid/subgid 配置：VM 内是新用户环境，配置步骤待验证。
3. 会话级 libvirt 自建家目录存储池（`virsh pool-define-as` 于 qemu:///session）：未执行，待验证。
4. GitHub runner 上 podman 的可用性（CI 端到端测试若引入容器化 Runner）：架构盘点 §9.1 已标注待验证，本 ADR 不重复展开。
5. `--userns=keep-id` 与具体镜像组合的行为回归：`podman_host` 首个容器族落地时实测。
