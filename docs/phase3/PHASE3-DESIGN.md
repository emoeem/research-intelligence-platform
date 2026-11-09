# Phase 3 阶段说明与实施设计：首批 Tool Packs（P3-0 交付物）

- 日期：2026-11-10
- 状态：已定稿（本阶段批次 P3-0 的交付物；批次内部实现细节按 ROADMAP §1.2 冻结纪律可在各批开工时微调，批次边界与完成判据见 §6）
- 定位：PHASE-ROADMAP §1.2 冻结纪律「每个 Phase 开工前按参考计划 §24 模板出阶段说明」在 Phase 3 的执行件。§24 模板出处：/home/emo/Downloads/RIP-完整项目实施计划.md 2296 行起，十节为：目标、范围、非目标、现状、设计、实施批次、风险、验收、回滚、交付物。
- 执行基准：docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md（下称「计划」）§14 Phase 3（774 到 785 行）与 §4.3 接入标准（166 到 172 行）；docs/phase0/TOOL-RUNTIME-ADR.md（下称「ADR」）§3.1、§3.2、§5、§6、§7；docs/phase0/PHASE-ROADMAP.md（下称「ROADMAP」）§4；docs/phase0/TOOL-CONTRACT-DRAFT.md（下称「TOOL-CONTRACT」）§9、§11；docs/phase2/PHASE2-DESIGN.md（下称「PHASE2-DESIGN」）与 docs/phase2/PHASE2-ACCEPTANCE.md（下称「PHASE2-ACCEPTANCE」）。
- 红线遵守声明：本文为设计文档。撰写过程只新增本文档与 docs/README.md 一行登记，未修改任何源文件；未写 migrations/ 下任何 platform*.db；未在宿主机安装任何依赖（pacman -Q 基线 2415 行，本会话实测，见 §4.4）；未启动任何虚拟机（VM 面维持挂起，见 §3.1）。文中「现状」一节的结论全部来自本会话只读实跑命令或对既有文档的原文引用，逐条标注来源。

---

## 1. 目标

把计划 §14 Phase 3（774 到 785 行）的五包规划落成本阶段可验收的增量：在 Phase 2 已交付的 podman_host 通道与 Tool Gateway 之上，交付真实工具镜像、Manifest 注册与端到端运行证据，使首批工具从「通道验证负载」（rip-tool-base:smoke）升级为「有输入输出 Schema、有错误处理、有 Artifact 归档」的可用工具。

**本阶段范围裁定（冻结）**：Web Research 与 Core Utilities 两包做到端到端可靠；Source Analysis、Document、Mobile Analysis 三包只做 planned 登记（状态登记，不做镜像、不写工具、不进 tool 表）。裁定依据两条原文：

1. 计划 §14:784 原文：「不追求工具数量，优先保证安装、运行、结果 Schema、错误处理和证据归档可靠。」两包端到端可靠优先于五包铺开；ROADMAP §4.3 批次表标题即「实施批次草案」，本文件 §6 是按本裁定收敛后的冻结执行版。
2. ADR §3.2 过渡期纪律原文：「`podman_host` 只运行已登记且来源可信（自研或许可证核实）的工具；不受信工具一律等待 VM。」本阶段全部落地工具走自研薄包装（包装对象限基础镜像自带或发行版包内、许可证可核实的成熟 CLI），不受信第三方工具（TPscan 等）不登记、不落 podman_host；Document 与 Mobile 两包的重依赖按 ROADMAP P3-4/P3-6 原文建议属 VM 通道信任级，VM 挂起期间只能 planned。

同时承接 Phase 2 的三项遗留并明确处置（详见 §3.2 承接表）：VM 面 S1 到 S4 挂起不解除；rip-gateway systemd 常驻编排不落地（顺延）；引擎层 submit_key 唯一性加固不顺带修（独立加固项顺延，本阶段新增工具沿用已落地的「先查后派」幂等纪律）。

## 2. 范围

### 2.1 允许修改或新增的落点（全部在 research-platform 仓库内，独立 commit，逐批可回滚）

| 落点 | 内容 | 批次 |
|---|---|---|
| `containerfiles/rip-tool-webresearch/`（新增） | Web Research Pack 镜像 Containerfile 与工具脚本；基础镜像 digest 固定；构建全程容器内 | P3-1 |
| `containerfiles/rip-tool-coreutils/`（新增） | Core Utilities Pack 镜像 Containerfile 与工具脚本；同上 | P3-1 |
| `manifests/tools/`（新增条目） | 两包工具 Manifest，json/yaml 双份（首批 28 份先例），runtime.type=podman_host，image digest 固定 | P3-2/P3-3 |
| `crates/tool-engine` | adapter/podman_host.rs 网络面最小扩展：按 Manifest network.mode 声明映射容器网络 flag（现硬编码 none，podman_host.rs:350 实测，见 §5.3） | P3-2 |
| `crates/tool-engine/tests/`、`crates/runtime-gateway/tests/` | 两包工具的端到端与负向契约测试（podman_host_e2e、recovery_drill 同款载具形态） | P3-2 到 P3-5 |
| `docs/phase0/TOOL-CONTRACT-DRAFT.md` | 仅限两处回填：§9.1 示范 Manifest 的 `<digest>` 占位换真实 digest（554 与 635 行）、§11 待验证 1（719 行）加消解标注；不改契约语义 | P3-6 |
| `docs/phase3/` | 本文档 + 各批次验收报告（参考计划 §24 批次验收报告模板）+ P3-6 阶段验收 | 全程 |
| `docs/releases/` | 阶段更新记录（DOC-STANDARD §四 格式） | P3-6 |
| `docs/README.md` | 阶段工作目录登记（本批已登记 Phase 3 行） | P3-0 |

### 2.2 不修改

`apps/server` 既有端点行为、RTIS 本体（红线：只 Adapter 接入不改本体）、`workers/python` 既有 Worker Protocol v1 协议、M0 到 M8 已交付能力、`platform.db` 与 `platform-dryrun.db`（红线：全程不写，一切库操作走开发库副本与测试内存库）、`schemas/tool-manifest/v1.json`（本阶段不新增词表值：podman_host 已由 Phase 2 落入，网络面 egress_allowlist 与 egress.domains 字段已在 schema 241 到 266 行，实测在位）、`migrations/`（预计零新增，声明见 §2.4）。

### 2.3 三包 planned 登记（本批生效，P3-6 收口核对）

planned 登记的载体是本表与 P3-6 验收报告，不进 tool 表：tool 表只收已落地工具，podman_host 分支要求 image digest 固定（TOOL-CONTRACT 校验九条第 4 条），无镜像的 planned 工具登记进去等于造脏数据。

**存根落盘（P3-5 批次）**：三包 planned 状态与 P3-7/P3-8 挂起承接已落为独立轻量格式存根（`schemas/pack-plan/v1.json` 形状，status=planned，只写能力点、信任级裁定与解锁条件，不含任何可运行登记面）：`manifests/packs/planned/source-analysis.json`（P3-5，self-owned 首位）、`document.json`（P3-4，第三方解析库等 VM 通道）、`mobile-analysis.json`（P3-6，Java 生态等 VM 通道）、`tpscan-p37.json`（P3-7，不受信工具等 VM 且许可证未核，ADR §3.1 条件 1 监控载体）、`browser-automation-p38.json`（P3-8，超出宿主 rootless 可表达范围，ADR §3.1 条件 2 承接载体）。因校验九条第 4 条拒绝占位 digest，planned 存根不使用 tool-manifest v1 形状、不进 Registry、不建容器定义；本表五包行与五份存根一一对应（Core Utilities 与 Web Research 两行已交付，无存根）。

| Pack | 计划内容（计划 §14:774 到 783 原文要点） | 状态 | planned 理由与解锁条件 |
|---|---|---|---|
| Core Utilities | 文件元数据、哈希、压缩、格式转换、JSON/YAML/CSV | 本阶段交付（端到端） | 零网络、现成 CLI、信任面最小 |
| Web Research | HTTP、TLS、页面抓取、静态资源提取 | 本阶段交付（端到端，首工具 rip.http.inspect） | T1 只读 GET 面在宿主 rootless 可表达 |
| Source Analysis | Git、代码搜索、依赖清单、静态检查 | planned | 依赖轻、可走 podman_host，但数量让位于可靠（计划 §14:784）；解锁：本阶段验收通过后的下一批次预算 |
| Document | PDF、Office、Markdown、OCR/文本抽取 | planned | 第三方解析库按 ROADMAP P3-4 原文建议按信任级走 VM 通道；VM 面挂起中（§3.1） |
| Mobile Analysis | APK 元数据、Manifest、权限和基础静态分析 | planned | Java 生态依赖重，ROADMAP P3-6 原文建议 P2-6 后走 VM 通道；VM 面挂起中（§3.1） |

八族空白（TOOL-INVENTORY §7：Web/HTTP、DNS/Domain、Network、Web Analysis、Source Code、Mobile/APK、Binary/Reverse、Browser Automation）中的 Web/HTTP 由 Web Research Pack 覆盖，Source Code 与 Mobile/APK 随三包 planned，DNS/Domain、Network、Binary/Reverse 本阶段不动（ROADMAP §4.5 风险 3 口径：以五 Pack 最小集为边界，不设数量 KPI）。

### 2.4 迁移声明

本阶段预计零新增迁移：runtime_node（007）、tool_run 与 artifact（005）已由 Phase 1/2 交付并实测在位（`ls migrations/*.sql`：001 到 007，本会话实测），两包工具的注册与运行全部落在既有表上。如确需迁移：只新增编号文件，编号取落库当时下一个可用号（当前时点实查为 008，落库前再实查），并在开发库副本连续执行两次无差异（ROADMAP §1.4 冻结规则原文：「迁移编号按合入顺序递增，先到先得」与「新迁移一律走开发库副本与测试内存库」）；`platform.db` 与 `platform-dryrun.db` 全程不写。

## 3. 非目标

### 3.1 本阶段明确不做

1. **不建 VM，不执行任何 VM 写操作**。S1 到 S4 维持挂起（PHASE2-DESIGN §5.2.5 挂起批次清单）：`virsh pool-define-as`、`virt-install`、`virsh start`、`virsh snapshot-create` 等命令一条不跑，云镜像不下载，`~/rip-runtime` 下不建 VM 池目录。解除条件不变，仍是 PHASE2-DESIGN §3.2 逐条摘录的 ADR §3.1 两类条件；本阶段不产生新的解除判定输入（三包 planned 状态不构成触发）。
2. **不登记不受信工具上 podman_host**（ADR §3.2 原文，§1 引用）。具体到本阶段：TOOL-INVENTORY §4.3 的 TPscan（无 LICENSE 文件，许可证未核）不登记不构建；任何第三方克隆物在许可证核实前不进镜像；登记工具的包装对象限基础镜像自带或发行版包内、许可证可核实的成熟 CLI（§5.2 复用清单），provenance 记录来源与许可证。
3. **Source Analysis、Document、Mobile Analysis 三包不做实现**（§2.3 planned 状态）：不建镜像、不写 Manifest、不进 tool 表。
4. **Browser Automation 族不在本阶段实现**：真实浏览器环境超出宿主 rootless 可表达范围（ADR §3.1 提前触发条件 2 已列），落 VM 面随 S 批次；本结论随本文件落盘即为 ROADMAP P3-8 要求的评估结论，不再开批次（PHASE2-ACCEPTANCE §4 监控条目 2 同口径）。
5. **不落地 rip-gateway systemd user 单元与守护循环**（Phase 2 遗留，顺延）：心跳维持派发/恢复按需探测 + 600s 间隔守卫（crates/runtime-gateway/src/heartbeat.rs:22 `DEFAULT_PROBE_INTERVAL` 600s，实测）；编排归属 P2-4c/Phase 5 Runtime 页数据面前置项（PHASE2-ACCEPTANCE §7.2 原文），本阶段端到端验证以测试载具驱动，不新增常驻面。
6. **不做引擎层 submit_key 唯一性加固**（Phase 2 遗留，顺延）：PHASE2-ACCEPTANCE §7.1 口径（submit 入口同键活跃 run 查重或 005 表部分唯一索引）属 tool-engine 独立加固项；本阶段新增工具沿用已落地纪律（重放先按 submit_key 查既有 run 再续跑，PHASE2-ACCEPTANCE §3 演练 2 实测恰 1 行），批次中发现重复 submit 状态错乱按 §7 风险 6 升级，不顺带改引擎。
7. 不开放 remote_runner（TOOL-CONTRACT D-TC-01 维持）、不引入 docker（ADR §3.2：全程不用）、不做 http_api runtime 首例（TOOL-CONTRACT §11.5 模板语法未定，两包工具不含该类型）、CLI 派发入口不新建（tool-engine 无 `[[bin]]`，实测；CLI 属 Phase 5/9，ROADMAP §4.4 原文即承认 CLI 形态待定稿）。

### 3.2 Phase 2 遗留承接表

| 遗留项（来源） | Phase 3 处置 | 去向 |
|---|---|---|
| VM 面 S1 到 S4 挂起（PHASE2-DESIGN §5.2.5；PHASE2-ACCEPTANCE §7.3） | 维持挂起，不解除、不触碰；解除条件原文继续以 PHASE2-DESIGN §3.2 为准 | ADR §3.1 两类条件命中即解锁，设计已落盘不重开评审 |
| rip-gateway systemd 编排未落地（PHASE2-ACCEPTANCE §7.2） | 不落地；本阶段端到端经测试载具驱动（podman_host_e2e、recovery_drill 先例），Gateway 库面与心跳按需探测直接复用 | P2-4c/Phase 5 Runtime 页数据面前置项 |
| 引擎层 submit_key 唯一性加固项（PHASE2-ACCEPTANCE §7.1） | 不顺带修；新增工具 run 的幂等依赖派发器「先查后派」纪律（已由 P2-4b 落地并演练 2 实测） | tool-engine 独立加固项，工单化跟踪 |

## 4. 现状

### 4.1 代码与协议面（Phase 2 已交付，commit f64ee24，工作区干净，本会话 git status 实测）

- `crates/tool-engine`：Engine submit/drive 在位（`src/lib.rs:156` submit）；`src/adapter/podman_host.rs`（931 行）承载容器执行面：`rip-tool-` 前缀容器名 + `rip.runtime=rip-tool` label 双过滤（238 到 253 行）、`--network none` 硬编码（349 到 350 行）、`--read-only`（353 行）、`--userns=keep-id`（355 行附近）、`--cap-drop ALL`、`--pids-limit/--cpus/--memory` 与 tmpfs 封顶基线注入（54 到 77 行 ResourceLimits 默认值：cpus 2.0、memory 2 GiB、pids 256、tmpfs 1g，ADR §7 基线的代码化）；`--rm` 用后即删。e2e 与预算测试载具在位：`tests/podman_host_e2e.rs`（Phase 2 实测 5/5）、`tests/podman_host_budget.rs`（6/6）。
- `crates/runtime-gateway`：心跳（`src/heartbeat.rs`，默认探测周期 600s，最小 30s，不 tight-poll）、健康落库（health_store.rs）、WAITING_FOR_RUNNER 判定与 submit_key 幂等重放（tests/recovery_drill.rs，Phase 2 实测 3/3；wait_runner.rs 4/4）。
- `crates/tool-registry`：Manifest 校验九条（`src/validate.rs:1` 模块注释）、content_hash（99 行）、注册落库走 `store.rs`（tool + tool_version 两表，provenance_json/content_hash 列，TOOL-CONTRACT 154 行口径）。
- `migrations/`：001 到 007 在位（本会话 `ls migrations/*.sql` 实测），下一可用号 008；tool_run.submit_key 幂等列与 artifact sha256/role 词表（primary/attachment/stdout/stderr/log）随 005 在位。
- `schemas/tool-manifest/v1.json`：runtime.type 词表含 podman_host（Phase 2 追加）；network.mode 三值 none/egress_allowlist/vm_local，egress 含 ports（egress_allowlist 必填）/domains/cidrs/note（241 到 266 行，本会话实读）；`egress.domains` 语义即「静态域名白名单；动态目标由 Tool Policy Resolver 派发时生成临时条目，运行结束失效」（schema 257 行 description 原文）。
- `manifests/tools/`：28 份（14 工具，json/yaml 双份），全部 python_worker 与薄包装类型，尚无 podman_host 工具（本阶段补首批）。
- `containerfiles/rip-tool-base/`：Phase 2 冒烟镜像族（python:3.12-slim@sha256:4840d514... 基础镜像 digest 固定，Containerfile 头注记录构建命令与 keep-id 约束落点）。
- TOOL-CONTRACT §9.1 示范 Manifest 的 rip.http.inspect 为占位状态：image `registry.local/rip/http-inspector@sha256:<digest>`（554 行）与 provenance.image_digest（635 行）均为 `<digest>` 占位；§11（719 行）待验证 1 原文：「rip.http.inspect 镜像与工具本体不存在：本文档仅示范契约形状……真实交付属 Phase 2（基础镜像）与 Phase 3（Web Research Pack）」。本阶段 P3-2/P3-6 消解该项。
- 质量门基线（Phase 2 收口实测，RELEASE_2026-11-10_PHASE2 与 SESSION-HANDOFF 39 行口径）：`cargo test --offline --workspace` 194 全绿、`cargo clippy --offline --workspace --all-targets -- -D warnings` 零告警、worker unittest 125 全绿（跳过 11）；`cargo fmt --all -- --check` 通过。本批开工时重跑结果见 §4.5。

### 4.2 运行时面（宿主机，本会话只读实跑，2026-11-10，输出见 §4.4）

- podman 6.1.4 rootless：宿主容器通道可用（Phase 2 通道未动）。
- 交付物镜像在库：`localhost/rip-tool-base:smoke` digest `sha256:84877bf60e8a768c08f5b7a9b334eac48231b70f8b1c248008723590f79dc81a`（与 tool-manifest 双份及测试常量三处一致，PHASE2-ACCEPTANCE §3 演练 5 口径，本会话复核一致）。
- 宿主 podman 实例与用户自用容器共用：`podman ps -a` 实测 3 个用户容器（ubuntu、kali、wb-proxy），RIP 面零驻留（rip-tool label 过滤计数 0）。本阶段延续隔离纪律：新容器沿用 `rip-tool-` 前缀 + `rip.runtime=rip-tool` label，Gateway/清理只处理自有标识。
- 磁盘 242G 可用（高于 50 GiB 镜像预算阈值，ADR §7 与复审条件 4）；`pacman -Q | wc -l` = 2415（Phase 2 起点基线一致，零安装红线对照）。
- 构建离线化在位：`.cargo/config.toml` 指向 vendor 目录（本会话实读），`cargo test/clippy --offline` 即 CI 同门。
- VM 栈维持挂起态：本会话未执行任何 virsh 写操作（红线）。

### 4.3 待验证项（随对应批次验证，不预先断言结论）

- rootless 下 `--cpus`/`--memory` 真实生效性：Phase 2 已按行为测试裁决（podman_host_budget 6/6，PHASE2-ACCEPTANCE §5），本阶段新镜像沿用同基线并复验，不放宽。
- egress_allowlist 的容器网络映射（§5.3）：rootless 容器无域名级防火墙原语，本阶段实现口径与已知限制见 §5.3，行为测试在 P3-2。
- 新基础镜像的 digest 与本机 containers-storage 对账、补装 CLI 的许可证核实（P3-1/P3-2/P3-3 逐项落 provenance）。
- CI（GitHub runner）无 podman 保证（V-9，PHASE2-ACCEPTANCE §7.4）：容器用例 podman 门禁自动跳过保持 CI 绿，容器端到端保持本地执行，本阶段不变。

### 4.4 本会话复核记录（只读实跑，2026-11-10）

| 命令 | 输出摘要 |
|---|---|
| `git log --oneline -1` | `f64ee24 RIP 2.0 Phase 2 Podman Runtime 与 Tool Gateway 起步 podman_host`；`git status --short` 空（干净） |
| `ls migrations/*.sql` | 001 到 007 七个文件，最大 007，下一可用号 008 |
| `podman --version` | `podman version 6.1.4` |
| `podman images --format '{{.Repository}}:{{.Tag}} {{.Digest}}'` | 含 `localhost/rip-tool-base:smoke sha256:84877bf6...dc81a`（与 Phase 2 登记一致）；其余为用户自有镜像 |
| `podman ps -a --format '{{.Names}}'` | `ubuntu`、`kali`、`wb-proxy` 三个用户自用容器 |
| `df -BG --output=avail /` | `242G` |
| `pacman -Q \| wc -l` | `2415` |
| `grep -n 'cpus\|memory\|pids\|tmpfs' crates/tool-engine/src/adapter/podman_host.rs` | 54 到 77 行 ResourceLimits 基线注入（2.0/2GiB/256/1g） |
| `grep -n 'DEFAULT_PROBE_INTERVAL' crates/runtime-gateway/src/heartbeat.rs` | 22 行，600s（最小 30s） |
| `sed -n '784p' docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md` | 「不追求工具数量，优先保证安装、运行、结果 Schema、错误处理和证据归档可靠。」（裁定依据原文在位核对） |
| `sed -n '109,117p' docs/phase0/TOOL-RUNTIME-ADR.md` | §3.2 过渡期纪律四条（§1 引用的可信工具口径在位核对） |
| `grep -c '' crates/tool-engine/src/adapter/podman_host.rs` | 931 行 |

未执行项：未启动任何虚拟机、未写任何数据库、未安装任何依赖、未修改任何源文件（本文档与 docs/README.md 登记行除外）。

### 4.5 本批开工时质量门重跑（实测，2026-11-10）

| 门 | 命令 | 结果（实测） |
|---|---|---|
| 测试 | `cargo test --offline --workspace` | 194 全绿（0 失败 0 忽略），退出码 0 |
| 静态检查 | `cargo clippy --offline --workspace --all-targets -- -D warnings` | 零告警，退出码 0 |
| 格式 | `cargo fmt --all -- --check` | 通过，退出码 0 |
| Worker 测试 | `python3 -m unittest discover -s tests -v`（workers/python 目录下，CI 同命令） | 125 全绿（跳过 11），OK |

与 Phase 2 收口基线（§4.1 末条）完全一致，本阶段起点为干净基线。

## 5. 设计

### 5.1 总体形状

~~~text
Tool Gateway（库面 + 按需心跳，PHASE2 批次产物，本阶段不新增常驻面）
   │  submit → PREPARING（digest 强校验）→ RUNNING → SUCCEEDED/FAILED/TIMED_OUT
   ▼
podman_host Adapter（crates/tool-engine/src/adapter/podman_host.rs）
   │  挂载白名单 in(ro)/out(rw)/tmp(tmpfs)、keep-id、cap-drop ALL、资源基线
   ▼
两包镜像（P3-1 构建，digest 固定并登记）
   ├── localhost/rip-tool-webresearch:<tag>   rip.http.inspect（T1 只读 GET，出网按 §5.3）
   └── localhost/rip-tool-coreutils:<tag>     五个 Core 工具（network.mode=none）
   ▼
Manifest 注册（manifests/tools/ 双份 + 校验九条 + 开发库副本 tool/tool_version）
   ▼
Artifact 回收（out/ 全量 sha256 登记 artifact 表 + stdout/stderr 入表）
~~~

容器命名与 label 沿用 Phase 2 冻结名（`rip-tool-` 前缀 + `rip.runtime=rip-tool`，podman_host.rs:238 到 253 实测），与用户自用容器（ubuntu、kali、wb-proxy，§4.2 实测）隔离；不改名不改 label 语义，Gateway 清理面零改动。

### 5.2 工具与镜像设计（复用优先，不自研重复轮子）

复用纪律（本阶段选型总则）：稳定通用逻辑优先用成熟且在维护的现成 CLI/库，平台特有胶水（帧协议适配、输出 Schema 归一化、Manifest）才手写；每项包装对象的来源与许可证进 Manifest provenance（计划 §4.3 十项记录口径）。

| Pack | 工具 | 包装对象（优先现成） | 网络 | 备注 |
|---|---|---|---|---|
| Web Research | rip.http.inspect | 成熟 HTTP 客户端 CLI（首选 curl，curl license，维护活跃）+ 自研薄包装脚本（只放行 GET、归一化输出、落响应体到 out/） | egress_allowlist（ports 80/443，目标域经 Policy Resolver 临时条目） | T1 只读交互（TOOL-CONTRACT 139 行 GET 面原文）；非 GET 与越界方法在包装脚本层拒绝并有负向测试 |
| Core Utilities | rip.core.filemeta | `file`（BSD 许可证，发行版包） | none | 输出 MIME/编码/维度等归一化 JSON |
| Core Utilities | rip.core.hash | `sha256sum`（coreutils，GPLv3，基础镜像自带） | none | 输出 sha256/size，与 artifact 表同算法可直接对账 |
| Core Utilities | rip.core.compress | `gzip`/`tar`（GPLv3，基础镜像自带） | none | 压缩/解压到 out/，partial 照常登记 |
| Core Utilities | rip.core.convert | Python 标准库（json/csv，3.12-slim 自带） | none | 格式转换（json 到 csv 等受限矩阵，该批定稿并写 Schema） |
| Core Utilities | rip.core.data | Python 标准库 json/csv + YAML 现成实现（该批裁决：优先发行版包 python3-yaml（PyYAML，MIT）按许可证核实纳入，否则 YAML 支持降级为显式不支持并在 Schema 声明，不手写解析器） | none | JSON/YAML/CSV 解析与校验 |

镜像分组（ROADMAP P3-1 口径：每 Pack 一份 Containerfile）：基础镜像优先复用本机已在库的 digest 固定镜像（`debian:bookworm-slim@sha256:734a72b3...` 或 `python:3.12-slim@sha256:4840d514...`，§4.2 实测两者在库），coreutils/gzip/tar 等 debian 基座自带能力直接用；需补装的 CLI（file、curl、python3-yaml）在镜像构建期经包管理器安装，provenance 记录安装清单与版本；运行期容器 `--network none`（Web Research 工具按 §5.3 声明出网）。构建全程在容器内完成，宿主机零安装（pacman -Q 基线对照逐批复核）；中间镜像与临时容器用完即删（本批纪律）。

### 5.3 网络面设计（Web Research 唯一需要出网的点；P3-2 批次已实测冻结）

现状（P3-2 开工时实测）：Adapter 原把 `--network none` 单值硬编码（podman_host.rs:349 到 350），而 schema 已有 network.mode=egress_allowlist 分支（schemas/tool-manifest/v1.json:264 到 265）。P3-2 落地（crates/tool-engine/src/adapter/egress_proxy.rs，std::net 零新依赖）：

- **分派**：run_command 按 Manifest network.mode 分派，非单值硬编码：`none` 维持 `--network none`（P2-5 现状）；`egress_allowlist` 走 `--network pasta:-T,<proxy_port>` + http(s)_proxy env 注入（`http://127.0.0.1:<proxy_port>`，大小写四变量）。
- **实测冻结**（本机 podman 6.1.4：pasta 在位 `/usr/bin/pasta`，slirp4netns 不在）：pasta:-T 端口转发下容器内 `127.0.0.1:<port>` 即宿主 loopback 同端口（探针实测 pong 往返）；宿主接口 IP（192.168.1.116）直连变体被 pasta 拒（Connection refused），不采用。
- **本机 CONNECT 白名单代理**（run 生命周期内启动，Drop 停机）：allowlist = input.url 派生 host（精确匹配，Policy Resolver 职责最小化，TOOL-CONTRACT §9.1 note）+ egress.ports；CONNECT 与绝对形式 HTTP 都过白名单；拒绝一律回 403 并落结构化事件（`ProxyEvent::Rejected`，run 级台账 `proxy_events()`），不静默换通道；代理隧道/转发接受也留痕（TunnelAccepted/ForwardAccepted）。
- **跨 host 重定向即拒绝面**：allowlist 在派发时冻结，工具跟随重定向到名单外 host 的 CONNECT 被拒（测试用稳定公共重定向 npmjs.com 到 www.npmjs.com 实证），这使「域名级白名单」在代理层有真实执行语义（对派发时可见的初始目标与重定向链）。

已知限制（如实声明，不虚报，ADR §8 已知代价的宿主面版本）：代理层策略执行只对遵守 http(s)_proxy env 的客户端（自有可信工具面）有效，容器绕过代理的直连未被网络层阻断；网络层硬隔离属 podman_vm 目标拓扑，解锁前 podman_host 不登记不受信工具（ADR §3.2）。`egress.domains` 静态字段继续作为派发决策记录与审计留痕（schema 257 行 description 口径）。负向测试覆盖：非白名单 CONNECT 403 + 结构化错误、mode=none 工具无网（分派守卫）。

### 5.4 Manifest 注册流

新增 Manifest 走 Phase 1 既有链路，不新造注册器：`manifests/tools/rip.http.inspect.{yaml,json}` 等（双份，首批 28 份先例），内容过校验九条（tool-registry validate.rs），content_hash 落 tool_version；落库只走开发库副本与测试内存库，`platform*.db` 不写；Registry 只读对账命令同 ROADMAP §2.4 第 4 条（tool 行数与 manifests 目录条目对账）。podman_host 分支必填字段与 podman_vm 同构（image digest 固定、entrypoint，TOOL-CONTRACT 81 行），permission_level=T1（两包工具全部只读或纯本地变换）。

### 5.5 资源限制基线（照抄 ADR §7，ADR 184 到 199 行，逐批复验不放宽）

直接沿用 PHASE2-DESIGN §5.1.3 全表（CPU 2 核、内存 2 GiB、pids 256、时长 600000 ms、输出 64 MiB 截断、并发每工具族 1、tmpfs 1 GiB、镜像预算 50 GiB、根文件系统只读、网络默认关闭），该表与 ADR:188 到 199 逐字一致且已代码化（ResourceLimits，podman_host.rs:54 到 77）并经行为测试（podman_host_budget 6/6）。本阶段不复述全表，只声明三件事：新镜像族沿用同基线；每批验收复验 `podman system df` 与 `df`（50 GiB 预算与 242G 实测余量）；放宽路径仍然只有 Tool Policy Resolver 裁决并记录，批次内无放宽。低并发纪律延续：验收 run 串行执行，请求间隔 ≥1s。

### 5.6 Artifact 回收与哈希校验（ADR §5 六步的宿主通道延续）

run 结束由 Gateway 对 `out/` 全量产物计算 sha256 登记 artifact 表（005 表 role 词表 primary/attachment/stdout/stderr/log，005 注释口径：结构化协议行以外的一律入本表，含 stdout/stderr）；对账双向：`sha256sum <artifact_uri>` 与表内 sha256 列一致（P3-5 判据），partial（超时/取消）产出照常登记并标注；超时与取消后容器回收、tmp/ 用后即焚（Phase 2 行为已在 podman_host_budget/recovery_drill 覆盖，本阶段随新镜像回归）。开发库副本只读对账（sqlite mode=ro），不写生产库。

## 6. 实施批次

本阶段共 7 批（P3-0 到 P3-6）；P3-0 即本文档。每批完成判据即该批验收命令通过并出批次验收报告（参考计划 §24 批次验收报告模板）。与 ROADMAP §4.3 草案的映射：本表 P3-1/P3-2/P3-3 对应 ROADMAP P3-1/P3-2/P3-3；ROADMAP P3-4/P3-5/P3-6 按 §2.3 planned（不建批次）；ROADMAP P3-7 不启动（TPscan 不受信不登记，§3.1 第 2 条）；ROADMAP P3-8 的评估结论随本文件落盘（§3.1 第 4 条），不建批次。收敛依据即 §1 两条裁定原文。

| 批次 | 内容 | 最小可验证单元（完成判据） | 并行性 |
|---|---|---|---|
| P3-0（本批） | 阶段说明（本文档，§24 十节） | 十节齐备；范围裁定与两条依据原文落盘；遗留承接表落盘；迁移声明落盘（§2.4）；零长横线；docs/README.md 登记 | 起点 |
| P3-1 | 两包 Containerfile 分组构建：containerfiles/rip-tool-webresearch/ 与 containerfiles/rip-tool-coreutils/（基础镜像 digest 固定，补装 CLI 在构建期，provenance 记录） | 两镜像构建成功且 `podman images --digests` digest 与登记一致；构建期宿主 pacman -Q 无变化；中间镜像用后即删 | 【串行】起点 |
| P3-2 | Web Research Pack 首工具 rip.http.inspect：薄包装脚本 + Manifest 双份注册 + Adapter 网络面最小扩展（§5.3） + T1 语义负向测试 | 对 example.com 的 GET run 端到端 SUCCEEDED，结构化输出 + 响应体 Artifact；tag 引用被 digest 强校验拒绝；mode=none 工具维持无网断言通过 | 【串行】依赖 P3-1 |
| P3-3 | Core Utilities Pack 五工具（§5.2 表）：五份 Manifest 双份注册，校验九条全过，network.mode=none | 五工具注册成功且开发库副本 tool/tool_version 对账一致；YAML 支持口径按 §5.2 裁决落定并写进 Schema 描述 | 【并行】依赖 P3-1（与 P3-2 无共享写面） |
| P3-4 | 端到端运行收口：两包全部登记工具各一个真实 run 经 Gateway + podman_host Adapter（测试载具形态，podman_host_e2e/recovery_drill 先例）；资源基线复验；等待运行与恢复路径覆盖 | 两包全部工具 run SUCCEEDED；WAITING_FOR_RUNNER 演练不误报 FAILED；验收后 `podman ps -a --filter label=rip.runtime=rip-tool -q \| wc -l` = 0（临时容器零驻留） | 【串行】依赖 P3-2/P3-3 |
| P3-5 | Artifact 回收与哈希校验：out/ sha256 与 artifact 表双向对账 + partial 登记 + 失败 run 错误信封与 stderr 归档不静默 | 每个成功 run 的 artifact.sha256 与 `sha256sum` 落盘文件一致；partial 用例 role 与标注正确；开发库副本只读对账通过 | 【串行】依赖 P3-4 |
| P3-6 | 收口：镜像 digest 回填 TOOL-CONTRACT §9.1 占位（554/635 行）与 §11 待验证 1 消解标注（719 行）；三包 planned 登记核对（§2.3）；三道门 + fmt + clippy 全绿；阶段验收报告与 RELEASE | §8 全部命令按预期退出；TOOL-CONTRACT 内 `<digest>` 占位清零（grep 实证）；RELEASE 落盘 | 【串行】依赖 P3-5 |

本阶段完成后 Runner 与工具全景：python_worker（Phase 1，14 工具）、podman_host（Phase 2 通道 + 本阶段两包 N 工具）、podman_vm 维持设计挂起态；不受信工具与浏览器族继续等待 VM（ADR §3.2）。

执行偏差记录（如实声明）：P3-2 在执行中拆为两半：出站 allowlist 执行面（egress_proxy + 分派 + env 注入 + 正负契约测试）为独立批次先行交付（ROADMAP P3-2 的技术前置；因 Adapter 原状 `--network none` 单值硬编码，podman_host.rs:349 到 350 实测）；rip.http.inspect 的 Manifest 双份注册与首工具端到端判据由 P3-3 批次完成（批次验收报告 docs/phase3/P3-3-BATCH-REPORT.md：计划 §4.3 三项验证实测、开发库副本 tool_run SUCCEEDED 对账、TOOL-CONTRACT §9.1 占位回填消解 §11 待验证项 1 与 ROADMAP V-10），判据不变。

## 7. 风险

1. **宿主 podman 实例与用户自用容器共用**（§4.2 实测 ubuntu/kali/wb-proxy 在库在跑）：误伤用户容器是本阶段最高危面。缓解：`rip-tool-` 前缀 + `rip.runtime=rip-tool` label 双过滤（Phase 2 冻结名，本阶段不改），清理与镜像删除只按自有标识，禁用系统级 prune；P3-1/P3-4 验收含用户容器存活断言。
2. **构建期供应链**：补装 CLI 走发行版包存在版本漂移。缓解：基础镜像 digest 固定后包索引状态可复现；provenance 记录安装清单与版本；不引入第三方构建镜像；镜像 digest 固定后运行面不再变动（TOOL-CONTRACT 校验九条第 4 条）。
3. **egress_allowlist 语义差距**（§5.3）：域名级阻断本阶段不做，egress.domains 只是决策与审计记录。误用风险：把声明当防线。缓解：已知限制写入验收报告与 Manifest note 字段；T1 语义由包装脚本层强制（GET 只放行）并有负向测试；出网端口收敛 80/443。
4. **镜像体积与磁盘预算**（ROADMAP §4.5 风险 2）：两包独立镜像叠加基础镜像层。缓解：50 GiB 预算逐批检查（本会话磁盘 242G 余量），中间镜像用完即删，`podman system df` 入批次报告。
5. **「不提前实现」纪律与三包诱惑**：Source Analysis 依赖轻、最容易被顺手做掉。缓解：§2.3 planned 状态冻结，实现类改动出现即视为越界，验收报告记录并回滚。
6. **submit_key 无引擎层唯一约束下的重复提交**（§3.2 承接项 3）：两包新工具的幂等依赖「先查后派」。缓解：P3-4 端到端含幂等重放用例（recovery_drill 演练 2 同款断言恰 1 行）；出现重复副作用立即停批上报，不顺带改引擎（§3.1 第 6 条）。
7. **低并发纪律与验收效率**：并发基线每工具族 1、间隔 ≥1s（ADR §7 与 AGENTS.md 纪律），验收不为提速改高并发。
8. **TOOL-CONTRACT 回填漂移**：回填若与 §9.1 示范字段形状冲突（如镜像内路径不同），以实现为准并在回填处注明差异，契约语义修订留收口评审，不静默改契约。

## 8. 验收

阶段验收命令（P3-6 收口时全跑；各批次判据命令随批次验收报告落盘）。方括号为占位符，执行时按实际值替换。

~~~bash
cd /home/emo/code/pensting/research-platform

# 0. 质量门（与 CI 同命令 + fmt，Phase 2 收口同款）
cargo test --offline --workspace
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tests -v

# 1. 文档自检（P3-0）
grep -c '^## ' docs/phase3/PHASE3-DESIGN.md            # 期望 ≥ 10（§24 十节）
grep -P '[\x{2014}\x{2013}]' docs/phase3/PHASE3-DESIGN.md; echo "dash_grep_exit=$?"   # 期望退出码 1（零长横线）
grep -rP '\x{2014}|\x{2013}' docs/ ; echo "docs_dash_exit=$?"                        # 期望 1（DOC-STANDARD 提交前检查）

# 2. 宿主机零安装对照（计划 §14 验收口径延续；基线 2415 行，§4.4 实测）
pacman -Q | wc -l                                       # 期望 2415，全程 diff 为空
# 镜像构建/拉取属运行时数据不属安装（ADR §3.2 口径）。

# 3. 镜像 digest 固定复核（P3-1 判据）
podman images --digests --format '{{.Repository}}:{{.Tag}} {{.Digest}}' | grep rip-tool
# 期望：两包镜像在列，digest 与 Containerfile 头注/Manifest provenance 登记一致

# 4. Registry 对账（开发库副本，只读，不触生产库；ROADMAP §2.4 第 4 条同款）
python3 -I -c "import sqlite3; c=sqlite3.connect('file:[开发库副本路径]?mode=ro',uri=True); print(c.execute('select count(*) from tool').fetchall())"
# 期望：tool 行数 = 原有 + 本阶段新登记数；tool_version.provenance_json 非空

# 5. 端到端 run 与 Artifact 对账（P3-4/P3-5 判据；run 经测试载具发起，形态同 podman_host_e2e）
sha256sum [artifact_uri 指向的文件]                     # 与 artifact.sha256 列一致
python3 -I -c "import sqlite3; c=sqlite3.connect('file:[开发库副本路径]?mode=ro',uri=True); print(c.execute('select run_id,status,tool_id from tool_run order by rowid desc limit 5').fetchall())"

# 6. 清理面复核（不碰用户容器；ubuntu/kali/wb-proxy 须仍在列）
podman ps -a --filter label=rip.runtime=rip-tool --format '{{.Names}}'   # 期望空
podman ps -a --format '{{.Names}}'                                       # 期望含 ubuntu、kali、wb-proxy

# 7. 占位回填核对（P3-6 判据）
grep -c '<digest>' docs/phase0/TOOL-CONTRACT-DRAFT.md   # 期望 0（P3-6 后）
~~~

计划 §14 Phase 3 验收口径在本阶段的达成形式：每个已交付 Pack 至少一个工具完成「安装、运行、结果 Schema、错误处理、证据归档」五点可靠链路（计划 §14:784 原文），证据为 P3-4/P3-5 的 run 记录、负向测试与 Artifact 对账，逐工具入批次验收报告。

## 9. 回滚

- **代码与镜像定义**：本阶段每批次独立 commit，`git revert <批次 commit>` 即整体撤销该批；revert 不触及 M0 到 M8 与 Phase 1/2 交付物（红线），tool-engine 既有行为由回归测试守护（三道门 + fmt）。
- **镜像与容器**（label 过滤保证不碰用户资产，顺序执行）：
  ~~~bash
  podman rm -f $(podman ps -aq --filter label=rip.runtime=rip-tool)
  podman rmi $(podman images -q --filter label=rip.runtime=rip-tool)
  rm -rf ~/rip-runtime/workspaces/*
  ~~~
  基础镜像（debian:bookworm-slim、python:3.12-slim）为共用层不动；用户镜像（ubuntu/kali/wb-proxy 等自有资产）一律不触碰。
- **Manifest 与 Registry**：新增 Manifest 条目随 commit 回滚；开发库副本中 tool/tool_version 测试行随副本回退（ROADMAP §1.4 口径：库操作只走开发库副本与测试内存库），生产 `platform*.db` 全程未写（红线），无生产回滚面。
- **TOOL-CONTRACT 回填**：`git revert` 恢复 `<digest>` 占位与待验证 1 原状；回填 commit 独立，可单独撤销。
- **迁移**：预计零新增（§2.4）；如确需，幂等 DDL 走开发库副本回退流程，生产库零写入，无生产回滚面。
- **VM 面**：本阶段未执行任何 VM 操作，无 VM 回滚面（S1 到 S4 维持挂起，PHASE2-DESIGN §5.2.5 不变）。

## 10. 交付物

- **代码**：`containerfiles/rip-tool-webresearch/`（Containerfile + rip.http.inspect 包装脚本）、`containerfiles/rip-tool-coreutils/`（Containerfile + 五工具脚本或 CLI 调用层）、`crates/tool-engine` 网络面最小扩展与两包契约/负向测试（adapter/podman_host.rs 与 tests/ 下）、`crates/runtime-gateway/tests/` 端到端用例扩展（如需）。预计零新增迁移（§2.4 声明）、零新增 crate。
- **Manifest**：`manifests/tools/` 新增两包工具条目（json/yaml 双份，podman_host 类型首批）。
- **文档**：本文档（`docs/phase3/PHASE3-DESIGN.md`）；P3-1 到 P3-6 各批次验收报告（`docs/phase3/` 下，按参考计划 §24 批次验收报告模板，含命令原文与输出摘要）；P3-6 阶段更新记录（`docs/releases/`，DOC-STANDARD §四 格式）；TOOL-CONTRACT-DRAFT.md 两处回填（§6 P3-6 范围）；docs/README.md 登记行（本批已落）。
- **测试**：两包工具端到端（真实容器 run）、负向（tag 引用拒绝、mode=none 无网、T1 方法越界拒绝、幂等重放恰 1 行）、Artifact 对账、用户容器不触碰断言。
- **数据**：无生产库写入（红线）；开发库副本上的 tool/tool_version 注册行与 run/artifact 测试行；镜像为运行时数据（ADR §3.2 口径），总量受 50 GiB 预算约束。
