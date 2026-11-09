# Phase 0 交付物 6：Tool Contract 草案（TOOL-CONTRACT-DRAFT）

日期：2026-11-09
性质：设计草案（Phase 0 交付物，计划 §16 第 6 条「Tool Manifest、输入输出和 Run 协议草案」）。
执行基准：`docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md` §4（工具装备体系）、§6（Tool Gateway 与 Tool Engine）、§12（数据模型建议）；Phase 1 落地边界见同计划 §14「Phase 1：Tool Contract 与 Registry」。
输入材料：`docs/phase0/CURRENT-ARCHITECTURE-INVENTORY.md`、`docs/phase0/TOOL-INVENTORY.md`、`schemas/worker-protocol/README.md` 与 `schemas/worker-protocol/v1.json`、`migrations/001_init.sql`、`migrations/003_tools_workflow.sql`、`workers/python/parsing_worker.py`。
红线遵守：本文档为纯设计产物，本次未修改任何源文件、未写 `migrations/` 下任何 `platform*.db`、未修改生产 RTIS 行为、未在宿主机安装任何依赖。文中对 platform.db 的两次查询均为 `mode=ro` 只读。

证据口径：所有结论来自本会话实际读取的文件（给出 `路径:行号`）或实际执行的命令（给出命令原文与输出摘要）。未能亲自验证的项明确标注「待验证」。

---

## 1. 文档定位与设计基线

### 1.1 本文档定义什么

1. Tool Manifest v1：工具登记的完整字段规范（§2），含顶层字段表、runtime 分类型必填字段、resources/network/secrets/health/provenance 补充字段，以及 Registry 落库前的校验规则。
2. ToolRun 生命周期状态机：QUEUED 到四个终态的完整跃迁表（§3）。
3. 统一调用协议对象 ToolCallRequest（§4）：所有 runtime 共用的派发请求形状。
4. 统一输出约定 ToolRunResult 信封（§5）：结构化 JSON 加 Artifact 引用、错误类别、usage、环境指纹、可重放脱敏参数。
5. 版本与兼容策略（§6）。
6. 健康检查（§7）。
7. 两个具体示例 Manifest：python_worker 包装现有 parsing worker、podman_vm 容器化 HTTP 工具（§8、§9）。
8. Phase 1 实施指引（§10）：落地物清单、首批登记批次、验收对照。

### 1.2 既有协议与数据面基线（草案不另起炉灶的依据）

本草案的三条硬基线，全部来自本会话实际读取：

1. Worker Protocol v1 是统一调用协议在 python_worker runtime 上的现成实现。5 方法（initialize / get_capabilities / execute / cancel / ping）见 `workers/python/runtime/worker_runtime.py:154` 与 `schemas/worker-protocol/README.md:13-21`；budget 默认与硬上限（timeout_ms 默认 30000，上限 [1, 600000]；max_output_bytes 默认 1048576，上限 [256, 67108864]）见 `worker_runtime.py:34-36` 与 `README.md:55-68`；幂等缓存与取消语义见 `README.md:74-86`。架构盘点结论一致：「Worker Protocol v1 是 2.0 Tool Contract 最接近的现有协议基础」（`docs/phase0/CURRENT-ARCHITECTURE-INVENTORY.md:102`）。
2. 工具数据面已存在三张表：`tool`（lifecycle 五态 REGISTERED/VALIDATED/ENABLED/DISABLED/DEPRECATED、governance 四态、permission_level T0/T1/T2、input_schema/output_schema/timeout_s/binary_ref，`migrations/001_init.sql:278-301`）、`tool_version`（version、provenance_json、content_hash、validated_at、status current/superseded，`001_init.sql:303-315`）、`tool_execution`（exec_id/tool_id/argv_hash/exit_code/duration_ms/started_at/permission_ctx，`migrations/003_tools_workflow.sql:79-91`）。本草案的状态机与字段命名必须能映射到这三张表，不引入平行概念。
3. 现网数据形状（本会话只读实测，命令：`python3 -I -c "sqlite3.connect('file:...platform.db?mode=ro', uri=True)..."`）：`tool` 表 20 行，tool_id 为纯短名（`nmap`、`nuclei`、`fscan` 等），kind 取值 `cli`/`mcp`/`datastore`，permission_level 取值 `T0`/`T1`，lifecycle 全为 `REGISTERED`；`tool_version` 20 行，health 实际取值 `ok`/`degraded`/`unknown`；`tool_execution` 0 行（与架构盘点差距清单第 3 条「工具执行面为零」一致，`CURRENT-ARCHITECTURE-INVENTORY.md:218`）。

### 1.3 设计决策记录

| 编号 | 决策 | 依据 |
|---|---|---|
| D-TC-01 | Manifest 的 `runtime.type` 词表采用计划 §6 Runtime 类型表：`podman_vm` / `python_worker` / `native_rust` / `mcp_client` / `http_api` / `cli_adapter` / `remote_runner`。§4.4 示例中的 `type: "container"` 视为 `podman_vm` 的别名，Registry 校验器接受并归一化，新登记一律写 `podman_vm` | 计划 §6 Runtime 类型表（`RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md:358-370`）是模块边界的权威口径；§4.4（同文件 176-208 行）只是示例 |
| D-TC-02 | Manifest `id` 必须带命名空间：正则 `^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$`，至少两段（如 `rip.doc.parse`）。Registry 映射到 `tool.tool_id` 主键时直接使用 manifest id；现网 20 行短名（nmap 等）不强制改名，按「旧格式存量，登记 Manifest 时保留原 id」处理，Phase 1 出对账清单 | 现网实测 tool_id 无点短名；命名空间避免跨来源冲突（计划 §4.3 要求记录来源仓库，同文件 166-172 行） |
| D-TC-03 | 调用协议不新造传输层：python_worker 复用 Worker Protocol v1 的 execute 通道，ToolCallRequest 的 `input` 原样映射到 execute.params.task；podman_vm 容器以「stdin 一行 JSON 请求，stdout 一行 JSON 结果」为默认帧协议（与 worker 分帧一致），HTTP 工具经 Runtime Adapter 翻译 | Worker Protocol v1 分帧规则（`schemas/worker-protocol/README.md:5-11`）；避免两套协议 |
| D-TC-04 | 状态机不引入 CANCELLING 中间态。取消请求记录 `cancel_requested_at` 时间戳，任务保持 RUNNING，直到协作式取消生效（转 CANCELLED）或任务自行完成（转 SUCCEEDED）。此语义与 Worker Protocol v1 的协作式取消完全一致（handler 无视令牌时任务可能正常交付，`README.md:84-86`） | 计划 §4.5 统一状态清单只有 QUEUED/PREPARING/RUNNING/四终态（同文件 225 行） |
| D-TC-05 | 提交时的 Schema 校验失败同步拒绝，不创建 ToolRun 行；PREPARING/RUNNING 期间暴露的输入问题才算 FAILED（error_category=INPUT 或 VALIDATION）。避免用 FAILED 淹没 Registry 校验职责 | 计划 §14 Phase 1 验收「验证参数」（同文件 761 行）；Registry 校验是落库前置职责 |
| D-TC-06 | secrets 不进调用协议、不进 Manifest 值、不进重放记录。Manifest 只声明 secret 名与注入方式；Tool Policy Resolver 在派发时从平台 secrets 存储解析并注入运行环境 | 计划 §4.5 要求「可重放的调用参数（敏感值脱敏）」（同文件 235 行）；§5.4 容器约束（同文件 300-318 行） |
| D-TC-07 | health_state 词表沿用现网 `tool_version.health` 的 CHECK 域：`unknown` / `ok` / `degraded` / `broken`，不新增状态 | `migrations/003_tools_workflow.sql:26-29`（守卫 DDL 说明）+ 本会话只读实测 DISTINCT health 返回 ok/degraded/unknown |

---

## 2. Tool Manifest v1 字段规范

### 2.1 顶层字段表

约定：REQUIRED 表示 Registry 落库前校验必填；类型列中「object」等指 YAML/JSON 类型；JSON Schema 指 draft 2020-12（与 `schemas/worker-protocol/v1.json:2` 一致）。

| 字段 | 类型 | 必填 | 约束与语义 |
|---|---|---|---|
| `schema_version` | string | 是 | 本 Manifest 格式主版本，当前恒为 `"1"`（只加可选字段不改语义则不变，见 §6.1） |
| `id` | string | 是 | 全局唯一工具标识，正则 `^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$`（D-TC-02） |
| `name` | string | 是 | 人类可读名，1 至 80 字符 |
| `version` | string | 是 | 工具自身语义化版本 X.Y.Z（§6.2），与 `tool_version.version` 一一对应 |
| `kind` | string | 是 | 恒为 `"tool"`（计划 §4.4:181）；插件等其他 kind 属 Plugin Contract 范围，不在本 Manifest |
| `family` | string | 是 | §4.2 十三工具族之一：`web_http` / `dns_domain` / `network` / `web_analysis` / `source_code` / `mobile_apk` / `binary_reverse` / `document` / `data_processing` / `knowledge` / `evidence` / `utility` / `browser_automation`（计划 §4.2:148-162） |
| `description` | string | 是 | 一句话能力描述 |
| `runtime` | object | 是 | 执行后端声明，按 §2.2 分类型必填 |
| `input_schema` | object | 是 | JSON Schema draft 2020-12，校验 ToolCallRequest.input |
| `output_schema` | object | 是 | JSON Schema draft 2020-12，校验 ToolRunResult.result（仅 SUCCEEDED 时） |
| `resources` | object | 是 | 见 §2.3 |
| `network` | object | 是 | 见 §2.3，离线工具写 `mode: "none"` |
| `secrets` | array | 是 | 允许空数组；条目见 §2.3（D-TC-06） |
| `permission_level` | string | 是 | `T0`/`T1`/`T2`，域沿用 `tool.permission_level`（`001_init.sql:289-290`）；语义裁定见 §2.4 |
| `idempotent` | boolean | 是 | true 表示同输入重复执行产出相同结果，Registry 可安全重放；python_worker 工具用 `idempotency_key` 支撑（`README.md:74-79`） |
| `health` | object | 是 | 健康探针声明，见 §7 |
| `provenance` | object | 是 | 来源与许可，字段见 §2.3；对齐计划 §4.3「接入时记录」清单（同文件 166-172 行） |
| `context` | object | 否 | `summary`（给 Agent 的一句话上下文）与 `docs`（文档路径），沿用计划 §4.4:205-207 |
| `tags` | array | 否 | 字符串数组，供 Tool Center 检索（计划 §4.6:243） |
| `examples` | array | 否 | 输入/输出样例对，供 Mock Runner 契约测试与升级兼容检查（§6.3） |

### 2.2 runtime 分类型必填字段

| runtime.type | 必填字段 | 说明 |
|---|---|---|
| `podman_vm` | `image`（必须 digest 固定 `@sha256:<64hex>`）、`entrypoint`（数组）；可选 `user`（默认非 root）、`readonly_rootfs`（默认 true）、`mounts`（role 为 input/output，路径限任务目录）、`env`（值支持 `{{run_id}}` 等模板变量） | 对齐计划 §4.4:183-186 与 §5.4 容器约束（同文件 300-318 行：非 root、只读根文件系统、只挂任务目录、不挂宿主机目录与容器 Socket） |
| `python_worker` | `protocol_version`（当前 `"1"`）、`worker_name`（须与 initialize 响应一致）、`command`（启动数组）；可选 `working_dir`（`repo://` 或 `artifact://` 前缀）、`restart`（`pooled` 默认复用进程 / `per_run` 每次运行新进程） | 协议与方法集见 `worker_runtime.py:154`；stdio 常驻服务运行方式见 TOOL-INVENTORY §1.2 |
| `native_rust` | `binary`（rp-resolve / rp-search-index / rp-review 等 Registry 已知二进制名）、`subcommand` | 现有二进制清单见 `CURRENT-ARCHITECTURE-INVENTORY.md:68, 75, 82`；禁止宿主机任意 shell（计划 §6:370） |
| `mcp_client` | `endpoint`（stdio 命令或 URL）、`tool_name`（远端工具名） | 参考 `rtis_mcp.mjs` 的 stdio JSON-RPC 2.0 形态（`CURRENT-ARCHITECTURE-INVENTORY.md:123`） |
| `http_api` | `base_url`、`method`、`auth`（`none`/`bearer_secret`/`header_secret`，引用 secret 名）；可选 `request_template`、`response_mapping` | 计划 §6:366 |
| `cli_adapter` | `image`（该 CLI 所在 Tool Pack 镜像，digest 固定）、`argv_template`（参数占位从 input_schema 映射） | 计划 §6:367「已安装在 VM 的 CLI 工具」 |
| `remote_runner` | Phase 1 不开放登记（计划 §6:368 定为 P2）；Registry 校验器直接拒绝 | 避免提前实现后续阶段能力（计划 §15:872） |

### 2.3 resources / network / secrets / provenance 字段规范

**resources**（对齐计划 §4.4:201-204 与 §5.4「设置 CPU、内存、PID、磁盘和执行时间上限」，同文件 312 行）：

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `cpu` | string | 是 | Kubernetes 数量格式，如 `"1"`、`"500m"` |
| `memory` | string | 是 | 字节量格式，如 `"512Mi"` |
| `timeout_seconds` | integer | 是 | [1, 600]，执行硬上限；超出转 TIMED_OUT（python_worker runtime 须换算进 budget.timeout_ms 的 [1, 600000] ms 域，`worker_runtime.py:35`） |
| `max_output_bytes` | integer | 否 | 结构化结果行字节上限，默认 1048576；对齐 Worker Protocol budget.max_output_bytes（`README.md:62-68`） |
| `pid_limit` | integer | 否 | 容器 PID 上限（podman_vm / cli_adapter） |
| `disk_mb` | integer | 否 | 任务目录磁盘配额（podman_vm / cli_adapter） |

**network**（对齐计划 §5.4「网络访问由执行配置决定，工具不能任意改变 VM 网络规则」，同文件 313 行）：

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `mode` | string | 是 | `none`（无网络，默认建议）/ `egress_allowlist`（白名单出网）/ `vm_local`（仅 VM 内部互通） |
| `egress.ports` | array | mode=egress_allowlist 时必填 | 允许出站端口数组 |
| `egress.domains` | array | 否 | 静态域名白名单；动态目标（如 `rip.http.inspect` 的请求 URL）由 Tool Policy Resolver 在派发时生成临时 allowlist 条目，运行结束失效 |
| `egress.cidrs` | array | 否 | CIDR 白名单 |

**secrets**（D-TC-06，条目对象）：

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `name` | string | 是 | 平台 secrets 存储中的键名，正则 `^[A-Z][A-Z0-9_]+$`；Manifest 中绝不出现值 |
| `inject_as` | string | 是 | `env`（环境变量）或 `file`（挂载文件，权限 600） |
| `required` | boolean | 是 | false 表示可选缺失时跳过注入 |

**provenance**（对齐计划 §4.3「来源仓库/发行页、版本或 Commit、许可证、维护状态、平台支持、依赖、镜像摘要、输入输出 Schema、资源要求、已知限制和测试结果」，同文件 170 行）：

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `origin` | string | 是 | `rip-native`（自研）/ `opensource`（第三方）/ `internal-kit`（内部 kit） |
| `source_url` | string | origin=opensource 时必填 | 上游仓库或发行页 |
| `source_commit` | string | origin=opensource 时必填 | 上游版本或 Commit |
| `license` | string | 是 | SPDX 标识或 `repo-internal`；未知必须写 `unknown`，此时 Registry 拒绝登记（TOOL-INVENTORY 对 TPscan 的裁定，TOOL-INVENTORY.md:144, 204） |
| `dependencies` | array | 是 | 运行依赖清单（可为空数组）；python_worker 的依赖须注明位于仓库内 `.venv`（TOOL-INVENTORY.md:46） |
| `image_digest` | string | runtime.type=podman_vm 时必填 | 与 runtime.image 内 digest 一致，Registry 校验一致性 |
| `known_limits` | array | 否 | 已知限制（如 parsing 输入上限 10 MiB） |
| `tested` | string | 是 | 最近一次验证的方式与结果摘要（如「workers/python/tests 125 项全绿，TOOL-INVENTORY §1.5」） |

### 2.4 permission_level 语义裁定（既有规则加草案补充，Phase 1 定稿）

`tool.permission_level` 的 T0/T1/T2 域已存在（`001_init.sql:289-290`），`tool_execution.permission_ctx` 注释明确「T0/T1/T2 + 授权引用」（`003_tools_workflow.sql:86`）。M6 已有成文映射规则（`docs/M6_REPORT.md:38`）：「readonly=true → T0（17 个），false → T1（rtis.py、rtis_kb（node 桥）、rtis_mcp.mjs）；T2 留高风险操作，本批未用」。本会话只读实测 platform.db：T1 恰为 rtis_kb（node 桥）/ rtis.py / rtis_mcp.mjs 三行，与 M6 报告一致。

本草案在 M6 规则基础上补齐边界语义：

- `T0`：无外部副作用。只读本地文件、只读平台库、纯计算。与 M6 的 readonly=true → T0 一致。示例：rip.doc.parse、三个 rip_* 脚本。
- `T1`：对外只读交互。发起网络请求但不改动目标状态（HTTP GET、DNS 查询、端口探测），或读写平台内部数据面但无外部目标副作用。示例：rip.http.inspect（GET 面）。
- `T2`：有外部写副作用。改动目标系统、发送消息。派发必须携带授权引用（authorization_ref），Tool Policy Resolver 校验缺失即拒绝（对应 D-TC-05 的 POLICY_DENIED）。

开放裁决项：M6 把 rtis.py（知识库唯一写入实现）归为 T1，本草案语义下「写平台内部库」仍算 T1、「改外部目标」才升 T2，与 M6 现状兼容；若 Phase 1 把写知识库也定为 T2，则 rtis.py、eri 写路径需要迁移等级并补授权引用链。**T1/T2 分界线（内部写是否升级）为草案提议，Phase 1 定稿前标注「待验证」。**

### 2.5 Manifest 校验规则（Registry 落库前检查清单）

1. 顶层必填字段齐全，`schema_version` 等于 Registry 支持的最高主版本（§6.1）。
2. `id` 不与既有工具冲突；`version` 与该 id 下既有版本可比（§6.2）。
3. `input_schema` / `output_schema` 本身是合法 JSON Schema draft 2020-12 文档。
4. `runtime.type` 在词表内；`remote_runner` 拒绝；`podman_vm` / `cli_adapter` 的 image 必须 digest 固定，tag 引用拒绝。
5. `provenance.license` 非 `unknown`；`image_digest` 与 `runtime.image` 一致。
6. `resources.timeout_seconds` 与 runtime 类型上限不冲突（python_worker 换算 ms 后仍在 [1, 600000]）。
7. `secrets[].name` 在平台 secrets 存储中存在（required=true 时强校验，required=false 宽告警）。
8. `examples` 存在时逐条校验 input 合 input_schema、output_hint 合 output_schema。
9. 落库写 `tool` + `tool_version`（provenance_json 存 provenance，content_hash 存 Manifest 规范化 JSON 的 sha256，validated_at 记校验时间），沿用两表现有列，不新增列（`001_init.sql:278-315`）。

---

## 3. ToolRun 生命周期状态机

### 3.1 状态清单

| 状态 | 性质 | 进入条件 | 写入方 |
|---|---|---|---|
| `QUEUED` | 初始 | 调用请求通过提交校验（D-TC-05），ToolRun 行落库 | Tool Scheduler |
| `WAITING_FOR_RUNNER` | 非终态 | 目标 runtime node 离线或不可达；「VM 离线时任务进入 WAITING_FOR_RUNNER，不伪装成工具失败」（计划 §5.6:336-337） | Tool Runner |
| `PREPARING` | 非终态 | Runner 已分配：解析 Artifact 引用到任务目录、拉取/确认镜像 digest、注入 secrets | Tool Runner |
| `RUNNING` | 非终态 | 执行体已启动：python_worker 已完成 initialize 且 execute 已送达；容器已启动 | Tool Runner / Runtime Adapter |
| `SUCCEEDED` | 终态 | 结果信封通过 output_schema 校验并交付 | Tool Runner |
| `FAILED` | 终态 | 任务级错误（EXECUTION/INPUT/VALIDATION/POLICY_DENIED/DEPENDENCY 等，见 §5.3），进程级失败同样入此态并带 error_category | Tool Runner |
| `TIMED_OUT` | 终态 | 超过 resources.timeout_seconds（python_worker 侧对应 status=timeout，`README.md:53`） | Tool Runner |
| `CANCELLED` | 终态 | 协作式取消生效且任务终止（python_worker 侧对应 error.code=-32004，`README.md:85`） | Tool Runner |

QUEUED → PREPARING → RUNNING → SUCCEEDED / FAILED / TIMED_OUT / CANCELLED 与计划 §4.5:225 完全一致；WAITING_FOR_RUNNER 是计划 §5.6 明文要求的状态，插入在 QUEUED 与 PREPARING 之间（也可从 PREPARING 因镜像拉取失败重入）。

### 3.2 跃迁表

| 从 | 到 | 触发 | 备注 |
|---|---|---|---|
| （无） | QUEUED | 提交校验通过，run 落库 | 校验失败同步拒绝，不产生 run |
| QUEUED | PREPARING | 调度器选出可用 runner | |
| QUEUED | WAITING_FOR_RUNNER | 无可用 runner（VM 离线） | 幂等提交键（submit_key）保留，重连后继续 |
| WAITING_FOR_RUNNER | PREPARING | 心跳恢复，runner 可用 | 重连后查询 VM 已完成/在跑任务（计划 §5.6:338） |
| WAITING_FOR_RUNNER | CANCELLED | 等待期内被取消 | |
| PREPARING | RUNNING | 执行体启动成功 | |
| PREPARING | FAILED | 镜像缺失/校验不一致/授权缺失（POLICY_DENIED）/输入解析失败（INPUT） | image digest 不符是硬失败，不静默换镜像 |
| PREPARING | WAITING_FOR_RUNNER | 拉取镜像时 node 失联 | |
| RUNNING | SUCCEEDED | status=result 且 output_schema 校验通过 | |
| RUNNING | FAILED | status=error / 容器非零退出 / 结构化结果校验失败 | 保留原始输出为 Artifact |
| RUNNING | TIMED_OUT | status=timeout / 平台侧超时兜底 | |
| RUNNING | CANCELLED | cancel 生效且任务终止（-32004） | 取消请求时刻记 cancel_requested_at（D-TC-04） |
| RUNNING | SUCCEEDED | 取消请求后任务仍自行完成并交付 | 协作式取消语义，`README.md:85`「handler 无视令牌时任务可能正常完成并交付结果」 |
| 终态 | （无） | 终态不可迁出 | 修正需新 run（幂等键不同） |

### 3.3 与现有数据面的映射

- ToolRun 是 `tool_execution`（`003_tools_workflow.sql:79-91`）的超集演进：exec_id 对应 run_id；argv_hash 对应「规范化 input 的 sha256」（对 input 中敏感值脱敏后计算，D-TC-06）；exit_code 对应容器退出码或 worker error.code；duration_ms 对应 usage.elapsed_ms；started_at 对应 RUNNING 进入时刻；permission_ctx 存 `permission_level + authorization_ref`。Phase 1 新建 `tool_run` 表承载全量字段（run_id/task_id/tool_id/input_ref/status/timestamps，计划 §12:697），`tool_execution` 保持不动（0 行空结构，红线不写库），Phase 1 出迁移对账清单后决定合并方式。
- 每次状态跃迁发一条 audit-engine 事件（correlation_id = run_id），同时补齐「Python/rtis 侧审计 JSONL 分叉」的收敛入口（架构盘点审计补充第 3 条，`CURRENT-ARCHITECTURE-INVENTORY.md:62`；差距清单第 5 条，同文件 220 行）。
- WAITING_FOR_RUNNER 的判定依赖 RuntimeNode.health（计划 §12:696），VM 心跳机制属 Phase 2 Tool Gateway 范围（计划 §14 Phase 2:763-772）；Phase 1 的 Mock Runner 用配置开关模拟该状态。

---

## 4. 统一调用协议对象（ToolCallRequest）

### 4.1 字段表

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `protocol_version` | string | 是 | 调用协议主版本，当前 `"1"` |
| `tool_id` | string | 是 | Manifest id |
| `tool_version` | string | 否 | 缺省用 Registry current 版本；重放/固定版本场景必填 |
| `run_id` | string | 是 | 服务端生成，全局唯一，审计链 correlation_id |
| `submit_key` | string | 否 | 调用方幂等提交键（计划 §5.6:337「唯一 Run ID 和幂等提交键」）；python_worker runtime 直通 `idempotency_key`（`README.md:74-79`） |
| `input` | object | 是 | 已通过 Manifest input_schema 校验的参数；文件内容用 `input_refs` 引用，不内联大对象 |
| `input_refs` | array | 否 | `[{artifact_id, mount_as}]`，由 Artifact Manager 暂存到任务目录（计划 §5.5:324-329 的受控文件交换路径） |
| `workspace_id` | string | 是 | 工作区隔离边界 |
| `task_id` | string | 否 | 关联研究任务（审计链用） |
| `context_refs` | array | 否 | 本次调用使用的知识/证据引用（计划 §4.5:219） |
| `budget` | object | 否 | `timeout_seconds` / `max_output_bytes`，缺省取 Manifest resources；显式值不得超过 Manifest 上限（超出按 VALIDATION 拒绝） |
| `execution_profile` | string | 否 | 命名执行配置（network/资源覆盖由 Tool Policy Resolver 裁定，计划 §6:356），Phase 1 仅保留字段 |
| `result_schema_version` | string | 否 | 期望的结果信封版本，默认 `"1"`（计划 §4.5:221） |
| `authorization_ref` | string | permission_level=T2 时必填 | 授权记录引用，Tool Policy Resolver 校验 |

注意：调用对象中**没有** secrets 字段（D-TC-06）；没有宿主机路径字段（文件一律经 input_refs，计划 §5.5:331「不向 Agent 直接开放 shell 路径」）。

### 4.2 python_worker runtime 的映射

ToolCallRequest 到 Worker Protocol v1 execute 的映射（全部有协议依据）：

| ToolCallRequest | execute.params | 出处 |
|---|---|---|
| `input` | `task` | v1.json executeParams（`v1.json:147-163`） |
| `submit_key` | `idempotency_key`（长度 1..512） | `v1.json:156-161` |
| `budget.timeout_seconds` × 1000 | `budget.timeout_ms`（域 [1, 600000]） | `v1.json:28-33` |
| `budget.max_output_bytes` | `budget.max_output_bytes`（域 [256, 67108864]） | `v1.json:34-39` |
| `run_id` | 不进 task；由宿主派发器记录在 stderr 日志上下文与 audit 事件 | 保持 task 与协议纯净 |

宿主派发器职责（Phase 1 新建，对应架构盘点差距第 4 条「Worker 宿主派发器缺失」，`CURRENT-ARCHITECTURE-INVENTORY.md:219`）：维护 worker 常驻进程池（restart=pooled）、握手（initialize + get_capabilities，`README.md:23-43`）、execute 派发、cancel 转发、stderr 结构化日志采集（`README.md:114`）。

### 4.3 podman_vm runtime 的映射

容器以「stdin 一行 JSON 请求，stdout 一行 JSON 结果」为默认帧协议（D-TC-03）；ToolCallRequest 的 `input` 序列化为请求行，容器 `entrypoint` 读入并输出结果行；非零退出码映射 FAILED。`env` 模板变量在 PREPARING 阶段渲染（`{{run_id}}`、`{{task_dir}}`）。HTTP 帧（http_api runtime）由 Runtime Adapter 翻译为 HTTP 请求/响应，Phase 1 只定义翻译责任，不定义具体模板语法（待 Phase 3 首个 http_api 工具落地时定稿）。

---

## 5. 输出约定（ToolRunResult 信封）

### 5.1 信封字段

「机器可读 JSON 和人类可读摘要」双轨是计划 §4.5:229 的明文要求；「原始 stdout/stderr 可以保存为 Artifact，但必须额外生成结构化结果」见同文件 237 行。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `protocol_version` | string | 是 | `"1"` |
| `run_id` / `tool_id` / `tool_version` | string | 是 | 与调用一致；tool_version 记录实际执行版本（重放依据） |
| `status` | string | 是 | `SUCCEEDED` / `FAILED` / `TIMED_OUT` / `CANCELLED`（与 §3 状态机终态同名） |
| `result` | object | SUCCEEDED 时必填 | 工具结构化输出，Registry 按 Manifest output_schema 校验后入库 |
| `summary` | string | SUCCEEDED/FAILED 时必填 | 人类可读一段话摘要（Tool Center 结果预览用，计划 §4.6:245） |
| `error` | object | 非成功时必填 | `{category, code, message, retryable, detail_ref}`，见 §5.3 |
| `artifacts` | array | 否 | 见 §5.2 |
| `evidence_refs` | array | 否 | 证据记录引用（计划 §4.5:234「Evidence 引用」） |
| `usage` | object | 是 | `{elapsed_ms, output_bytes, truncated, cpu_ms, max_rss_bytes}`；后两项 podman/worker 可得则填，否则 null。elapsed_ms/output_bytes/truncated 与 Worker Protocol usage 语义一致（`v1.json:42-66`） |
| `environment` | object | 是 | 「版本与环境指纹」（计划 §4.5:232）：`{runtime_type, runtime_node_id, image_digest, worker_protocol_version, host_fingerprint_hash}`，按 runtime 类型取用 |
| `replay` | object | 是 | `{input_sanitized, submit_key}`：脱敏后输入与幂等键，可重放（计划 §4.5:235） |

python_worker runtime 的信封生成：execute 响应（`v1.json:164-186` 的 executeResult）由宿主派发器包装为信封；`status=result` → SUCCEEDED 并把 `data` 平移进 `result`（若 data 是截断信封 `truncated:true`，则 result 降级为截断说明 + 原始字节 Artifact 引用）；`status=error` → FAILED 并映射 error.code；`status=timeout` → TIMED_OUT；`cached=true` 透传进 `replay.cached` 并照常入库（幂等命中也是一次合法运行记录）。

### 5.2 Artifact 规范

对齐计划 §12:698（artifact_id、run_id、uri、hash、mime、size、provenance）：

| 字段 | 说明 |
|---|---|
| `artifact_id` | 全局唯一，服务端登记时生成 |
| `role` | `primary`（主输出文件）/ `attachment` / `stdout` / `stderr` / `log` |
| `uri` | Artifact Manager 存储地址（任务结束后由暂存区归档，计划 §5.4:315「任务结束后清理临时容器与目录；需要保留的结果先归档」） |
| `sha256` / `size_bytes` / `mime_type` | 收集时计算登记（计划 §5.5:328「收集输出，计算哈希并登记 Artifact」） |
| `provenance` | 生成方式描述（工具输出 / 平台采集） |

stdout/stderr 约定：结构化协议行以外的输出（容器噪声、worker stderr JSON 日志）一律存为 `role=stdout/stderr` 的 Artifact，不进入 `result`；「stdout 不承载日志」的 worker 纪律（`README.md:8`、TOOL-INVENTORY.md:22）由派发器在容器侧维持同样分轨。

### 5.3 标准错误类别

| category | 含义 | 对应 Worker Protocol 错误码（`README.md:88-110`） | retryable 默认 |
|---|---|---|---|
| `VALIDATION` | 参数/预算/提交校验失败 | -32602 INVALID_PARAMS、-32003 BUDGET_INVALID | false |
| `INPUT` | 输入不可读/超大/格式不支持 | -32008 INPUT_ERROR、-32009 UNSUPPORTED_FORMAT | false |
| `TIMEOUT` | 预算超时 | -32002 BUDGET_TIMEOUT | 视工具 |
| `CANCELLED` | 被取消 | -32004 CANCELLED | false |
| `EXECUTION` | 工具内部执行失败 | -32006 EXECUTION_ERROR、任务级 -32603 | 视工具 |
| `POLICY_DENIED` | 授权缺失/网络策略拒绝（平台侧） | 无对应码，平台侧产生 | false |
| `DEPENDENCY` | 外部依赖不可达（上游服务、模型文件缺失） | 平台侧归类 | true |
| `INFRA` | 平台/协议层错误（握手失败、版本不匹配、信封损坏） | -32700/-32600/-32601/-32000/-32001/-32005/-32007 | true |
| `RUNNER_UNAVAILABLE` | runner 失联（仅 PREPARING/WAITING 期失败时使用） | 平台侧产生 | true |

映射原则：Worker Protocol 错误码一一对应落入上表（可追溯到 `README.md:88-110` 码表）；容器与 http runtime 的平台侧错误由 Runtime Adapter 归类，归类规则属于该 Adapter 的契约测试范围。

### 5.4 截断与预算对齐

- 结构化结果超过 `max_output_bytes` 时：python_worker 侧由运行时截断信封兜底（`README.md:62-68`）；容器侧由派发器按同样策略生成截断信封（result 替换 + 原始输出落 Artifact，`usage.truncated=true`）。两个 runtime 的行为必须一致，Mock Runner 契约测试覆盖（§10.2）。
- 大文件输出（如抓取的页面体）不应硬塞进 result：工具应在 result 中给引用字段（如 `body_artifact_id`），实体内容走 Artifact（示例 §9 演示此模式，与计划 §4.4:200 的 `body_artifact_id` 字段一致）。

---

## 6. 版本与兼容策略

### 6.1 Manifest schema_version 策略

沿用 Worker Protocol v1 的版本策略（`README.md:120-123`「只加字段不加语义 = 主版本不变；改既有字段语义 = 升主版本并拒绝旧客户端」）：

- 当前 `"1"`。新增**可选**顶层字段不升版本；修改既有字段语义、删除字段、收窄域 → 升 `"2"`，Registry 拒绝高于自身支持主版本的 Manifest 并给升级指引。
- `input_schema` / `output_schema` 内部演进不升 Manifest 主版本（它们是工具自有契约，随工具版本走）。

### 6.2 工具版本策略

- `version` 语义化版本：MAJOR 表示 input/output 语义破坏性变更；MINOR 表示向后兼容的增量能力；PATCH 表示修复。Registry 按 semver 比较拒绝回退登记。
- 落库复用 `tool_version` 表：UNIQUE(tool_id, version) 天然防重（`001_init.sql:314`）；`status` 只有 `current` / `superseded` 两值（`001_init.sql:310`），新版本登记时旧 current 置 superseded，单工具同时至多一个 current。
- 升级即失效幂等缓存：Worker Protocol 的幂等指纹含 worker_version（`README.md:78`、`v1.json:157`），与「升级天然失效旧缓存」一致；平台级幂等指纹同样纳入 tool_version。

### 6.3 兼容性检查与回滚

1. **升级前契约测试**：新版本登记时，Registry 用旧版本 `examples` 逐条跑新版本（Mock Runner 或真实 runtime），输出不合旧 output_schema 即判定 MAJOR 级变更，与声明版本比对，不符则拒绝登记。该检查 Phase 1 以 Mock Runner 实现，真实 runtime 契约测试属 Phase 2 容器 Runner。
2. **双版本共存**：current 之外保留最近一个 superseded 版本可执行（重放历史 run 时按记录的 tool_version 精确取用）；更早版本只留记录不可执行。
3. **镜像回滚**：`runtime.image` 一律 digest 固定（§2.5 第 4 条），回滚 = 登记回旧 digest 的新 ToolInstallation；「镜像固定版本和 digest；更新先验证再切换」（计划 §5.4:314）。
4. **弃用**：`tool.lifecycle` 置 `DEPRECATED`（`001_init.sql:282-284` 五态域），Tool Center 展示（计划 §4.6:247「固定版本、更新、卸载」），DEPRECATED 工具可运行但新 run 带告警事件。
5. **淘汰**：governance 轴置 REJECTED/ARCHIVED 后对调度不可见，与 resource 治理同构（`CURRENT-ARCHITECTURE-INVENTORY.md:67` 只读边界同款语义）。

---

## 7. 健康检查

### 7.1 状态域与归属

- 工具级健康状态域沿用 `tool_version.health`：`unknown` / `ok` / `degraded` / `broken`（D-TC-07，`003_tools_workflow.sql:26-29`；现网实测取值 ok/degraded/unknown，broken 域内但未出现）。M6 已有成文判定先例（`docs/M6_REPORT.md:42-43`）：白名单只读探测（--version / -h / node --check / sqlite quick_check，timeout 5s，不执行扫描），「探测执行且版本可解析 → ok；二进制可执行但版本不可得 → degraded；文件缺失 → unknown」，本节探针设计是该先例向各 runtime 的推广，degraded/unknown 判定语义保持一致。
- 节点级健康属 RuntimeNode.health（计划 §12:696），由 Tool Gateway 心跳维护（计划 §5.6:335），Phase 2 范围。
- Tool Health Monitor 是 §6 模块边界的独立组件（计划 §6:355「工具、镜像和 Runner 健康检查」），Phase 1 以最小实现落地：只探工具级状态 + 定时写 tool_version.health。

### 7.2 Manifest health 字段

| 字段 | 说明 |
|---|---|
| `probe.type` | `worker_ping`（python_worker）/ `exec`（podman_vm、cli_adapter）/ `http_get`（http_api）/ `none`（不探针，保持 unknown 直到首次运行） |
| `probe.command` | type=exec 时必填，容器内自检命令，退出 0 为 ok |
| `probe.endpoint` | type=http_get 时必填，相对 base_url 的路径，期望 2xx |
| `probe.timeout_ms` | 单次探针超时，默认 5000 |
| `probe.interval_seconds` | 探测周期，默认 600 |
| `probe.degraded_after` | 连续失败多少次转 degraded，默认 1 |
| `probe.broken_after` | 连续失败多少次转 broken，默认 3 |
| `on_run_recover` | 布尔，默认 true：一次成功运行同样把 degraded/broken 拉回 ok |

探针结果本身不是 ToolRun：只更新 tool_version.health 与一条 JSONL 健康事件（Phase 1 不建健康专用表，健康历史经审计事件可追溯）。

### 7.3 各 runtime 的探针语义

| runtime.type | 探针 | 依据 |
|---|---|---|
| `python_worker` | `worker_ping`：对进程池中的 worker 发协议 ping，响应 `{pong:true, worker_version, uptime_ms, active_tasks}` 为 ok | ping 方法与 pingResult 形状（`README.md:21`、`v1.json:208-216`） |
| `podman_vm` | `exec`：在镜像内跑自检命令（不访问网络，不写任务目录） | 容器可执行性即健康的最小定义；镜像 digest 已在 PREPARING 强校验 |
| `http_api` | `http_get` 探针端点 | 现有 Web 工作台无 /api/healthz 的教训（grep 实测无命中，`CURRENT-ARCHITECTURE-INVENTORY.md:110`）：新登记的 http 工具必须显式声明探针端点，缺省视为 `none` 并在 Tool Center 标 unknown |
| `native_rust` / `mcp_client` / `cli_adapter` | exec（二进制 --version 或 MCP initialize/ping） | 最小可执行性 |
| `remote_runner` | 不适用（Phase 1 不开放） | D-TC 词表裁定 |

---

## 8. 示例 Manifest 一：python_worker 包装现有 parsing worker

### 8.1 Manifest 全文（rip.doc.parse）

~~~yaml
schema_version: "1"
id: "rip.doc.parse"
name: "Document Parser"
version: "0.1.0"
kind: "tool"
family: "document"
description: "解析 md/txt/html 文本为章节结构、标题树与词元统计，包装现有 Worker Protocol v1 worker"
runtime:
  type: "python_worker"
  protocol_version: "1"
  worker_name: "parsing-worker"
  command: ["python3", "-I", "workers/python/parsing_worker.py"]
  working_dir: "repo://research-platform"
  restart: "pooled"
input_schema:
  type: "object"
  additionalProperties: false
  oneOf:
    - required: ["text"]
    - required: ["path"]
  properties:
    text:
      type: "string"
      maxLength: 10485760
      description: "内联文本，UTF-8 字节上限 10 MiB，与 path 二选一"
    path:
      type: "string"
      description: "输入文件，Artifact 暂存到任务目录后的相对路径，与 text 二选一"
    format:
      type: "string"
      enum: ["md", "txt", "html"]
      description: "缺省时 text 按 txt，path 按扩展名推断"
output_schema:
  type: "object"
  required: ["format", "source", "sections", "heading_tree", "stats"]
  properties:
    format:
      type: "string"
      enum: ["md", "txt", "html"]
    source:
      type: "object"
      description: "输入来源：kind(inline|file)、path、bytes、sha256"
    sections:
      type: "array"
      items:
        type: "object"
      description: "章节列表，md 带行号区间，html 行号为 null"
    heading_tree:
      type: "array"
      items:
        type: "object"
      description: "标题嵌套树"
    stats:
      type: "object"
      description: "chars/lines/words/unique_words/cjk_chars/ascii_words/tokens_est/sections/headings"
resources:
  cpu: "1"
  memory: "256Mi"
  timeout_seconds: 60
  max_output_bytes: 1048576
network:
  mode: "none"
secrets: []
permission_level: "T0"
idempotent: true
health:
  probe:
    type: "worker_ping"
    timeout_ms: 2000
    interval_seconds: 300
    degraded_after: 1
    broken_after: 3
  on_run_recover: true
provenance:
  origin: "rip-native"
  source_path: "workers/python/parsing_worker.py"
  license: "repo-internal"
  dependencies: []
  known_limits:
    - "输入硬上限 10 MiB"
    - "html 行号不可用（剥标签后无行映射）"
  tested: "workers/python/tests 125 项全绿（TOOL-INVENTORY §1.5，2026-11-09 实跑）"
context:
  summary: "文档解析：md/txt/html 转章节结构与统计"
  docs: "workers/python/parsing_worker.py 模块头注"
  tags: ["parse", "document", "markdown", "html"]
examples:
  - input:
      text: "# 标题\n\n正文一段。"
      format: "md"
    output_hint:
      format: "md"
      stats:
        sections: 1
~~~

### 8.2 字段取值依据（逐条出处）

| 取值 | 依据 |
|---|---|
| `worker_name: "parsing-worker"`、`version: "0.1.0"` | `parsing_worker.py:224-225`（NAME/VERSION） |
| `command` 用 `-I` 隔离模式 | 红线「不在宿主机安装依赖」的既有执行纪律（TOOL-INVENTORY.md:6 全部验证均用 `-I`） |
| input_schema 的 text/path 二选一与 format 枚举 | `parsing_worker.py:5-7`（模块头注）、`parsing_worker.py:274-275`（恰好其一校验）、`parsing_worker.py:29`（SUPPORTED_FORMATS） |
| `maxLength: 10485760` | `parsing_worker.py:28`（MAX_INPUT_BYTES = 10 MiB，inline 与 file 双路径都检查） |
| output_schema 五字段 | `parsing_worker.py:259-265`（execute 返回对象） |
| md 带行号、html 行号 null | `parsing_worker.py:245-246`（with_lines=(fmt != "html")） |
| `network.mode: "none"` | worker 纯标准库无网络调用（TOOL-INVENTORY.md:32） |
| `permission_level: "T0"` | 只读解析，无副作用（§2.4 裁定） |
| `idempotent: true` | Worker Protocol idempotency_key 支持（`README.md:74-79`）+ 解析确定性 |
| `restart: "pooled"` | worker 为 stdio 常驻服务，支持同进程串行多请求（TOOL-INVENTORY.md:36「stdio 常驻服务」） |
| `resources.timeout_seconds: 60` | 解析 10 MiB 上限内为秒级任务；60s < 协议硬上限 600000ms（`worker_runtime.py:35`） |
| `tested` | TOOL-INVENTORY.md:69（125 项测试全绿，2026-11-09 实跑） |

### 8.3 调用与输出示例

调用（ToolCallRequest 摘要）：

~~~json
{
  "protocol_version": "1",
  "tool_id": "rip.doc.parse",
  "run_id": "rn-01HTXYZEXAMPLE",
  "submit_key": "task-42-doc-a",
  "input": { "text": "# 标题\n\n正文一段。", "format": "md" },
  "workspace_id": "ws-default",
  "budget": { "timeout_seconds": 60, "max_output_bytes": 1048576 },
  "result_schema_version": "1"
}
~~~

宿主派发器翻译后的 execute.params（Worker Protocol v1）：

~~~json
{
  "task": { "text": "# 标题\n\n正文一段。", "format": "md" },
  "budget": { "timeout_ms": 60000, "max_output_bytes": 1048576 },
  "idempotency_key": "task-42-doc-a"
}
~~~

返回信封（SUCCEEDED 时的骨架，result 内容来自 `parsing_worker.py:259-265` 的真实输出形状）：

~~~json
{
  "protocol_version": "1",
  "run_id": "rn-01HTXYZEXAMPLE",
  "tool_id": "rip.doc.parse",
  "tool_version": "0.1.0",
  "status": "SUCCEEDED",
  "result": {
    "format": "md",
    "source": { "kind": "inline", "path": null, "bytes": 30, "sha256": "..." },
    "sections": [],
    "heading_tree": [],
    "stats": { "chars": 30, "sections": 1, "headings": 1 }
  },
  "summary": "解析 md 30 字节，1 个章节 1 个标题",
  "usage": { "elapsed_ms": 12, "output_bytes": 512, "truncated": false, "cpu_ms": null, "max_rss_bytes": null },
  "environment": {
    "runtime_type": "python_worker",
    "runtime_node_id": "host-cachyos",
    "image_digest": null,
    "worker_protocol_version": "1",
    "host_fingerprint_hash": "..."
  },
  "replay": { "input_sanitized": { "text": "...", "format": "md" }, "submit_key": "task-42-doc-a" }
}
~~~

---

## 9. 示例 Manifest 二：容器化 HTTP 工具

### 9.1 Manifest 全文（rip.http.inspect）

~~~yaml
schema_version: "1"
id: "rip.http.inspect"
name: "HTTP Inspector"
version: "1.0.0"
kind: "tool"
family: "web_http"
description: "在工具 VM 容器内发起 HTTP 请求并提取结构化响应信息"
runtime:
  type: "podman_vm"
  image: "registry.local/rip/http-inspector@sha256:<digest>"
  entrypoint: ["/opt/rip/bin/http-inspector"]
  user: "1000:1000"
  readonly_rootfs: true
  mounts:
    - { role: "input", path: "/task/in", mode: "ro" }
    - { role: "output", path: "/task/out", mode: "rw" }
  env:
    RIP_RUN_ID: "{{run_id}}"
    RIP_TASK_DIR: "{{task_dir}}"
input_schema:
  type: "object"
  required: ["url"]
  additionalProperties: false
  properties:
    url:
      type: "string"
      format: "uri"
    method:
      type: "string"
      enum: ["GET", "HEAD", "POST"]
      default: "GET"
    headers:
      type: "object"
      additionalProperties:
        type: "string"
      description: "附加请求头，Authorization 类头部必须走 secrets，禁止明文传入"
    timeout_seconds:
      type: "integer"
      minimum: 1
      maximum: 60
      default: 15
    save_body:
      type: "boolean"
      default: true
      description: "响应体超过 1 MiB 时落为 Artifact，result 内只留 body_artifact_id"
output_schema:
  type: "object"
  required: ["status_code", "final_url"]
  properties:
    status_code:
      type: "integer"
    final_url:
      type: "string"
    headers:
      type: "object"
    body_bytes:
      type: "integer"
    body_artifact_id:
      type: "string"
      description: "响应体 Artifact 引用（role=primary）"
resources:
  cpu: "1"
  memory: "512Mi"
  pid_limit: 64
  disk_mb: 256
  timeout_seconds: 90
  max_output_bytes: 1048576
network:
  mode: "egress_allowlist"
  egress:
    ports: [80, 443]
    domains: []
    note: "目标 URL host 由 Tool Policy Resolver 在派发时加入临时 allowlist，运行结束失效"
secrets: []
permission_level: "T1"
idempotent: false
health:
  probe:
    type: "exec"
    command: ["/opt/rip/bin/http-inspector", "--selfcheck"]
    timeout_ms: 5000
    interval_seconds: 600
    degraded_after: 1
    broken_after: 3
  on_run_recover: true
provenance:
  origin: "rip-native"
  source_repo: "rip tool pack: web-research（Phase 3 交付物）"
  license: "repo-internal"
  dependencies: []
  image_digest: "sha256:<digest>"
  known_limits:
    - "仅 80/443 出站；非 HTTP(S) 目标不适用"
  tested: "待验证（镜像与工具本体属 Phase 2/3 交付，本文档仅示范契约形状）"
context:
  summary: "HTTP 请求与响应检查工具"
  docs: "docs/usage.md"
  tags: ["http", "web", "recon"]
examples:
  - input:
      url: "https://example.com/"
      method: "GET"
    output_hint:
      status_code: 200
      final_url: "https://example.com/"
~~~

### 9.2 字段取值依据与差异说明

| 取值 | 依据 |
|---|---|
| id/name/version/kind/input/output 骨架、resources.cpu/memory/timeout_seconds、context | 计划 §4.4 示例原文（同文件 176-208 行），本草案在其上补齐 family/network/secrets/health/provenance/permission/idempotent/examples 七块 |
| `image` digest 占位 | §4.4:185 原文形态；`<digest>` 是占位符，真实镜像属 Phase 2（基础镜像）/ Phase 3（Web Research Pack）交付，**登记前必须替换为真实 64 位 digest，现标注待验证** |
| `user` 非 root、`readonly_rootfs`、mounts 限任务目录 | 计划 §5.4:302-309（rootless、只读根文件系统、只挂当前任务所需目录、不挂宿主机家目录与 Socket） |
| `network.mode: "egress_allowlist"` + 动态目标说明 | 计划 §5.4:313「网络访问由执行配置决定」；HTTP 工具的目标天然动态，静态 domains 留空、由 Policy Resolver 出临时条目 |
| `secrets: []` + headers 描述中禁止明文 Authorization | D-TC-06；需要认证的目标登记时应加 secrets 条目（name + inject_as=env）并让工具从环境读取 |
| `permission_level: "T1"` | 网络只读（§2.4 裁定：GET/HEAD/POST 对目标有潜在写效果，POST 场景登记方应改 T2 并要求 authorization_ref；本示例按只读面登记 T1，POST 属演示枚举） |
| `idempotent: false` | 远端资源可能变化，同输入不保证同输出；重放仅作审计参考 |
| `health.probe.type: "exec"` | §7.3 podman_vm 探针语义 |
| `save_body` + `body_artifact_id` 模式 | §5.4 大文件输出走 Artifact 约定；与 §4.4:200 `body_artifact_id` 字段一致 |

### 9.3 调用流程（对照 §3 状态机）

1. QUEUED：提交携带 `input: {url: "https://example.com/"}`，Policy Resolver 校验 T1 无需授权引用。
2. PREPARING：Runner 校验 `registry.local/rip/http-inspector@sha256:<digest>` 在 VM 缓存中（缺失则拉取并复核 digest）；创建任务目录，挂载 `/task/in`(ro) 与 `/task/out`(rw)；渲染 env 模板。
3. RUNNING：容器启动，stdin 收到请求行；出站仅 80/443 且目标 host 在临时 allowlist。
4. 终态：stdout 结果行经 output_schema 校验后入库；`/task/out` 内响应体文件收集登记为 Artifact（sha256/size/mime）；容器与任务目录清理，保留项先归档（计划 §5.4:315）。

---

## 10. Phase 1 实施指引

### 10.1 落地物清单

Phase 1 范围按计划 §14:753-761（定义 Manifest 与 JSON Schema；定义 ToolDefinition/ToolPack/ToolRun/Artifact 基础模型；建立发现、查询、Schema 获取与健康状态接口；登记现有 Python Workers 与脚本；Mock Runner 验证调用协议）。对应落地物：

| 落地物 | 位置建议 | 内容 |
|---|---|---|
| Manifest JSON Schema | `schemas/tool-manifest/v1.json` | 以本文档附录 A 为起点，draft 2020-12，与 worker-protocol/v1.json 同目录惯例 |
| 调用协议 Schema | `schemas/tool-contract/request-v1.json`、`result-v1.json` | §4.1 与 §5.1 字段表机械化 |
| tool-registry crate | `crates/tool-registry` | tool/tool_version 读写面、Manifest 校验（§2.5 九条）、发现/查询/Schema 获取接口；依赖方向 database + domain（模块边界纪律，`CURRENT-ARCHITECTURE-INVENTORY.md:41`） |
| tool-engine crate（骨架） | `crates/tool-engine` | ToolCallRequest 受理、状态机（§3.2）、ToolRunResult 信封（§5）、Mock Runner（内存假 runtime，可注入 succeed/fail/timeout/cancel 四路径） |
| worker 宿主派发器 | tool-engine 内 python_worker Runtime Adapter | 差距清单第 4 条的修补（`CURRENT-ARCHITECTURE-INVENTORY.md:219`）：进程池 + 握手 + execute/cancel + stderr 日志采集 |
| domain 扩展 | `crates/domain` | ToolRunStatus 枚举、ArtifactId/RunId 新类型；2.0 新实体唯一合法落点（`CURRENT-ARCHITECTURE-INVENTORY.md:48`） |
| 迁移 SQL | `migrations/005_tool_contract.sql` | tool_run + artifact 表，幂等 DDL + schema_version 登记；**本 Phase 0 不写任何 migrations/platform*.db，DDL 落地属 Phase 1 执行项** |
| 契约测试 | `crates/tool-engine/tests/` 或 tests/ 载具 | 状态机全路径、信封形状、截断一致性（§5.4）、Manifest 校验九条 |

禁止项（Phase 1 明确不做，对齐计划 §15:872「不提前实现后续阶段功能」）：不建 Tool VM、不写真实 podman Runner（Phase 2）、不做 Tool Center 页面（Phase 5）、不开放 remote_runner。

### 10.2 首批登记批次（映射 TOOL-INVENTORY 总表）

| 批次 | 工具（TOOL-INVENTORY 编号） | 登记方式 | 前置条件 |
|---|---|---|---|
| B1 | rip.doc.parse（T02）、rip.text.embed（T03）、rip.kb.evaluate（T04） | python_worker Manifest 直登（§8 模板） | 派发器就绪；T03 的模型许可证字段待补（TOOL-INVENTORY.md:50 待验证项 6） |
| B2 | rip.kb.keyword_search（T05）、rip.kb.inventory（T06）、rip.ops.etl_status（T07） | cli_adapter 或 native_rust 薄包装，输出需结构化 Adapter（TOOL-INVENTORY.md:87「可再包一层结构化输出 Adapter」） | 三个脚本已实跑验证（TOOL-INVENTORY.md:89, 94, 99） |
| B3 | rip.ops.worker_runtime（T01，协议底座，不单独暴露为可调用工具，登记为 runtime 资产） | Registry 元数据条目 | 无 |
| B4 | rip.evidence.rtk（T08：auth/ledger/lint/mcp/scaffold 五子命令拆五个 Manifest 或一 Manifest 多 op） | cli_adapter | toolkit pytest 套件补跑（宿主无 pytest，TOOL-INVENTORY.md:118 待验证项 1，用临时 venv） |
| B5 | rip.kb.rtis（T09）、rip.kb.eri（T10） | Adapter Manifest 指向现有入口（rt.py / rtis_kb.mjs / eri.py），**不改本体**（红线，TOOL-INVENTORY.md:161） | rt.py 与 rtis_kb.mjs 只读冒烟已过（TOOL-INVENTORY.md:164-165）；rtis.py 编排层不登记（涉生产写路径） |
| 缓登 | T11（与 rtk.mcp 合并）、T12（TPscan 许可证未核） | 按 TOOL-INVENTORY 复用建议执行 | 待验证项 3、5 |

Mock Runner 契约测试最小集（Phase 1 验收「实现 Mock Runner 验证调用协议」，计划 §14:759）：SUCCEEDED/FAILED/TIMED_OUT/CANCELLED 四路径 + WAITING_FOR_RUNNER 注入 + 截断信封一致性 + 幂等命中 + Manifest 校验九条 + §8.3/§9.3 两示例的往返。

### 10.3 验收对照（计划 §14 Phase 1:761）

| 验收句 | 本草案支撑 |
|---|---|
| 「工具可发现、查看 Schema、验证参数」 | §2 字段规范 + §2.5 校验规则 + Registry 接口（§10.1） |
| 「不执行任意宿主机 Shell」 | runtime 词表无 shell 类型（D-TC-01）；native_rust 限已知二进制（§2.2）；计划 §6:370 禁令 |
| 「Registry 有版本和来源信息」 | version/provenance 字段（§2.1、§2.3）+ tool_version 表既有列（§6.2） |

---

## 11. 未验证项汇总

1. **rip.http.inspect 镜像与工具本体不存在**：本文档仅示范契约形状，`<digest>` 为占位符；真实交付属 Phase 2（基础镜像）与 Phase 3（Web Research Pack）。登记前必须完成计划 §4.3 三项验证（源码检查、最小功能验证、输出格式验证）。
2. **T1/T2 分界线为草案提议**：M6 已有成文规则 readonly=true → T0、false → T1、T2 预留（`docs/M6_REPORT.md:38`），本草案与其兼容；但「写平台内部库是否升 T2」未裁决（rtis.py 现网 T1，`docs/M6_REPORT.md:38`；只读实测一致），Phase 1 定稿前标注待验证。
3. **附录 A 的 Manifest JSON Schema 未做 jsonschema 库级校验**：宿主机与仓库 `.venv` 均无 jsonschema 模块（本会话实测 `python3 -I -c "import jsonschema"` 双双 ModuleNotFoundError），红线禁止安装；本次仅完成 JSON 语法级校验（json.loads 通过）与两示例 YAML 解析校验（pyyaml 6.0.3 实测通过，见附录 A 尾注）。Schema 语义正确性属 Phase 1 首个实现任务。
4. **tool_run/artifact 的 DDL 未编写**：属 Phase 1 执行项，本草案只定字段语义（§3.3、§5.2）。
5. **http_api runtime 的请求/响应模板语法未定**：待 Phase 3 首个 http_api 工具落地时定稿（§4.3）。
6. **WAITING_FOR_RUNNER 的心跳判定依赖 Phase 2 Tool Gateway**：Phase 1 以配置开关在 Mock Runner 模拟（§3.3）。

---

## 附录 A：Tool Manifest v1 JSON Schema 草案

以下为 §2 字段规范的机械化起点（draft 2020-12，与 `schemas/worker-protocol/v1.json` 同版式）。Phase 1 落地为 `schemas/tool-manifest/v1.json` 后以 jsonschema 校验器加测（§11 第 3 条）。

~~~json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "research-platform/schemas/tool-manifest/v1.json",
  "title": "RIP Tool Manifest v1",
  "description": "Tool registration manifest. Field semantics are normative in docs/phase0/TOOL-CONTRACT-DRAFT.md section 2. Draft for Phase 1; jsonschema-level validation pending.",
  "type": "object",
  "required": [
    "schema_version", "id", "name", "version", "kind", "family",
    "description", "runtime", "input_schema", "output_schema",
    "resources", "network", "secrets", "permission_level",
    "idempotent", "health", "provenance"
  ],
  "additionalProperties": false,
  "properties": {
    "schema_version": { "type": "string", "const": "1" },
    "id": {
      "type": "string",
      "pattern": "^[a-z][a-z0-9-]*(\\.[a-z][a-z0-9-]*)+$",
      "maxLength": 128
    },
    "name": { "type": "string", "minLength": 1, "maxLength": 80 },
    "version": {
      "type": "string",
      "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$"
    },
    "kind": { "type": "string", "const": "tool" },
    "family": {
      "type": "string",
      "enum": [
        "web_http", "dns_domain", "network", "web_analysis", "source_code",
        "mobile_apk", "binary_reverse", "document", "data_processing",
        "knowledge", "evidence", "utility", "browser_automation"
      ]
    },
    "description": { "type": "string", "minLength": 1 },
    "runtime": {
      "type": "object",
      "required": ["type"],
      "properties": {
        "type": {
          "type": "string",
          "enum": [
            "podman_vm", "python_worker", "native_rust",
            "mcp_client", "http_api", "cli_adapter", "remote_runner"
          ]
        }
      },
      "allOf": [
        {
          "if": { "properties": { "type": { "const": "podman_vm" } } },
          "then": {
            "required": ["image", "entrypoint"],
            "properties": {
              "image": { "type": "string", "pattern": "@sha256:[0-9a-f]{64}$" },
              "entrypoint": { "type": "array", "minItems": 1, "items": { "type": "string" } },
              "user": { "type": "string" },
              "readonly_rootfs": { "type": "boolean", "default": true },
              "mounts": {
                "type": "array",
                "items": {
                  "type": "object",
                  "required": ["role", "path", "mode"],
                  "properties": {
                    "role": { "type": "string", "enum": ["input", "output"] },
                    "path": { "type": "string" },
                    "mode": { "type": "string", "enum": ["ro", "rw"] }
                  }
                }
              },
              "env": { "type": "object", "additionalProperties": { "type": "string" } }
            }
          }
        },
        {
          "if": { "properties": { "type": { "const": "python_worker" } } },
          "then": {
            "required": ["protocol_version", "worker_name", "command"],
            "properties": {
              "protocol_version": { "type": "string", "const": "1" },
              "worker_name": { "type": "string", "minLength": 1 },
              "command": { "type": "array", "minItems": 1, "items": { "type": "string" } },
              "working_dir": { "type": "string" },
              "restart": { "type": "string", "enum": ["pooled", "per_run"], "default": "pooled" }
            }
          }
        },
        {
          "if": { "properties": { "type": { "const": "native_rust" } } },
          "then": {
            "required": ["binary", "subcommand"],
            "properties": {
              "binary": { "type": "string" },
              "subcommand": { "type": "string" }
            }
          }
        },
        {
          "if": { "properties": { "type": { "const": "mcp_client" } } },
          "then": {
            "required": ["endpoint", "tool_name"],
            "properties": {
              "endpoint": { "type": "string" },
              "tool_name": { "type": "string" }
            }
          }
        },
        {
          "if": { "properties": { "type": { "const": "http_api" } } },
          "then": {
            "required": ["base_url", "method", "auth"],
            "properties": {
              "base_url": { "type": "string", "format": "uri" },
              "method": { "type": "string" },
              "auth": { "type": "string", "enum": ["none", "bearer_secret", "header_secret"] },
              "request_template": { "type": "string" },
              "response_mapping": { "type": "object" }
            }
          }
        },
        {
          "if": { "properties": { "type": { "const": "cli_adapter" } } },
          "then": {
            "required": ["image", "argv_template"],
            "properties": {
              "image": { "type": "string", "pattern": "@sha256:[0-9a-f]{64}$" },
              "argv_template": { "type": "array", "items": { "type": "string" } }
            }
          }
        },
        {
          "if": { "properties": { "type": { "const": "remote_runner" } } },
          "then": { "description": "P2 runtime: registry must reject manifests of this type (D-TC vocabulary, plan section 6)." }
        }
      ]
    },
    "input_schema": { "type": "object" },
    "output_schema": { "type": "object" },
    "resources": {
      "type": "object",
      "required": ["cpu", "memory", "timeout_seconds"],
      "additionalProperties": false,
      "properties": {
        "cpu": { "type": "string", "pattern": "^[0-9]+(\\.[0-9]+)?(m)?$" },
        "memory": { "type": "string", "pattern": "^[0-9]+(Mi|Gi|Ki)$" },
        "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 600 },
        "max_output_bytes": { "type": "integer", "minimum": 256, "maximum": 67108864 },
        "pid_limit": { "type": "integer", "minimum": 1 },
        "disk_mb": { "type": "integer", "minimum": 1 }
      }
    },
    "network": {
      "type": "object",
      "required": ["mode"],
      "additionalProperties": false,
      "properties": {
        "mode": { "type": "string", "enum": ["none", "egress_allowlist", "vm_local"] },
        "egress": {
          "type": "object",
          "properties": {
            "ports": { "type": "array", "items": { "type": "integer", "minimum": 1, "maximum": 65535 } },
            "domains": { "type": "array", "items": { "type": "string" } },
            "cidrs": { "type": "array", "items": { "type": "string" } },
            "note": { "type": "string" }
          }
        }
      },
      "allOf": [
        {
          "if": { "properties": { "mode": { "const": "egress_allowlist" } } },
          "then": { "required": ["egress"], "properties": { "egress": { "required": ["ports"] } } }
        }
      ]
    },
    "secrets": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name", "inject_as", "required"],
        "additionalProperties": false,
        "properties": {
          "name": { "type": "string", "pattern": "^[A-Z][A-Z0-9_]+$" },
          "inject_as": { "type": "string", "enum": ["env", "file"] },
          "required": { "type": "boolean" }
        }
      }
    },
    "permission_level": { "type": "string", "enum": ["T0", "T1", "T2"] },
    "idempotent": { "type": "boolean" },
    "health": {
      "type": "object",
      "required": ["probe"],
      "properties": {
        "probe": {
          "type": "object",
          "required": ["type"],
          "properties": {
            "type": { "type": "string", "enum": ["worker_ping", "exec", "http_get", "none"] },
            "command": { "type": "array", "items": { "type": "string" } },
            "endpoint": { "type": "string" },
            "timeout_ms": { "type": "integer", "minimum": 100, "default": 5000 },
            "interval_seconds": { "type": "integer", "minimum": 30, "default": 600 },
            "degraded_after": { "type": "integer", "minimum": 1, "default": 1 },
            "broken_after": { "type": "integer", "minimum": 1, "default": 3 }
          },
          "allOf": [
            {
              "if": { "properties": { "type": { "const": "exec" } } },
              "then": { "required": ["command"] }
            },
            {
              "if": { "properties": { "type": { "const": "http_get" } } },
              "then": { "required": ["endpoint"] }
            }
          ]
        },
        "on_run_recover": { "type": "boolean", "default": true }
      }
    },
    "provenance": {
      "type": "object",
      "required": ["origin", "license", "dependencies", "tested"],
      "properties": {
        "origin": { "type": "string", "enum": ["rip-native", "opensource", "internal-kit"] },
        "source_url": { "type": "string" },
        "source_commit": { "type": "string" },
        "license": { "type": "string", "minLength": 1 },
        "dependencies": { "type": "array", "items": { "type": "string" } },
        "image_digest": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
        "known_limits": { "type": "array", "items": { "type": "string" } },
        "tested": { "type": "string", "minLength": 1 }
      }
    },
    "context": {
      "type": "object",
      "properties": {
        "summary": { "type": "string" },
        "docs": { "type": "string" }
      }
    },
    "tags": { "type": "array", "items": { "type": "string" } },
    "examples": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["input"],
        "properties": {
          "input": { "type": "object" },
          "output_hint": { "type": "object" }
        }
      }
    }
  }
}
~~~

校验记录（本会话实跑）：

1. Schema JSON 语法：`python3 -I` 提取本附录代码块后 `json.loads` 通过（无异常，无输出即成功）。
2. 两个示例 Manifest YAML 解析：`python3 -I` + pyyaml 6.0.3 逐一 `yaml.safe_load` 通过。
3. 未做 jsonschema 库级校验（模块缺失，红线禁装），见 §11 第 3 条。
4. 全文长横线检查：`grep -P '[\x{2014}\x{2013}]'` 对本文档零命中（见提交说明附带的校验命令输出）。
