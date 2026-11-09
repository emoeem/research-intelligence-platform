# PHASE-ROADMAP：Phase 1 到 10 实施路线（Phase 0 交付物 8）

- 日期：2026-11-09
- 状态：已冻结（Frozen）。批次编号与依赖关系冻结，批次内部实现细节可在各阶段开工时按参考计划 §24 模板出阶段说明后微调。
- 定位：`docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md`（下称「计划」）§16 第 8 条交付物「按依赖关系冻结的后续实施计划」（计划:892）。
- 执行基准：计划 §14（实施路线与阶段边界，732 到 851 行）、计划 §18（最终建议与决策，915 到 926 行）；总体参考 `/home/emo/Downloads/RIP-完整项目实施计划.md` §23（建议的实际执行顺序，2273 到 2294 行）与 §24（每阶段交付模板，2296 行起）。
- 输入材料：docs/phase0/ 全部 7 份文档（CURRENT-ARCHITECTURE-INVENTORY、TOOL-INVENTORY、TOOL-RUNTIME-ADR、TELEGRAM-ADMIN-UI-REUSE、PLUGIN-CONTRACT-DRAFT、TOOL-CONTRACT-DRAFT、CONTEXT-BROKER-DESIGN）。
- 红线遵守：本文为纯规划文档。撰写过程只新增本文件，未修改任何源文件，未写 `migrations/` 下任何 `platform*.db`，未在宿主机安装任何依赖，未启动任何虚拟机。
- 证据口径：计划引用写（计划 行号），phase0 文档引用写（文档名 节号/行号）。运行时实测结论：本会话对无 sudo、rootless Podman、会话级 libvirt、KVM、磁盘五项做了只读复核（见 §2 Phase 2 前置条件表「本会话复核」列），其余实测记录摘自各文档自带的验证记录（其撰写会话实跑）。

---

## 1. 依赖冻结总览

### 1.1 阶段依赖图

~~~text
Phase 0（已完成，7 份交付物 + 本文件）
   │
   ├─→ Phase 1 Tool Contract 与 Registry
   │       │
   │       ├─→ Phase 2 Tool VM 与 Podman Runtime
   │       │       │
   │       │       ├─→ Phase 3 首批 Tool Packs
   │       │       │
   │       │       └─→（Run/Artifact 数据面）
   │       │               │
   │       └─→ Phase 4 Context Broker ←────（Mock 先行通道：P4 第一批起可早于 Phase 2）
   │               │
   │               └─→ Phase 5 Web Workbench 基础版 ←─（壳层通道：P5 第一批起可早于 Phase 2）
   │                       │
   │                       └─→ Phase 6 Plugin Runtime 与 Plugin Center
   │                               │
   │                               └─→ Phase 7 Agent Engine
   │                                       │
   │                                       └─→ Phase 8 Workflow Studio
   │                                               │
   │                                               └─→ Phase 9 MCP / REST / CLI
   │                                                       │
   │                                                       └─→ Phase 10 生态、可靠性与发布
~~~

### 1.2 冻结顺序

主干串行链（不可调换）：

```
Phase 1 → Phase 2 → Phase 3 → Phase 6 → Phase 7 → Phase 8 → Phase 9 → Phase 10
```

Phase 4 与 Phase 5 的主干位置在 Phase 3 之后（计划 §14 编号顺序），但各自的第一批可以提前开工，形成三条并行车道：

| 车道 | 批次 | 提前开工条件 | 汇合点 |
|---|---|---|---|
| 车道 A（Context Broker 先行） | P4-1、P4-2、P4-3 | Phase 1 的 Manifest Schema 与 domain 类型冻结（不需要 Registry 实现） | P4-4 起需要 Phase 2/3 的 Run 与 Artifact 数据 |
| 车道 B（Web 壳层先行） | P5-1、P5-2、P5-3、P5-9 | 只依赖 Phase 0 的 UI 复用盘点，不依赖任何新后端 | P5-5 起逐页接数据，按各页依赖排 |
| 车道 C（宿主容器通道先行） | P2-4、P2-5 | 只依赖 Phase 1 的调用协议与状态机定义 | P2-5 完成后 Phase 3 的自研可信工具批次即可开工 |

两条提前通道（不属于车道，属于批次级并行）：

- Phase 9 的 `rip.run_tool` 端到端通道（P9-2）可提前到 Phase 3 完成后实施，不等 Phase 6/7/8 的能力面齐全（依据：计划 §11 外部 AI 八步，676 到 679 行，只依赖工具派发链）。
- Phase 7 的 AgentDefinition 域模型与数据面（P7-1）可与 Phase 5/6 并行（纯类型与迁移，无运行时依赖）。

与参考计划 §23 执行顺序的映射（每条一步，序号为 §23 原文序号）：

| §23 步骤 | 对应本路线 |
|---|---|
| 1 复核 M0 与仓库状态 | Phase 0 已完成（盘点 HEAD 022a717 实测一致） |
| 2 到 5（Domain Model、Rust 骨架、只读接入、Knowledge/Evidence 统一） | M0 到 M8 已交付，红线不推翻（计划 §15:873） |
| 6 Tool Contract / Registry | Phase 1 |
| 7 VM + Podman Runtime | Phase 2 |
| 8 标准化 Python Workers | 并入 Phase 1（B1 登记 + 宿主派发器）与 Phase 3（容器化排期） |
| 9 Context Broker | Phase 4 |
| 10 单 Agent 研究闭环 | Phase 7 |
| 11 Workflow Engine | Phase 8 |
| 12 Web Workbench 围绕真实能力逐页交付 | Phase 5 基础版（车道 B 先行）+ 各页随依赖就绪逐页交付 |
| 13 Plugin、MCP、REST、CLI | Phase 6 + Phase 9 |
| 14 生产化 | Phase 10 |

§23 末句「每一步都应有单独的实施计划、变更范围和验收报告」（参考计划 2294 行）冻结为纪律：每个 Phase 开工前按参考计划 §24 模板（目标/范围/非目标/现状/设计/实施批次/风险/验收/回滚/交付物）出阶段说明，本路线只冻结阶段间依赖与批次切分。

### 1.3 并行标注规则

批次标注【并行】表示该批次与其他标注批次无共享写面、接口已冻结，可由不同会话/人员同时推进；标注【串行】表示依赖前一批次的产物。全部批次遵守计划 §15 十步流程（857 到 868 行）与工作区红线（不在宿主机安装依赖、不写 `platform*.db`、不改生产 RTIS 行为、不推翻 M0 到 M8）。

### 1.4 迁移编号冻结规则（本路线新裁决）

已实测：`migrations/` 现有 001 到 004 四个 SQL（本会话 `ls migrations/*.sql` 确认）。TOOL-CONTRACT-DRAFT §10.1 提议 `migrations/005_tool_contract.sql`，CONTEXT-BROKER-DESIGN §6.1 草案写了 `005_context_broker.sql`，两份文档的 005 编号冲突。冻结规则：**迁移编号按合入顺序递增，先到先得；Phase 1 合入 005_tool_contract.sql，Phase 4 落地时改用当时下一个可用编号（预计 006_context_broker.sql），设计文档中的编号让位于本规则。**新迁移一律走开发库副本与测试内存库，`platform.db` 与 `platform-dryrun.db` 都不写（红线；盘点 §6 处置结论）。

---

## 2. Phase 1：Tool Contract 与 Registry

### 2.1 目标

工具先有统一身份、Schema、版本与运行记录（计划 §18 第 3 条，921 行）：定义 Tool Manifest 与 JSON Schema，定义 ToolDefinition、ToolPack、ToolRun、Artifact 基础模型，建立工具发现、查询、Schema 获取与健康状态接口，将现有 Python Workers 与脚本登记为首批候选工具，实现 Mock Runner 验证调用协议（计划 §14:753-759）。设计输入已齐：TOOL-CONTRACT-DRAFT（993 行，含 Manifest v1 字段规范、ToolRun 状态机 13 条跃迁、ToolCallRequest、ToolRunResult 信封、附录 A JSON Schema 草案）。

### 2.2 前置依赖

| 前置 | 状态 | 依据 |
|---|---|---|
| Worker Protocol v1 协议与实现 | 已就绪 | `schemas/worker-protocol/v1.json` 与 `worker_runtime.py`（TOOL-CONTRACT §1.2 第 1 条） |
| tool / tool_version / tool_execution 三表数据面 | 已存在 | `migrations/001_init.sql:278-315`、`003_tools_workflow.sql:79-91`（TOOL-CONTRACT §1.2 第 2 条） |
| 首批登记候选资产 | 已盘点 | TOOL-INVENTORY T01 到 T12 总表（193 到 204 行） |
| Rust workspace 纪律 | 已就绪 | 六 crate 依赖方向与头部约束（盘点 §2，41 行） |
| jsonschema 库级校验器 | 缺件 | 宿主与仓库 `.venv` 均无 jsonschema（TOOL-CONTRACT §11.3），用临时 venv（项目目录内，不属于宿主机安装）解决 |

### 2.3 实施批次草案

| 批次 | 内容 | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P1-1 | `schemas/tool-manifest/v1.json`（以附录 A 为起点）+ `schemas/tool-contract/request-v1.json`、`result-v1.json` | jsonschema 库级 check_schema 通过；rip.doc.parse 与 rip.http.inspect 两个示例分别校验通过；构造一个缺 provenance 的反例被拒绝 | 【并行】与 P1-2、车道 B 同步开工 |
| P1-2 | domain 扩展：ToolRunStatus 枚举、ArtifactId/RunId 新类型（2.0 新实体唯一合法落点，盘点 48 行） | `cargo test --offline -p rip-domain` 纯类型测试通过；不新增 crate 依赖 | 【并行】与 P1-1 同步 |
| P1-3 | `migrations/005_tool_contract.sql`：tool_run + artifact 两表，幂等 DDL + schema_version 登记 | 在开发库副本经 `crates/database` 迁移执行器连续执行两次无差异（复用 migrate.rs 幂等机制，盘点 53 行） | 【串行】依赖 P1-2 类型定稿 |
| P1-4 | `crates/tool-registry`：Manifest 校验九条（TOOL-CONTRACT §2.5）、tool/tool_version 读写面、发现/查询/Schema 获取接口 | 用两示例 Manifest 走完登记、查询、取 Schema 全流程（测试库）；校验九条逐条有拒绝用例 | 【并行】与 P1-5（接口先按 P1-1 冻结） |
| P1-5 | `crates/tool-engine` 骨架 + Mock Runner：SUCCEEDED/FAILED/TIMED_OUT/CANCELLED 四路径、WAITING_FOR_RUNNER 注入、截断信封一致性、幂等命中、状态机 13 条跃迁全路径 | 契约测试全覆盖以上七项（TOOL-CONTRACT §10.2 测试最小集） | 【并行】与 P1-4 |
| P1-6 | worker 宿主派发器（tool-engine 内 python_worker Runtime Adapter）：进程池 + initialize/get_capabilities 握手 + execute/cancel 转发 + stderr 日志采集 | ToolCallRequest 端到端打到真实 parsing worker 并返回 ToolRunResult 信封（修补盘点差距第 4 条「Worker 宿主派发器缺失」，219 行） | 【串行】依赖 P1-5 状态机 |
| P1-7a | 首批登记 B1：rip.doc.parse（T02）、rip.text.embed（T03）、rip.kb.evaluate（T04），python_worker Manifest 直登 | Registry 可发现、可查 Schema；T03 的模型许可证字段补齐（TOOL-INVENTORY 待验证项 6） | 【串行】依赖 P1-4 与 P1-6 |
| P1-7b | 首批登记 B2：rip.kb.keyword_search（T05）、rip.kb.inventory（T06）、rip.ops.etl_status（T07），薄包装 + 结构化输出 Adapter | 三工具登记后各跑一次只读调用成功 | 【并行】与 P1-7a（均只依赖 P1-4/P1-6） |
| P1-7c | 首批登记 B3 + B4：T01 登记为 runtime 资产条目；T08 rtk 五子命令拆 Manifest 登记 | rtk pytest 套件在临时 venv 补跑通过（TOOL-INVENTORY 待验证项 1） | 【并行】与 P1-7a/7b |
| P1-7d | 首批登记 B5：rip.kb.rtis（T09）、rip.kb.eri（T10）Adapter Manifest 指向现有入口，不改本体（红线，TOOL-INVENTORY 161 行） | rt.py 与 rtis_kb.mjs 只读调用经派发器打通；rtis.py 编排层不登记 | 【并行】与 P1-7a 到 7c |
| 缓登 | T11（与 rtk.mcp 合并后登记）、T12（TPscan 许可证核实后 Adapter，且按 TOOL-RUNTIME-ADR §3.1 提前触发条件 1 必须落 VM 不落 podman_host） | 按 TOOL-INVENTORY 复用建议执行 | 依赖对应待验证项关闭 |

Phase 1 明确不做（对齐计划 §15:872）：不建 Tool VM、不写真实 podman Runner、不做 Tool Center 页面、不开放 remote_runner 登记（TOOL-CONTRACT §10.1 禁止项）。

### 2.4 验收命令（实际命令形式）

验收句：工具可发现、查看 Schema、验证参数；不执行任意宿主机 Shell；Registry 有版本和来源信息（计划 §14:761）。对照 TOOL-CONTRACT §10.3。

~~~bash
cd /home/emo/code/pensting/research-platform

# 1. Schema 交付物在位且库级合法（临时 venv，项目目录内）
python3 -m venv .venv-check && .venv-check/bin/pip install jsonschema
.venv-check/bin/python -c "import json,jsonschema; s=json.load(open('schemas/tool-manifest/v1.json')); jsonschema.Draft202012Validator.check_schema(s); print('SCHEMA_VALID')"

# 2. 全仓质量门（与 CI 同命令，ci.yml 实测原文）
cargo test --offline --workspace
cargo clippy --offline --workspace --all-targets -- -D warnings

# 3. worker 族回归（TOOL-INVENTORY §8 同命令）
cd workers/python && ./.venv/bin/python tests/run_all.py   # 期望 SUMMARY ... ok=True

# 4. Registry 只读对账（开发库副本，不触 platform*.db）
cd /home/emo/code/pensting/research-platform
python3 -I -c "import sqlite3; c=sqlite3.connect('file:<开发库副本路径>?mode=ro',uri=True); print(c.execute('select count(*) from tool').fetchone(), c.execute('select count(*) from tool_version').fetchone())"
# 期望：tool 行数 ≥ 首批登记数（B1 到 B5 合计 9 到 13 个工具条目，取决于 B4 rtk 拆分口径，另有 T01 runtime 资产条目），tool_version 与之对应且 provenance_json 非空
~~~

### 2.5 主要风险

1. T1/T2 分界线（写平台内部库是否升 T2）未裁决，rtis.py 现网 T1（TOOL-CONTRACT §2.4 转引 M6_REPORT.md:38 口径），登记 B5 前必须定稿（TOOL-CONTRACT §11.2 待验证）。
2. tool_run 新表与 tool_execution 旧表的合并方式未决（TOOL-CONTRACT §3.3：Phase 1 出迁移对账清单后再定），存在双表并行的口径漂移风险。
3. Registry 落库目标与红线冲突面：tool/tool_version 现网数据在 platform.db（20 行，TOOL-CONTRACT §1.2 实测），登记写入必须落在开发库副本，出对账清单后另行评审切流，禁止直接写生产库。
4. 临时 venv 校验路径（jsonschema、pytest）要防止变成宿主机全局安装；venv 一律建在项目目录内并 gitignore。
5. B1 批次被 T03 模型许可证字段阻塞（待验证项），排期时把 B1 拆为先登 T02/T04、T03 许可证补齐后补登。

---

## 3. Phase 2：Tool VM 与 Podman Runtime

### 3.1 目标

把「AI 不允许随意污染宿主机」落实为架构约束（计划 §18 第 2 条，920 行）：准备专用 Linux VM，配置 VM 内 rootless Podman，建立 Tool Gateway 与心跳机制，建立基础镜像和镜像缓存，实现容器创建、输入挂载、输出收集、超时和取消，完成资源限制和故障恢复测试（计划 §14:763-770）。拓扑决策已冻结：TOOL-RUNTIME-ADR 两步走，目标拓扑 VM 内 rootless Podman 分容器不推翻，起步执行拓扑为宿主机 rootless Podman 分容器（新增过渡 Runtime 类型 podman_host）。

### 3.2 显式前置条件（运行时实测结论，无 sudo 对拓扑的约束）

以下前置条件全部为硬约束，任一不满足则对应批次不得开工。实测记录摘自 TOOL-RUNTIME-ADR §1.3 与 §11（其撰写会话实跑），本会话对五项关键结论做了只读复核（「复核」列标注：本会话实跑，命令与输出见 §3.5）：

| 编号 | 前置条件 | 实测依据 | 对实施的影响 | 本会话复核 |
|---|---|---|---|---|
| PC2-1 | 全链路零 sudo、零宿主机安装 | `timeout 5 sudo -n true` 输出「sudo: 需要密码」退出码 1（ADR §1.2） | 拓扑任何环节不得依赖 root：存储池、VM、VM 内 Podman、Gateway 全部走用户态；需要 root 的方案（系统级 libvirt、宿主直装）一律否决 | 已复核，退出码 1 |
| PC2-2 | VM 栈只能基于 qemu/libvirt 会话级 | qemu-system-x86_64/qemu-img/libvirtd/virsh/virt-install 在位，quickemu 不存在；`virsh uri` 返回 `qemu:///session` 退出码 0（ADR §1.3） | 不依赖 quickemu；VM 用 `qemu:///session` 会话级连接，完全绕开无 sudo 限制；系统级 `qemu:///system` 不可用 | 已复核，`qemu:///session`，libvirtd active |
| PC2-3 | 会话级存储池必须自建在家目录 | 现有唯一池 tmp 指向 `/tmp`，tmpfs 16G（ADR §1.3），不适合放 VM 镜像 | P2-1 首项就是 `virsh pool-define-as` 在家目录自建池；`/tmp` 不落任何镜像（待验证项，ADR §12.3） | 未复核（池操作属写操作，留给 P2-1） |
| PC2-4 | KVM 硬件加速可用 | `/dev/kvm` crw-rw-rw- 读写双向可用（ADR §1.3） | VM 性能可行，无需软件模拟降级 | 已复核，crw-rw-rw- |
| PC2-5 | 磁盘预算充足且有复审阈值 | 根分区可用 259G（ADR §1.3）；镜像与缓存初始预算 50 GiB（ADR §7） | 低于 50 GiB 触发 ADR 复审条件 4（§9），暂停新增镜像登记 | 已复核，259G |
| PC2-6 | 常驻服务可挂 systemd user 单元 | rp-workbench.service active（ADR §1.3）；transient unit 创建成功；user manager degraded 但 3 个失败单元与 RIP 无关 | Gateway 与过渡 Runner 编排挂 user 级单元；degraded 不构成阻断，不需修复无关单元 | 未复核（沿用 ADR 实测） |
| PC2-7 | VM 内 rootless Podman 全流程未验证 | 云镜像、VM 启动、VM 内 subuid/subgid 配置、`--userns=keep-id` 行为均待验证（ADR §12 第 1/2/5 条） | Phase 0 红线禁止启动 VM 已解除；P2-2/P2-3 是全链路首次验证，任何一项失败即触发批次重审，不得跳过 | 未复核（属 Phase 2 执行项） |
| PC2-8 | docker 在位但全程不用 | docker 29.9.3 在位且当前用户可用（ADR §1.3） | 不作为 Runner，任何批次不得引入 docker 依赖 | 未复核（沿用 ADR 实测与策略） |
| PC2-9 | 调用协议与状态机已冻结 | Phase 1 的 ToolCallRequest、状态机（含 WAITING_FOR_RUNNER 语义）、信封（§2 P1-1 到 P1-5） | Gateway 心跳把 RuntimeNode.health 与 WAITING_FOR_RUNNER 判定接通（TOOL-CONTRACT §3.3），Phase 1 Mock 开关换成真实心跳 | 依赖 Phase 1 验收通过 |

### 3.3 实施批次草案

| 批次 | 内容 | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P2-1 | 会话级存储池自建 + 云镜像选型落盘 | `virsh pool-define-as` 在家目录建池成功，卷下载后 sha256 对账通过（消解 ADR 待验证项 3） | 【串行】起点 |
| P2-2 | VM 首次启动（qemu:///session，KVM 加速）+ 基线快照 | `virsh domstate` 返回 running；基线快照可回滚（对应参考计划 §17 场景 4 的前置） | 【串行】依赖 P2-1 |
| P2-3 | VM 内 rootless Podman 配置 + 容器冒烟 | VM 内用户配置 subuid/subgid 后 `podman ps` 退出码 0；`--network none` 冒烟容器跑通（消解 ADR 待验证项 2） | 【串行】依赖 P2-2 |
| P2-4 | `crates/runtime-gateway` 骨架：心跳探测、RuntimeNode.health 落库、WAITING_FOR_RUNNER 判定接通 | 人为停 Runner 后新派发 run 进入 WAITING_FOR_RUNNER 而非 FAILED（计划 §5.6:336-337 原文语义）；恢复后回到 PREPARING | 【并行】与 P2-1 到 P2-3（宿主侧开发不依赖 VM 就绪） |
| P2-5 | podman_host 过渡 Runner（宿主 rootless Podman 分容器）：计划 §5.4 容器约束清单逐条落地（rootless、非 root 容器用户、只读根、独立任务目录、不挂 socket、capabilities 收敛、资源上限、digest 固定）+ `--userns=keep-id` 所有权对齐 | 真实工具（Phase 1 已登记的 python_worker 之外的第一个容器化工具）经 RIP API 派发到宿主容器跑通；产物归属宿主用户（消解 ADR 待验证项 5） | 【并行】与 P2-1 到 P2-3；依赖 P2-4 心跳骨架 |
| P2-6 | podman_vm Runtime Adapter 端到端：容器创建、in/out 挂载、digest 强校验、超时与协作式取消、输出收集登记 Artifact | 真实工具从 RIP API 派发到 VM 内容器运行并登记 Artifact（计划 §14:772 验收第 1 条） | 【串行】依赖 P2-3 与 P2-4 |
| P2-7 | 基础镜像与镜像缓存：按 Tool Pack 拆分镜像族、digest 固定、缓存策略、资源限制基线（ADR §7 表：cpu 2 核、内存 2 GiB、pids 256、时长 600000 ms 上限、输出 64 MiB、tmpfs 1 GiB） | 同一镜像第二次派发不再拉取；超 budget 的 run 被强制取消且产出按 partial Artifact 登记 | 【串行】依赖 P2-6 |
| P2-8 | 故障恢复演练：VM 离线/重连、任务状态不乱、幂等重放、镜像回滚、磁盘自愈阈值 | 完成五项验收（§3.4）与参考计划 §17 场景 4（VM 故障恢复）等价演练；演练后出 ADR §3.1 充分条件达成评审结论 | 【串行】依赖 P2-7 |

P2-5 与 P2-6 完成后按 ADR §3 第 2 条记录 Runner 全景状态：podman_host 过渡、podman_vm 目标，二者共存到 P2-8 切换评审。

### 3.4 验收命令（实际命令形式）

验收五项（计划 §14:772）：真实工具可从 RIP API 派发到 VM 运行；宿主机没有新增工具依赖；容器不能访问宿主机任意目录；输出可登记 Artifact；VM 离线不会导致任务状态错乱。

~~~bash
# 前提复核（只读，随时可跑）
timeout 5 sudo -n true; echo "exit=$?"                      # 期望 1（无免密 sudo，全链路不得依赖 root）
podman info --format '{{.Host.Security.Rootless}} {{.Store.GraphRoot}}'   # 期望 true 与家目录存储
virsh uri                                                    # 期望 qemu:///session
df -BG --output=avail / | tail -1                            # 期望 ≥ 50

# P2-1/P2-2/P2-3 检查
virsh -c qemu:///session pool-list --all                     # 期望出现自建家目录池，active
virsh -c qemu:///session domstate rip-tool-vm                # 期望 running
virsh -c qemu:///session snapshot-list rip-tool-vm           # 期望基线快照存在
# VM 内（经控制台/SSH 通道执行）：
#   podman info --format '{{.Host.Security.Rootless}}'        # 期望 true
#   podman run --rm --network none <基础镜像@sha256:...> true  # 退出码 0

# P2-6 Artifact 登记（只读对账开发库副本）
python3 -I -c "import sqlite3; c=sqlite3.connect('file:<开发库副本>?mode=ro',uri=True); print(c.execute('select run_id,status,tool_id from tool_run order by rowid desc limit 3').fetchall())"
sha256sum <artifact_uri 指向的文件>                           # 与 artifact.hash 列对账一致

# P2-8 容器越界负向检查（验收第 3 条）
# 派发一个尝试读宿主任意路径的工具 run，期望被拒绝或读取不到（只挂任务目录生效）；
# VM 离线演练：virsh -c qemu:///session destroy rip-tool-vm 后派发，期望 run 停在 WAITING_FOR_RUNNER；
# 重连（virsh start）后期望按幂等键续跑，不出现重复副作用、不误报 FAILED。
~~~

宿主机没有新增工具依赖（验收第 2 条）的检查口径：Phase 2 全程对照执行 `pacman -Q` 输出无变化且无全局 pip/npm 安装（镜像拉取属运行时数据不属安装，ADR §3.2 口径）。

### 3.5 主要风险

1. VM 端到端从未跑通是 Phase 2 最大不确定项（ADR 负面后果第 3 条：不可因「工具链在位」低估工作量）。缓解：P2-1 到 P2-3 每批独立验证，首批失败不阻塞 P2-4/P2-5 车道。
2. 双拓扑并存期实现与测试矩阵翻倍（ADR 负面后果第 2 条）：Gateway 必须同时支持两种 Runner 通道到 P2-8 切换评审为止。
3. podman_host 过渡期污染风险由 ADR §3.1 提前触发条件 3 兜底：发生越界污染立即暂停新增 podman_host 登记并加速迁移。
4. 不受信第三方工具（如 TPscan）误落 podman_host：ADR §3.1 条件 1 已冻结为强制 VM，登记批次必须核对。
5. CI 上的 podman 可用性未验证（盘点 §9.1 待验证）：容器化 Runner 的端到端测试若进 CI，需先评估 GitHub runner 安装 podman 的路径，否则容器端到端测试保持本地执行。
6. 磁盘增长：容器存储与 VM 镜像共用 259G 预算，超 50 GiB 镜像预算触发 ADR 复审；P2-7 落地磁盘自愈清理前，人工关注 `df` 阈值。

---

## 4. Phase 3：首批 Tool Packs

### 4.1 目标

按计划 §14:774-784 首批五 Pack：Core Utilities（文件元数据、哈希、压缩、格式转换、JSON/YAML/CSV）、Web Research（HTTP、TLS、页面抓取、静态资源提取）、Source Analysis（Git、代码搜索、依赖清单、静态检查）、Document（PDF、Office、Markdown、OCR/文本抽取）、Mobile Analysis（APK 元数据、Manifest、权限和基础静态分析）。不追求工具数量，优先保证安装、运行、结果 Schema、错误处理和证据归档可靠（计划 §14:784）。同时补齐 TOOL-INVENTORY §7 的八族空白（Web/HTTP、DNS/Domain、Network、Web Analysis、Source Code、Mobile/APK、Binary/Reverse、Browser Automation）。

### 4.2 前置依赖

- Phase 2 的容器通道：P2-5（podman_host）就绪后自研可信工具批次可开工；P2-6（podman_vm）就绪后不受信工具与浏览器类批次开工（ADR §3.2 过渡期纪律：podman_host 只跑来源可信工具）。
- Phase 1 的 Registry 与契约测试设施（新工具登记走 Manifest + 校验九条 + 契约测试）。
- 计划 §4.3 接入标准：来源仓库/版本/许可证/维护状态/依赖/镜像摘要/输入输出 Schema/资源要求/已知限制/测试结果十项记录齐全（计划 170 行），每个工具接入至少做源码检查、最小功能验证、输出格式验证（计划 172 行）。

### 4.3 实施批次草案

| 批次 | 内容 | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P3-1 | Pack 基础镜像族构建流水线：每 Pack 一份 Containerfile，构建全部在容器内完成（宿主零安装），产物 digest 固定并登记 | 每个基础镜像构建成功且 `podman inspect` digest 与登记一致；构建过程宿主机无新安装 | 【串行】起点 |
| P3-2 | Web Research Pack 首工具：rip.http.inspect 真实镜像落地（替换 TOOL-CONTRACT §9 占位 digest，消解其待验证项 1），完成 §4.3 三项验证 | 从 RIP API 派发对 example.com 的 GET，返回结构化结果 + 响应体 Artifact（TOOL-CONTRACT §9.3 四步全走通） | 【并行】依赖 P3-1 与 P2-5 |
| P3-3 | Core Utilities Pack：文件元数据、哈希、压缩、格式转换、JSON/YAML/CSV 五个最小工具 | 五工具各一个 run 端到端 + Artifact 登记 + 契约测试 | 【并行】依赖 P3-1 与 P2-5 |
| P3-4 | Document Pack 补强：PDF/Office/OCR 进容器（现 parsing_worker 仅 md/txt/html 纯文本，TOOL-INVENTORY §1.2） | PDF 解析工具一个 run 端到端；第三方解析库全部在容器内 | 【并行】依赖 P3-1（自研可走 P2-5，引入第三方库建议等 P2-6 后走 VM，按信任级裁定） |
| P3-5 | Source Analysis Pack：Git 元数据、代码搜索、依赖清单、静态检查 | 对本仓库（research-platform）跑一次只读代码搜索 run 成功 | 【并行】依赖 P3-1 与 P2-5 |
| P3-6 | Mobile Analysis Pack：APK 元数据、Manifest、权限静态分析 | 一个测试 APK 的元数据 run 端到端 | 【并行】依赖 P3-1（Java 生态依赖重，建议 P2-6 后走 VM 通道） |
| P3-7 | T12 TPscan 许可证裁决与登记（若准入）：必须落 VM 内容器（ADR §3.1 提前触发条件 1） | 许可证核实结论落盘；准入则按 §4.3 三项验证后登记，拒绝则记录淘汰 | 【串行】依赖 P2-6 与许可证核实 |
| P3-8 | Browser Automation 族补齐评估：真实浏览器环境的执行配置超出宿主 rootless 可表达范围（ADR §3.1 提前触发条件 2 的输入之一是 TELEGRAM-ADMIN-UI-REUSE §2.5 视觉回归浏览器需求），落 VM | 评估结论落盘：是否 Phase 3 内实现或延后；实现则浏览器容器走 VM | 【串行】依赖 P2-6 |

### 4.4 验收命令（实际命令形式）

验收口径（计划 §14:784 原文）：每个 Pack 至少一个工具完成「安装、运行、结果 Schema、错误处理、证据归档」五点可靠链路。

~~~bash
# 每个登记工具的端到端检查（以 rip.http.inspect 为例）：
# 1) 经 Phase 1 派发接口发起 run（CLI 形态随 P1-5 定稿，形如）
cargo run --offline -p tool-engine --bin <派发入口> -- run rip.http.inspect --input '{"url":"https://example.com/"}'
# 2) Run 终态与 Artifact 只读对账（开发库副本）
python3 -I -c "import sqlite3; c=sqlite3.connect('file:<开发库副本>?mode=ro',uri=True); print(c.execute(\"select status,tool_id from tool_run order by rowid desc limit 5\").fetchall())"
sha256sum <artifact 落盘文件>        # 与 artifact.hash 一致
# 3) 镜像 digest 固定复核
podman inspect <镜像>@sha256:<digest> --format '{{.Digest}}'
# 4) 契约测试（与 CI 同门）
cargo test --offline --workspace && cargo clippy --offline --workspace --all-targets -- -D warnings
~~~

### 4.5 主要风险

1. 第三方开源工具的许可证与供应链核查工作量（计划 §4.3：不只凭 Star 数决定纳入）；TPscan 无 LICENSE 文件（TOOL-INVENTORY §4.3 实证）是现成反例。
2. 异构依赖（Python/Node/Java/Go 各版本）进入镜像导致镜像体积与构建时长膨胀，挤压 50 GiB 镜像预算（ADR §7）；缓解：每 Pack 独立镜像、构建全部容器内、登记 image_digest。
3. 八族空白一次性补齐的诱惑与「不提前实现」纪律冲突：以五 Pack 最小集为边界，Binary/Reverse、DNS/Domain 等族延后，不设数量 KPI。
4. 低并发纪律（单线程或低并发、间隔 ≥1s）与验收效率的平衡：并发基线每工具族默认 1（ADR §7），验收不得为提速改高并发。
5. http_api runtime 的模板语法未定（TOOL-CONTRACT §11.5），若 Phase 3 出现首个 http_api 工具则随该批次定稿并补契约测试。

---

## 5. Phase 4：Context Broker

### 5.1 目标

按需加载而非全量塞入：识别任务意图、检索候选知识/插件/工具、排序、计算 Token 预算、去重压缩、保留来源引用，防止不相关插件进入上下文（计划 §8:444）；落地四级上下文（L0 到 L3）、ContextManifest 三表与引用追踪、可解释匹配（CONTEXT-BROKER-DESIGN 全文为实施蓝本，其 §11 M4.0 到 M4.5 即本阶段执行序）。验收三条：不默认把整个工具目录或知识库正文塞入上下文；可解释选择了哪些知识和工具、为什么；工具输出通过 Artifact 引用进入上下文（计划 §14:794）。

### 5.2 前置依赖

- Phase 1：Tool Registry 的目录与 Schema 接口（CONTEXT-BROKER-DESIGN §11 依赖前提原文）；Registry 未就绪的子步骤用 Mock 源先行（先例：计划 §14:759 Mock Runner）。
- Phase 2/3：Run 与 Artifact 记录作为引用目标（同上）；P4-4 起需要真实 Run 数据。
- 复用面已实测：search-engine 检索、resource-engine tokenizer、quality-engine 治理过滤、audit-engine correlation_id、crates/database 迁移执行器、worker 缓存原子写先例（CONTEXT-BROKER-DESIGN §12 复用对照表）。
- RTIS 只读 Adapter 纪律：只经 `rtis_kb.mjs`/`rt.py` 只读接入，不改本体（TOOL-INVENTORY 161 行）。

### 5.3 实施批次草案

| 批次 | 内容 | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P4-1 | 迁移草案（编号按 §1.4 规则取下一个可用号）+ domain Context 类型 + policy 加载 + token 估算器 + 纯函数单测 | 迁移在开发库副本幂等执行两次无差异；estimator 有单测基线（= CONTEXT-BROKER-DESIGN M4.0 判据） | 【并行】车道 A 起点，Phase 1 后即可开工 |
| P4-2 | L0 规则包源 + L1 目录源（Mock Registry 起步）+ card/brief 三级工具说明生成器 | 12 个候选工具（TOOL-INVENTORY 总表 T01 起）可出目录卡片与 brief（= M4.1 判据） | 【并行】依赖 P4-1 |
| P4-3 | intent 纯函数 + knowledge/rtis 只读源接入 + 打分与 matched_signals | 给定「kingdee HTTP 检查」类任务，rtis 源返回只读命中（= M4.2 判据，命令口径同 TOOL-INVENTORY §8 的 rt.py 冒烟） | 【并行】依赖 P4-1 |
| P4-4 | assemble 全流程 + 三表落库 + expand 记账 + AssembledContext 契约校验 | 同一任务两次装配产生两个 manifest，selection_hash 一致可对账（= M4.3 判据） | 【串行】依赖 P4-2/P4-3 与 Phase 2/3 的 Run/Artifact 数据 |
| P4-5 | 内容寻址缓存 + 七类版本失效触发器 + rp-context CLI（assemble/expand/manifest-show/cache-stats） | 改一个工具版本号后 cache-stats 显示对应条目重建（= M4.4 判据） | 【串行】依赖 P4-4 |
| P4-6 | 验收三测试 + 实施差异文档 | 塞入/可解释/Artifact 引用三测试进 CI（= M4.5 判据，映射见 CONTEXT-BROKER-DESIGN §11 末） | 【串行】依赖 P4-5 |

### 5.4 验收命令（实际命令形式）

~~~bash
cd /home/emo/code/pensting/research-platform

# 1. Broker 质量门（含验收三测试）
cargo test --offline --workspace && cargo clippy --offline --workspace --all-targets -- -D warnings

# 2. 装配端到端（CLI 形态 = CONTEXT-BROKER-DESIGN §10 rp-context 四子命令）
cargo run --offline -p context-broker --bin rp-context -- assemble --task-id t-acceptance
cargo run --offline -p context-broker --bin rp-context -- manifest-show <manifest_id>
cargo run --offline -p context-broker --bin rp-context -- cache-stats

# 3. 验收第 1 条「不默认塞入」对账：装配后 Manifest item 总 tokens ≤ 预算（只读查开发库副本）
python3 -I -c "import sqlite3; c=sqlite3.connect('file:<开发库副本>?mode=ro',uri=True); print(c.execute('select layer,kind,sum(est_tokens) from context_manifest_item group by layer,kind').fetchall())"
# 期望：无 L3 正文内联行，L1 为目录卡片，used 总量在 budget_report 内

# 4. 验收第 3 条「Artifact 引用」对账：context_ref_log 有 CITED_IN_TOOL_CALL 事件且引用指向 artifact
python3 -I -c "import sqlite3; c=sqlite3.connect('file:<开发库副本>?mode=ro',uri=True); print(c.execute(\"select event,count(*) from context_ref_log group by event\").fetchall())"
~~~

### 5.5 主要风险

1. Token 估算常数未对真实 tokenizer 校准（CONTEXT-BROKER-DESIGN §13.1）：预算分项在校准前只有相对意义，P4-6 前用 `models/bge-small-zh-v1.5/tokenizer.json` 离线抽样校准（其 §5.2 预留路径）。
2. RTIS knowledge.db 条数口径差（README 14717 与 rtis.py 头注 14664，盘点 129 行）影响「不默认塞入」测试的规模口径，P4-3 时实查一次。
3. `past_success` 权重通路无真实数据（tool_execution 0 行，盘点 218 行），Phase 4 恒为 0，排序效果评估在首批 Run 积累后回看，防止误判为排序缺陷。
4. 与 Phase 1 Registry 接口签名的对接是设计文档显式待验证项（CONTEXT-BROKER-DESIGN §13.2），P1-4 定稿时同步评审 `ContextSource` trait。
5. 范围蔓延风险：向量检索轴 Phase 4 明确不实现（§1.2 边界），预留接口不落地，防止提前实现（计划 §15:872）。

---

## 6. Phase 5：Web Workbench 基础版

### 6.1 目标

建立 React/TypeScript 前端，复用评估 Telegram Bot Admin 的通用布局和组件思路，实现 Overview、Projects、Knowledge、Tool Center、Runs、Runtime 六页，实现统一 API Client、路由懒加载、主题和全局搜索，加入桌面与移动端适配（计划 §14:796-802）。复用清单已盘点：TELEGRAM-ADMIN-UI-REUSE 三级复用矩阵与 §5 抽取路线（脚手架、基础件包、壳层、门禁、逐页）。

### 6.2 前置依赖

- Phase 0 的 TELEGRAM-ADMIN-UI-REUSE（复用清单与工作量级）。
- Application API 方向冻结：不复刻 apps/server handler 直连 SQL 的架构债（盘点 §4 处置结论：handler 逐个切换到 engine 接口，而不是继续加 SQL）；新页面的数据一律走新 API 面而不是直连库。
- 数据依赖按页不同：Overview 可用现有只读 API（rp_server.py 四端点，盘点 §4 实测）；Tool Center 依赖 Phase 1 Registry；Runs/Runtime 依赖 Phase 2 Gateway 心跳与 tool_run；Projects 依赖 research_task 数据面（002_tasks.sql）。
- 前端依赖口径：npm/pnpm 依赖只进项目内 node_modules（同 TOOL-INVENTORY 对仓库内 `.venv` 的合规口径），不全局安装；Playwright 浏览器二进制落用户缓存目录，属运行时数据（同 ADR §3.2 镜像拉取口径）。

### 6.3 实施批次草案

| 批次 | 内容 | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P5-1 | 脚手架：Vite + React 19 + TypeScript + Tailwind 4（daisyUI 5 先评估）+ 全路由 lazy + vendor 分组 + 首帧主题内联（TELEGRAM §5 第 1 条） | `pnpm typecheck` 与 `pnpm build` 通过；首屏只带壳 + 当前路由 | 【并行】车道 B 起点，不依赖 Phase 2 到 4 |
| P5-2 | 基础件包：focus-trap、Dialogs、ui primitives、ErrorBoundary、泛化 useApi（fetcher 参数注入）、EChart 封装（TELEGRAM §5 第 2 条，解耦切点见其 §3 各条） | 基础件单测通过；ApiError 与非 JSON 响应防御带入 | 【并行】依赖 P5-1 |
| P5-3 | 壳层：Layout 模式重写（图标轨 + tab 条 + 抽屉 + 沉浸态 + 按路由 re-key ErrorBoundary）+ RIP 版 workspaces 数据模型 + 全局搜索面板换源 | 壳层在桌面与移动视口渲染正常；⌘K 面板接 RIP 假数据 | 【并行】依赖 P5-2 |
| P5-4 | Application API 骨架：api-server crate（计划 §13:723 crate 清单），先覆盖 Phase 5 页面所需只读接口；apps/server 既有 4 端点切到 engine 接口或桥接 | Overview 页数据走新 API 面；rp_server.py 直连 SQL 不再扩大（盘点 §4 架构债收敛第一步） | 【并行】可与 P5-1 到 P5-3 同步（接口契约先行） |
| P5-5 | Overview 页 + Knowledge 页 | Overview KPI 卡可点击进列表；Knowledge 检索接 search-engine API | 【串行】依赖 P5-3 与 P5-4 |
| P5-6 | Tool Center 页：分类树 + 工具列表 + 详情（Schema、来源、许可证、资源限制、运行历史、试运行入口，计划 §10.4 D） | 对 Phase 1 登记的 11 个工具出目录与详情；试运行生成独立 Run | 【串行】依赖 P5-3、P5-4 与 Phase 1 Registry |
| P5-7 | Runs 页 + Runtime 页：Run 列表/详情/Artifact 预览；VM 在线状态、心跳、Podman 版本、镜像 digest、资源（计划 §10.4 H） | 一个真实 Run 从列表点开可见状态轨迹与 Artifact；Runtime 页心跳数据来自 Phase 2 Gateway | 【串行】依赖 Phase 2 P2-4 起的数据面 |
| P5-8 | Projects 页最小版：项目/任务树与研究时间线（research_task 数据面） | 项目页可浏览 42 个存量任务（sources-snapshot 口径，盘点 §6） | 【串行】依赖 P5-3 与 P5-4 |
| P5-9 | 视觉回归与 a11y 门禁：playwright 静态服务器 + 页面矩阵（每页 2 主题 × 2 视口）+ axe AA 并轨 + CI gate 接线（TELEGRAM §2.5 方法整套照搬，stub 与 mock 表重写） | 首批页面矩阵进 CI 门禁；console.error 与 pageerror 零容忍断言生效 | 【并行】可与 P5-5 到 P5-8 同步搭骨架，页面完成后逐页入矩阵 |

### 6.4 验收命令（实际命令形式）

验收句：Web 能管理工具、查看运行、浏览知识和任务；工具结果可预览；关键路由有视觉和交互测试（计划 §14:804）。

~~~bash
cd <rip 前端应用目录>

pnpm typecheck && pnpm build          # 构建门
pnpm test:visual:gate                  # 视觉 + a11y 门禁（形式取自 telegram-bot qa 体系，TELEGRAM §2.5）
# 矩阵期望：已交付页面数 × 2 主题 × 2 视口 全过；maxDiffPixelRatio 阈值沿用 0.002

cd /home/emo/code/pensting/research-platform
cargo test --offline --workspace       # api-server 侧质量门
# 只读核对 Web 消费的 API 不直连生产写路径：
grep -n "mode=ro" apps/server/rp_server.py | head -3   # 既有 4 端点只读口径不变
~~~

### 6.5 主要风险

1. 架构债复发：新页面图省事直连 SQL 会复制 rp_server.py 模式（盘点 §4 明文相悖项），P5-4 的 api-server 骨架必须先于数据页面落地。
2. telegram-bot 侧快照漂移：admin 仍在活跃开发（最近提交 2026-11-08，TELEGRAM §5 实测），复制时点即冻结点，RIP 侧不回同步，复用条目的上游演进不追踪。
3. 视觉回归基线的字体环境：快照基线只能由 CI 字体环境生成（TELEGRAM §2.5 引 telegram-bot 提交 a1796e8 教训），本地生成基线会造成全量假差异。
4. Playwright 浏览器二进制与 CJK 字体的获取路径在无 sudo 约束下的可行性未验证（待验证）：用户级缓存与字体安装（用户字体目录）可试，不可行则视觉矩阵裁剪视口或延后。
5. daisyUI 变量契约层取舍（TELEGRAM §2.3 耦合点）：RIP 若不用 daisyUI 需重写该层，主题工作量按 S 到 M 重估。

---

## 7. Phase 6：Plugin Runtime 与 Plugin Center

### 7.1 目标

插件 Manifest、Registry、生命周期；Knowledge/Tool/Project/Agent/Workflow/Integration 六类插件；本地目录、Git、MCP、HTTP 接入；依赖检查、版本固定、启停和升级（计划 §14:806-811）。设计输入已齐：PLUGIN-CONTRACT-DRAFT（六类能力模型、七态主链加五辅助态生命周期、五类来源与 plugin.lock、三层依赖检查、rip-data/plugins 布局、两个示例 Manifest）。

### 7.2 前置依赖

- Phase 1 Tool Registry（tool / tool_pack capability 的落点）。
- Phase 4 Context Broker（插件目录进 L1、匹配进 L2、禁用即撤出匹配面；PLUGIN-CONTRACT §9 三级按需加载与 §8 四级上下文对齐）。
- Phase 5 壳层（Plugin Center 页面的宿主）。
- audit-engine（install/enable/disable/upgrade/uninstall 全量 Emit，插件事件从第一天只走 audit-engine 不新增 JSONL 分叉，PLUGIN-CONTRACT §11.4）。
- 质量扩展落点：quality-engine 治理对象从 Resource 扩到 Plugin 是既有裁定（盘点 83、203 行），INVALID 处置复用其双轴状态机（PLUGIN-CONTRACT §11.5，建议未实现）。

### 7.3 实施批次草案

| 批次 | 内容 | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P6-1 | `crates/plugin-registry`：Manifest strict 校验（未知字段拒绝）、§4.2 校验顺序七条、DISCOVERED→VALIDATED→INSTALLED 静态链、checksums.sha256 与 plugin.lock | 两示例 Manifest（knowledge.rtis-redteam、tool.rip.python-workers）校验通过；构造缺 license 与浮动分支反例被拒绝 | 【并行】起点 |
| P6-2 | `crates/plugin-runtime`：REGISTERED→ENABLED→LOADED→ACTIVE 主链 + disable/upgrade/uninstall 语义 + §5.4 数据保留表 + 全量审计事件 | 生命周期全路径契约测试（含禁用不删数据、卸载保留已入主库知识、升级失败回滚） | 【并行】与 P6-1（接口先冻结） |
| P6-3 | rip-data/plugins 目录布局 + 位置配置（rip.data_dir、RIP_DATA_DIR 覆盖、源码零绝对路径字面量）+ state/ 文件承载 | 目录树按 PLUGIN-CONTRACT §8 建立且可配置重定位；测试用临时目录 | 【并行】依赖 P6-1 常量 |
| P6-4 | 五类来源接入：local_dir 先行，git（pinned commit）、archive（sha256）、mcp_server、http_api 逐个补 | 每类来源各一个最小插件走完 discover→validate→install | 【并行】依赖 P6-1 到 P6-3 |
| P6-5 | Knowledge 插件落地：knowledge.rtis-redteam（摄取管道 + plugin_id 溯源；不改 RTIS 本体，RTIS_RESOLVER_MODE 三态回滚机制不动） | RTIS 语料经摄取管道入主库且记录 plugin_id；生产 RTIS 行为零变化（红线核查） | 【串行】依赖 P6-4 local_dir 通道 |
| P6-6 | Tool 插件 + 外部 Project 插件：tool.rip.python-workers + rtis_mcp.mjs 以 project_endpoint 接入（mcp_client runtime 映射） | 验收「至少接入一个外部项目和一个知识插件」（计划 §14:813）达成 | 【并行】与 P6-5 |
| P6-7 | 三层依赖与兼容检查：静态层（VALIDATED）、激活层（依赖解析 + 环检测 + compatibility.rip_core + runtime 可用性）、运行层（周期健康检查，不 tight-poll） | 依赖缺失拒绝 enable 并输出机器可读清单；环检测用例拒绝 | 【串行】依赖 P6-2 |
| P6-8 | Plugin Center 页面：已安装/可更新/已禁用/可发现、兼容性列、依赖图、安装/启停/升级/卸载操作（计划 §10.4 E） | 页面对 P6-5/P6-6 两个插件完成全操作回路且不影响核心数据库 | 【串行】依赖 Phase 5 壳层与 P6-7 |

### 7.4 验收命令（实际命令形式）

~~~bash
cd /home/emo/code/pensting/research-platform

cargo test --offline --workspace && cargo clippy --offline --workspace --all-targets -- -D warnings

# 生命周期回路（CLI 形态随 P6-2 定稿，形如）
cargo run --offline -p plugin-runtime --bin rp-plugin -- install knowledge.rtis-redteam
cargo run --offline -p plugin-runtime --bin rp-plugin -- enable knowledge.rtis-redteam
cargo run --offline -p plugin-runtime --bin rp-plugin -- disable knowledge.rtis-redteam
# disable 后：Context Broker 匹配面无该插件（rp-context assemble 不再命中），installed/ 内容仍在
test -d <rip-data>/plugins/installed/knowledge.rtis-redteam/0.1.0 && echo "DATA_RETAINED"

# 插件启停不影响核心数据库（验收句，计划 §14:813）：启停前后只读核对核心库
python3 -I -c "import sqlite3; c=sqlite3.connect('file:<开发库副本>?mode=ro',uri=True); print(c.execute('select count(*) from audit_event').fetchone())"
# 期望：启停只追加插件生命周期审计事件，核心实体表行数不变

# 元数据与完整内容按需加载（验收句）：rp-context L1 目录只有卡片，L3 expand 才有 docs
cargo run --offline -p context-broker --bin rp-context -- manifest-show <manifest_id> | grep -c "included.*catalog-card"
~~~

### 7.5 主要风险

1. rip-data 目录当前不存在（PLUGIN-CONTRACT §13.3 ls 实测），首次落地要连带确定其相对部署根的定位与 gitignore 策略。
2. 仓库顶层无 LICENSE 文件（PLUGIN-CONTRACT §12.2 实测），示例 Manifest 的 license 占位 UNLICENSED 必须在 P6-5 前裁决，否则校验七条第 4 条永远过不了自己的插件。
3. knowledge.db 实际条数口径差（PLUGIN §12.1 待验证）影响 Knowledge 插件摄取规模的验收口径。
4. 双 MCP 并存混淆：rtis_mcp.mjs（kit 工具面，9 个 redteam_* 工具）与后续 Phase 9 的平台 mcp-server 边界必须在 P6-6 写清：前者作为 project_endpoint 被 RIP 接入，不是平台 MCP 面。
5. 插件写面绕过 Application API 原则的风险：capability 登记一律走平台数据面写入通道（走迁移纪律），杜绝插件直写库文件（PLUGIN-CONTRACT §2 原则 2 第一红线）。

---

## 8. Phase 7：Agent Engine

### 8.1 目标

Agent Definition 与能力绑定、Context Broker 集成、Tool Engine 调用、Task Memory、Evidence Memory、Research Memory、暂停恢复取消重试和运行轨迹（计划 §14:815-821）。验收：Agent 能独立查询知识并调用 Tool VM 中的工具；调用可追踪；结果能回到任务上下文并形成 Evidence（计划 §14:823）。执行循环基准：计划 §9 五百行流程（454 到 491 行）与知识工具联动八步（478 到 488 行）。

### 8.2 前置依赖

- Phase 4 Context Broker 公开接口（assemble/expand/manifest，CONTEXT-BROKER-DESIGN §10 设计签名，P1-4/P4-4 落稿后对齐）。
- Phase 2/3 真实工具执行面（Agent 调用工具必须走 Tool Engine 到容器/VM，不得直调宿主 Shell，计划 §6:370 禁令同等生效）。
- Phase 6（软依赖）：agent_definition capability 使 Agent 定义可打包分发；首个 Agent 可先内置不走插件通道。
- audit-engine trace（按 correlation_id 回放任务、意图、上下文、工具、证据链，盘点 §2.3）。

### 8.3 实施批次草案

| 批次 | 内容 | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P7-1 | AgentDefinition / AgentRun 域模型 + 迁移（编号顺延）+ capability_refs 绑定模型（计划 §12:699-700 实体行） | 类型与迁移在开发库副本幂等执行两次无差异；绑定引用校验（指向不存在工具即拒绝） | 【并行】可与 Phase 5/6 同步（纯数据面） |
| P7-2 | `crates/agent-engine` 最小执行闭环：目标 → assemble → 计划 → 单工具调用 → Artifact/Evidence 登记 → 结束报告 | 一个内置测试 Agent 对登记工具完成一次调用且 Evidence 落账（计划 §14:823 验收第 1 条的缩小版） | 【串行】依赖 Phase 4 完成与 P7-1 |
| P7-3 | Context Broker 深度集成：L0 到 L3 全层消费、expand 记账、ambiguous 意图确认回路 | Agent 每轮上下文来自 Broker（禁止 Agent 自查库）；expand 扣账在 context_ref_log 可查 | 【串行】依赖 P7-2 |
| P7-4 | 三类 Memory：Task Memory、Evidence Memory、Research Memory + 原始证据不因压缩丢失约束（计划 §9:474 原文） | 压缩/驱逐后 Evidence 与 Artifact 引用仍可解析（content_hash 校验通过） | 【串行】依赖 P7-3 |
| P7-5 | 运行控制：暂停、恢复、取消、重试、停滞检测、重复调用检测、目标偏移检测、证据不足提示、预算与资源限制（计划 §9:474 能力清单） | 每项控制各一个契约测试用例（取消走协作式语义，与 ToolRun D-TC-04 一致） | 【串行】依赖 P7-4 |
| P7-6 | 运行轨迹与回放：audit trace 按 correlation_id 回放全链 + Agent Studio 页面基础（列表、配置、试运行轨迹，计划 §10.4 F 缩小版） | 一次完整 Agent run 的 trace 回放覆盖任务、意图、上下文、工具、证据五类事件 | 【串行】依赖 P7-5 与 Phase 5 壳层 |

### 8.4 验收命令（实际命令形式）

~~~bash
cd /home/emo/code/pensting/research-platform

cargo test --offline --workspace && cargo clippy --offline --workspace --all-targets -- -D warnings

# 端到端验收（CLI 形态随 P7-2 定稿，形如）：
cargo run --offline -p agent-engine --bin rp-agent -- run <agent_id> --goal "检索 kingdee 相关知识并调用 rip.http.inspect 检查 example.com"
# 验收第 2 条「调用可追踪」：trace 回放（audit-engine Trace 语义，只读查开发库副本）
python3 -I -c "import sqlite3; c=sqlite3.connect('file:<开发库副本>?mode=ro',uri=True); print(c.execute(\"select event_id,kind from audit_event where correlation_id='<run_id>' order by event_id\").fetchall())"
# 期望事件链覆盖：intent → context_manifest → tool_run → artifact → evidence

# 验收第 3 条「结果回任务上下文形成 Evidence」：Evidence 的 provenance 指向 Artifact 且引用 Manifest item
python3 -I -c "import sqlite3; c=sqlite3.connect('file:<开发库副本>?mode=ro',uri=True); print(c.execute(\"select evidence_id,source_artifact from evidence order by rowid desc limit 3\").fetchall())"
~~~

### 8.5 主要风险

1. Agent 循环失控（无限重试、目标漂移、重复调用）：计划 §9:474 的检测项必须与 P7-5 同批落地，不允许先上循环后补护栏；低并发纪律约束 Agent 并行子任务默认关闭。
2. Broker 接口签名传导风险：`ContextSource`/`Broker` 设计签名标注待验证（CONTEXT-BROKER-DESIGN §10），P1-4/P4-4 定稿若变更，P7-3 联调要回改。
3. LLM 调用面：Broker 核心无模型推理（设计 §1.2 明确），Agent 层的规划与决策引入模型调用，测试策略要区分确定性测试与模型在环测试，模型在环用例不进 CI 门禁只做本地验收。
4. Memory 设计缺既有实现参照（盘点差距第 7 条：domain 实体覆盖 2/15），P7-4 的数据面设计要在 P7-1 迁移评审时单独过，防止与 Evidence 实体（计划 §12:704）重复建模。

---

## 9. Phase 8：Workflow Studio

### 9.1 目标

Workflow DSL 与 Schema、可视化节点画布、Tool/Knowledge/Agent/Condition/Parallel 节点、运行暂停恢复失败重试和版本管理（计划 §14:825-831）。验收：多步骤 Workflow 可在 Web 中编辑、运行并查看每个节点结果（计划 §14:832）。Agent 与 Workflow 分离：Workflow 负责确定性编排，Agent 负责动态决策（计划 §9:476）。

### 9.2 前置依赖

- Phase 7 Agent Engine（Agent 节点的执行体）。
- Phase 5 壳层与 P5-9 门禁（画布页面的宿主与视觉回归），XYFlow 依赖（计划 §10.6:617）。
- workflow 数据面已存在但无 Rust 读写面（003_tools_workflow.sql 的 workflow 与 workflow_registry_meta 表，盘点 §12 差距第 2 条）；存量 workflow 行数待验证（PLUGIN-CONTRACT §12.3）。
- Tool Engine（Tool 节点）与 Phase 4（Knowledge 节点经 Broker 检索）。

### 9.3 实施批次草案

| 批次 | 内容 | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P8-1 | WorkflowDefinition / WorkflowRun 域模型 + `crates/workflow-engine` DSL 与 Schema + graph 静态校验（环、孤立节点、必填参数）| DSL 解析与校验纯函数测试全过；存量 workflow 行导入或标记不兼容（行数先实查） | 【并行】可与 Phase 6/7 尾部同步 |
| P8-2 | 非智能节点执行器：Tool / Knowledge / Condition / Parallel | 四类节点各一个最小 workflow 定义跑通并落 WorkflowRun 节点状态 | 【串行】依赖 P8-1 |
| P8-3 | Agent 节点 + 长任务执行（暂停/恢复/失败重试/版本管理，参考计划 §10.4 长任务语义） | 含 Agent 节点的 workflow 暂停后续跑，节点状态续接不重跑已完成节点 | 【串行】依赖 Phase 7 完成 |
| P8-4 | 可视化画布：XYFlow 节点分类面板、参数 Schema 表单、执行日志与节点状态底栏（计划 §10.4 G），只读查看降级（移动端，计划 §10.5） | 画布编辑一个四节点 workflow 并运行成功；移动端只读查看可用 | 【串行】依赖 P8-2 与 Phase 5 壳层 |

### 9.4 验收命令（实际命令形式）

~~~bash
cd /home/emo/code/pensting/research-platform

cargo test --offline --workspace && cargo clippy --offline --workspace --all-targets -- -D warnings

# 后端端到端（CLI 形态随 P8-1 定稿，形如）：
cargo run --offline -p workflow-engine --bin rp-workflow -- run <workflow_id> --input '{"target":"example.com"}'
# 节点状态对账（只读开发库副本）：
python3 -I -c "import sqlite3; c=sqlite3.connect('file:<开发库副本>?mode=ro',uri=True); print(c.execute(\"select node_id,state from workflow_run_node where run_id='<run_id>'\").fetchall())"
# 期望：每个节点有独立终态，失败节点可重试且版本号递增

# Web 侧（画布页交互与视觉）：
cd <rip 前端应用目录> && pnpm test:visual:gate
~~~

### 9.5 主要风险

1. 画布复杂度是前端最大单体：XYFlow 与既有 vendor 分组策略的兼容（react-router 教训，TELEGRAM §2.4：不进双入口共享 vendor 组）要在 P8-4 开工前验证 bundle 影响。
2. 存量 workflow 行数与形态未知（PLUGIN §12.3 待验证），P8-1 先实查再定导入策略，避免为兼容旧形态扭曲 DSL。
3. Workflow 与 Agent 职责边界漂移：把动态决策写进 workflow 节点会违反分离原则（计划 §9:476），评审时以「节点图是否需要运行期才知道走向」为判据。
4. 长任务执行的中断恢复与 ToolRun 状态机的衔接：workflow 暂停时运行中的 Tool Run 取舍（等完成为止还是取消）要在 P8-3 定稿并测试。

---

## 10. Phase 9：MCP / REST / CLI 对外能力

### 10.1 目标

MCP Server 适配 Application API、动态工具发现、Resources、Tools、Prompts；REST API 和 CLI；外部 AI 使用示例与协议测试（计划 §14:834-841）。验收：没有本地工具的外部 AI 仍能通过 MCP 调用 RIP Tool Engine；MCP 与 Web 使用相同业务逻辑（计划 §14:841）。MCP 面清单：计划 §11:640-656 的 17 个 tool 与 7 个 resource；目录分页与按需发现（计划 §11:658）。

### 10.2 前置依赖

- Application API 唯一入口已成型（P5-4 起的 api-server crate），MCP/REST/CLI 全部适配它而不是各自实现业务逻辑（计划 §3 原则第 1 条，117 行）。
- 能力面齐全度决定 MCP 目录完整性：tools（Phase 1 到 3）、plugins（Phase 6）、agents（Phase 7）、workflows（Phase 8）、evidence/artifact（Phase 2 起）。
- rtis_mcp.mjs（stdio JSON-RPC 2.0，9 个 redteam_* 工具面，盘点 §5）是 kit 层起点参考：MODULE_BOUNDARIES 对 rtis 组件的去向裁定含「rtis_mcp.mjs → integrations/mcp」（盘点 127 行），平台 MCP server 以它为形态参考但代码独立。
- 鉴权基准：沿用 rp_server.py 的 Bearer + 常时比较 + 节流四项安全实现（盘点 §4 可整体保留结论）。

### 10.3 实施批次草案

| 批次 | 内容 | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P9-1 | `crates/mcp-server`：MCP 适配 Application API；初始化只暴露目录与分页发现（rip.list_tools 分页 + rip.get_tool_schema 按需），不暴露完整说明（计划 §11:658 原文） | stdio 握手 + tools/list 分页 + get_tool_schema 三交互跑通；初始化响应不含工具全文说明 | 【串行】起点（依赖 api-server 稳定） |
| P9-2 | rip.run_tool 端到端通道：外部 AI 发现 → Schema → run_tool → 派发 Tool Engine → 返回结构化结果 + Artifact + Evidence 引用（计划 §11:676-678 八步） | 以最小 MCP 客户端（可复用 rtk.mcp，TOOL-INVENTORY T08）模拟无工具外部 AI 完成一次 rip.http.inspect 调用 | 【并行】可在 Phase 3 完成后提前实施（只依赖工具派发链，不等 Phase 6/7/8） |
| P9-3 | REST API 完善 + CLI：REST 面覆盖 MCP 同能力集；CLI 收敛既有 rp-resolve/rp-search-index/rp-review 与新入口，rt.py/scripts 保持 kit 层不动 | 同一业务操作（列工具、查 Schema、跑工具、查 Run）REST 与 CLI 各走通且结果一致 | 【串行】依赖 P9-1 |
| P9-4 | 协议一致性测试：MCP 与 Web 同业务逻辑的双通道同断言测试载具（复用 rp-eri-parity 载具模式，盘点 §2.7） | 任选 5 个业务操作，Web API 与 MCP 两通道断言同一输出形状与同一审计链 | 【串行】依赖 P9-1 到 P9-3 |
| P9-5 | Resources 与 Prompts：rip:// 七个 resource（计划 §11:662-668）与 prompt 面 | 外部 AI 可读 rip://runs/<run_id> 与 rip://evidence/<id> 两类引用 | 【串行】依赖 P9-1 |

### 10.4 验收命令（实际命令形式）

~~~bash
cd /home/emo/code/pensting/research-platform

cargo test --offline --workspace && cargo clippy --offline --workspace --all-targets -- -D warnings

# 验收第 1 条：无本地工具的外部 AI 经 MCP 调用 Tool Engine（stdio JSON-RPC 2.0 交换，形态同 rtis_mcp.mjs 通道）
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"cursor":""}}' | cargo run --offline -p mcp-server --bin rp-mcp
# 期望：分页目录（不含完整说明），含 rip.run_tool
echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rip.run_tool","arguments":{"tool_id":"rip.http.inspect","input":{"url":"https://example.com/"}}}}' | cargo run --offline -p mcp-server --bin rp-mcp
# 期望：返回结构化结果 + artifact_id + evidence 引用；Run 在 tool_run 表可见（只读对账）

# 验收第 2 条：MCP 与 Web 同业务逻辑（P9-4 一致性载具）
cargo test --offline -p rp-mcp-parity   # 载具 crate 名随 P9-4 定稿
~~~

### 10.5 主要风险

1. MCP 面与 Web 逻辑漂移是最容易复发的债：一致性测试载具（P9-4）必须是常驻 CI 门禁而不是一次性验收。
2. 双 MCP server 并存（平台 rp-mcp 与 kit 的 rtis_mcp.mjs）造成的目录混淆与误调用：文档与 tools/list 命名空间必须隔离（平台 rip.* 前缀，kit redteam_* 前缀保持）。
3. 对外面扩大攻击面：MCP/REST 鉴权照搬 rp_server 四项安全实现并补 MCP 握手鉴权；禁止匿名 run_tool（T2 工具仍走 authorization_ref 校验）。
4. 提前通道（P9-2）与主干合流的版本协调：P9-2 若在 Phase 3 后提前实施，Phase 6/7/8 能力面并入时目录分页要重新压测。

---

## 11. Phase 10：生态、可靠性与发布

### 11.1 目标

插件注册表和工具包目录、更新策略兼容性检查和回滚、多 Runner/多 VM 扩展、性能稳定性恢复和安全测试、安装备份迁移升级文档（计划 §14:843-849）。验收：独立安装部署；Web、MCP、REST、CLI 协同；知识、插件、工具、Agent、Workflow 生命周期完整；宿主机保持干净（计划 §14:851）。总验收以计划 §17 十四条为准（896 到 911 行）。

### 11.2 前置依赖

Phase 1 到 9 全部验收通过；ADR 复审条件全部处于已知状态（VM 切换完成或 podman_host 退役计划已定，ADR §9.1）；database crate 的 backup()/restore() 缺口（盘点 §2.2 审计补充：MODULE_BOUNDARIES 声明但未实现）在本阶段补齐。

### 11.3 实施批次草案

| 批次 | 内容 | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P10-1 | 插件注册表与工具包目录：official/community/local/remote 信任级落地（PLUGIN-CONTRACT §6 信任级四档）、目录发布形态 | 信任级影响安装确认强度与自动更新默认策略（仅 official 自动提示，D10）生效 | 【并行】起点 |
| P10-2 | 更新策略与回滚演练：plugin.lock 与镜像 digest 双轨的升级、失败回滚、回滚窗口清理 | 一次插件升级失败自动回滚旧版本；一次镜像 digest 回滚演练（ADR §6 镜像回滚语义） | 【串行】依赖 P10-1 |
| P10-3 | 多 Runner/多 VM 扩展：RuntimeNode 多节点调度与故障转移；remote_runner 是否开放按计划 §6:368 维持 P2 评审 | 双 Runner（podman_vm + podman_host 或双 VM）下调度与心跳正确；单节点离线不丢任务 | 【串行】依赖 Phase 2 心跳机制稳定 |
| P10-4 | 性能、稳定性、恢复与安全测试：并发上限与资源基线压测、磁盘自愈、VM 故障矩阵、T0/T1/T2 越界拒绝矩阵、secrets 不泄露核查 | 每类一组的自动化测试进 CI（模型在环与 VM 长演练除外，本地执行） | 【并行】与 P10-1 到 P10-3 同步编写 |
| P10-5 | 备份/恢复与迁移：database crate 补 backup()/restore()；备份范围（库、rip-data、Artifact 存储）、恢复演练、升级迁移文档 | 备份到恢复的 roundtrip 演练通过，数据一致性核对（对账口径沿用 verify.py 纪律，盘点 §6） | 【串行】独立于 P10-1 到 P10-4 |
| P10-6 | 发布文档与独立部署演练：安装、备份、迁移、升级四文档（计划 §17:911）；fresh clone 部署演练 + 全链冒烟 | 参考计划 §17 五个端到端场景（研究一个开源项目、外部 AI 调用工具、知识库治理、VM 故障恢复、工作流恢复）各演练一遍 | 【串行】收口批次 |

### 11.4 验收命令（实际命令形式）

~~~bash
# 独立部署演练（fresh clone，按 P10-6 文档执行后冒烟）：
cd /tmp && git clone <rip 仓库> rip-deploy-check && cd rip-deploy-check
cargo test --offline --workspace                       # 离线构建与测试通过（vendor 基线）
# 冒烟：resolve/search/workbench/一次工具 run 各一

# 宿主机干净度（验收句「宿主机保持干净」）：部署前后 command -v 快照 diff
for c in podman virsh virt-install qemu-img docker quickemu node python3; do command -v $c; done > /tmp/host-tools-after.txt
diff /tmp/host-tools-before.txt /tmp/host-tools-after.txt   # 期望：无新增条目（docker 在位属既有，quickemu 仍缺）

# 备份恢复 roundtrip（P10-5）：
<backup 命令，随 P10-5 定稿> && <restore 命令>
python3 -I scripts/rip_etl_status.py                   # 期望 quick_check=ok 与恢复前计数一致

# 磁盘与预算阈值（ADR 复审条件 4）：
df -BG --output=avail / | tail -1                      # 期望 ≥ 50
virsh -c qemu:///session pool-list --all               # 镜像池占用在 50 GiB 预算内
~~~

### 11.5 主要风险

1. backup()/restore() 是声明未实现的能力（盘点 §2.2），P10-5 是最后实现窗口，拖期直接阻塞 §17 第 14 条验收。
2. 多 VM 的磁盘预算：单 VM 镜像加容器存储已占 50 GiB 预算，第二台 VM 前必须复审（ADR §9.4 阈值联动）。
3. 发布演练依赖人工操作序列，易与文档漂移：P10-6 的演练记录即文档验收依据，演练发现与文档不一致时改文档不改实现（或反过来走变更评审）。
4. 性能测试的基线缺失：当前无性能数据积累，P10-4 以「不回归」为门（对照 Phase 4 装配耗时、Phase 2 派发延迟的既有记录），不设绝对指标。

---

## 12. 总验收对照（计划 §17 逐条映射）

| §17 验收句 | 支撑 Phase |
|---|---|
| 可以作为独立研究平台启动和使用 | Phase 10 P10-6 |
| Web Workbench 能管理项目、知识、工具、插件、Agent、Workflow、Evidence 和 Runtime | Phase 5/6/7/8 各页 + P10-6 |
| 知识库检索、来源追踪、治理质量、关联证据 | Phase 4（引用追踪）+ 既有 quality-engine/search-engine |
| Tool Registry 发现、Schema、版本、健康 | Phase 1 |
| Tool Engine 派发专用 VM，Podman 隔离执行 | Phase 2 |
| AI 不需要在宿主机安装工具或依赖 | Phase 2/3 全程红线 + P10-6 干净度 diff |
| 统一 Run、Artifact、Evidence 和日志记录 | Phase 1（协议）+ Phase 2（登记）+ Phase 7（Evidence 闭环） |
| 插件动态发现、安装、启用、禁用、升级、卸载 | Phase 6 |
| 知识与工具说明按任务动态加载，控制 Token | Phase 4 |
| Agent 结合知识库与工具库完成多步骤任务 | Phase 7 |
| Workflow 可视化编辑和运行 | Phase 8 |
| 外部 AI 无自有工具经 MCP 使用 RIP 工具 | Phase 9 P9-2/P9-4 |
| Rust Core、Python Workers、RTIS 集成边界清晰 | 全程红线（RTIS 只 Adapter 接入，worker 走 python_worker runtime）+ P9-4 一致性 |
| 备份、恢复、升级、兼容性与故障诊断文档 | Phase 10 P10-5/P10-6 |

---

## 13. 跨阶段红线与工程纪律（全程生效）

1. 不在 CachyOS 宿主机安装任何依赖或工具；项目内 venv/node_modules 与容器/VM 内依赖不属宿主机安装（口径沿用 TOOL-INVENTORY §0 与 ADR §3.2）。
2. 不写 `migrations/` 下任何 `platform*.db`；所有新表走开发库副本与测试内存库，生产切流另行评审（盘点 §6 纪律）。
3. 不修改生产 RTIS 行为；接入只走 Adapter 与既有回滚机制（RTIS_RESOLVER_MODE 三态 + backups/，盘点 §5）。
4. 不推翻 M0 到 M8 已交付能力（计划 §15:873）；新实体只进 crates/domain（盘点 48 行）。
5. 不提前实现后续阶段功能（计划 §15:872）；每 Phase 的禁止项随批次列出。
6. 不允许 Agent 与工具容器自由调用包管理器、访问宿主文件系统与容器 Socket（计划 §15:876-877）。
7. 不擅自 reset/clean/checkout/stash/rebase/commit/push；保留用户已有改动（计划 §15:878-879）。
8. 每个 Phase 开工前按参考计划 §24 模板出阶段说明；每批次验收报告含命令原文与输出摘要。
9. 文档纪律：新增文档一律中文，全文禁止 em dash 与 en dash。
10. 低并发纪律：扫描与批量操作默认单线程或低并发（工作区 AGENTS.md 约定）；运行时并发基线按 ADR §7（每工具族默认 1）。

---

## 14. 待验证清单汇总（按解锁阶段排序）

| 编号 | 待验证项 | 阻塞的批次 | 来源 |
|---|---|---|---|
| V-1 | T03 bge 模型权重上游许可证 | P1-7a（B1 批次 T03 条目） | TOOL-INVENTORY §9.6 |
| V-2 | toolkit pytest 套件（临时 venv 补跑） | P1-7c（B4 批次） | TOOL-INVENTORY §9.1 |
| V-3 | T1/T2 分界线（内部写是否升 T2） | P1-7d（B5 批次登记前定稿） | TOOL-CONTRACT §11.2 |
| V-4 | jsonschema 库级校验（临时 venv） | P1-1 | TOOL-CONTRACT §11.3 |
| V-5 | 会话级存储池自建（virsh pool-define-as 家目录） | P2-1 | TOOL-RUNTIME-ADR §12.3 |
| V-6 | VM 端到端全流程（云镜像、首启、VM 内 rootless Podman 跑真实工具） | P2-2/P2-3/P2-6 | TOOL-RUNTIME-ADR §12.1 |
| V-7 | VM 内 subuid/subgid 配置 | P2-3 | TOOL-RUNTIME-ADR §12.2 |
| V-8 | `--userns=keep-id` 与具体镜像组合行为 | P2-5 | TOOL-RUNTIME-ADR §12.5 |
| V-9 | CI runner 上 podman 可用性 | 容器端到端测试进 CI 的排期 | 盘点 §9.1 |
| V-10 | rip.http.inspect 真实镜像（替换占位 digest）；已关闭（2026-11-10 P3-3：digest 固定镜像落地 + §9.1 回填 + 端到端经 Gateway::dispatch，见 TOOL-CONTRACT §11.1 与 docs/phase3/P3-3-BATCH-REPORT.md） | P3-2 | TOOL-CONTRACT §11.1 |
| V-11 | Token 估算常数对真实 tokenizer 校准 | P4-6 前 | CONTEXT-BROKER-DESIGN §13.1 |
| V-12 | RTIS knowledge.db 实际条数 | P4-3、P6-5 | 盘点 §5 口径差 |
| V-13 | tool_execution 历史 0 行，past_success 校准 | Phase 4 排序回看 | CONTEXT-BROKER-DESIGN §13.4 |
| V-14 | 向量检索轴接入时点（bge 与 search-engine 未接通） | Phase 4 之后（预留不实现） | CONTEXT-BROKER-DESIGN §13.5 |
| V-15 | Playwright 浏览器与 CJK 字体在无 sudo 约束下的用户级安装路径 | P5-9 | 本路线 §6.5 第 4 条（新识别） |
| V-16 | 仓库顶层 LICENSE 裁决 | P6-5（插件 license 字段） | PLUGIN-CONTRACT §12.2 |
| V-17 | workflow 表存量行数与形态 | P8-1 | PLUGIN-CONTRACT §12.3 |
| V-18 | rtis_mcp.mjs 以 MCP 宿主实挂验证 | P6-6（project_endpoint 通道） | TOOL-INVENTORY §9.2 |
| V-19 | http_api runtime 模板语法 | Phase 3 首个 http_api 工具 | TOOL-CONTRACT §11.5 |
| V-20 | reqmcp/yakitmcp 端到端（需外部 GUI 在线） | T11 合并登记 | TOOL-INVENTORY §9.3 |

---

## 15. 本会话核查记录（撰写本文实际执行的命令）

| 命令 | 输出/结论 |
|---|---|
| `ls docs/phase0/` | 7 份文档齐（本文件为其第 8 份） |
| `wc -l docs/phase0/*.md` | 449/224/472/250/993/238/275 行，与各文档口径一致 |
| `grep -n '^#' docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md` | §14 在 732 行、§16 在 883 行、§17 在 896 行、§18 在 915 行 |
| `sed -n '2273,2300p' /home/emo/Downloads/RIP-完整项目实施计划.md` | §23 十四步顺序与末句纪律取得 |
| `ls migrations/*.sql crates/` | 迁移 001 到 004；crates 六个（005 编号空闲，§1.4 裁决依据） |
| `git log -1 --oneline`（research-platform） | `022a717`，与各盘点文档基线一致 |
| `cat .github/workflows/ci.yml` | 验收命令来源：`cargo test --offline --workspace`、`cargo clippy --offline --workspace --all-targets -- -D warnings`、`python -m unittest discover -s tests -v` |
| `timeout 5 sudo -n true` | `sudo: 需要密码`，退出码 1（PC2-1 复核） |
| `podman --version` 与 `podman info --format '{{.Host.Security.Rootless}} {{.Store.GraphRoot}}'` | 6.1.4；`true /home/emo/.local/share/containers/storage`（PC2 复核） |
| `virsh uri` 与 `systemctl is-active libvirtd.service` | `qemu:///session` 退出码 0；`active`（PC2-2 复核） |
| `df -BG --output=avail /` | 259G（PC2-5 复核） |
| `ls -l /dev/kvm` | `crw-rw-rw-`（PC2-4 复核） |

未执行项：未启动任何虚拟机、未写任何数据库、未安装任何依赖、未修改任何源文件。本文所有验收命令为各阶段完工时待执行的 prescribe 形式，本文不将其计为已通过。
