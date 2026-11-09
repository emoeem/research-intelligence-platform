# Phase 0 交付物 5：插件协议草案（PLUGIN-CONTRACT-DRAFT）

日期：2026-11-09
性质：设计草案。本次只新增 docs/phase0/ 本文件，未修改任何源码，未写任何数据库（含 migrations/ 下 platform*.db），未向宿主机安装任何依赖，未改动生产 RTIS 行为（红线逐条遵守）。
对应计划条目：`docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md`（下称「计划」）§16 第 5 条：PLUGIN-CONTRACT-DRAFT.md，插件 Manifest 与生命周期草案（计划 889 行）；正文对齐计划 §7（插件系统，374 至 421 行）、§14 Phase 6（806 至 813 行）、§17 验收标准插件行（905 行）。
盘点基准：`docs/phase0/CURRENT-ARCHITECTURE-INVENTORY.md`（下称「盘点」）。
证据口径：对计划的引用写作（计划 行号），对盘点的引用写作（盘点 行号），均为本会话实际读取内容；本会话实际执行的命令原文与输出集中在 §13。草案性设计与未验证项分别标注「草案」「待验证」。

---

## 1. 范围

本文定义 RIP 2.0 插件协议：六类插件的注册边界（§3）、Plugin Manifest（§4）、生命周期状态机与数据保留（§5）、来源类型与版本固定（§6）、依赖与兼容检查（§7）、目录布局与位置配置（§8）、按需加载规则（§9），并给出两个示例 Manifest（§10）。实现落点与现状差距在 §11，草案决策点与待验证清单在 §12。

非范围：Tool capability 内部的调用协议细节（统一调用对象、Run 状态、输出约定，计划 214 至 237 行）归 TOOL-CONTRACT-DRAFT（计划 §16 第 6 条，该文档尚不存在，docs/phase0/ 实测只有三份文件）；Context Broker 的匹配与预算算法归 CONTEXT-BROKER-DESIGN（计划 §16 第 7 条）。

---

## 2. 设计原则

1. 插件不只是可执行程序：可以是工具、第三方项目、知识包、Markdown 文档、Agent、Workflow 或集成（计划 376 行）。六类中只有 Tool、Project、Integration 涉及执行面，Knowledge、Agent、Workflow 本质是声明性内容包。
2. 外部插件不能直接访问主数据库文件，只能通过 API、SDK 或受控 Adapter 访问业务能力（计划 123 行）。这是本协议第一红线，所有 capability 的写入路径必须经 Application API。
3. 元数据先行，按需加载：插件中心只加载 Manifest 元数据，任务匹配后加载插件摘要，真正调用才加载完整说明和 Schema（计划 419 至 421 行）。
4. 来源与版本可溯：支持固定 Commit/Release，记录许可证和来源（计划 417 行）；镜像固定 digest（计划 185、314 行）。
5. 禁用、卸载、升级是三种不同操作；禁用不删除数据，卸载插件不默认删除已进入主知识库的知识和证据（计划 415 行）。
6. 生命周期变更全部落审计事件（计划 122 行标准化记录要求的延伸；盘点 200 行确认 audit-engine 是当前唯一 Emit 出口）。
7. 不推倒 M0-M8：本草案是 Phase 6 的前置设计输入，Phase 0 与 Phase 1 不实现插件子系统（计划 806 行起；871 行「不提前实现后续阶段功能」）。

---

## 3. 六类插件与可注册能力

### 3.1 统一模型与能力类型

每个插件 = 一个 Manifest + 若干 capability。capability 是插件向平台注册的最小能力单元；capability_type 字段出自计划 §12 的 PluginCapability 设计（计划 692 行），七种取值为本草案枚举。

| capability type | 提供方 kind | 注册内容 | 平台落点 |
|---|---|---|---|
| knowledge_source | knowledge | 语料声明：位置、格式、解析管道、索引方式、质量规则 | Knowledge 数据面与检索索引源（盘点 202 行：search-engine 索引源当前仅 platform + catalog 两库，插件数据面是 2.0 扩展点） |
| tool | tool | 单个 ToolDefinition：id、版本、输入输出 Schema、runtime、资源限制、已知限制（计划 170 行记录清单） | Tool Registry（计划 349 行） |
| tool_pack | tool | 一组 tool 的打包声明与公共 runtime | Tool Registry + Tool Pack Manager（计划 350 行） |
| project_endpoint | project | 外部完整项目的接入点（MCP/API/CLI/Adapter）、健康检查、能力映射 | RuntimeNode + Tool Registry（mcp_client 与 http_api 行，计划 365 至 366 行） |
| agent_definition | agent | AgentDefinition、提示词包、上下文策略、能力绑定 | AgentDefinition 模型（计划 699 行） |
| workflow_definition | workflow | graph、input_schema、模板标记 | WorkflowDefinition 模型（计划 701 行） |
| integration_connector | integration | 连接器描述：Git 托管、搜索、模型、向量库、远程 MCP、endpoint、凭据引用 | Integrations 页面（计划 553 行） |

### 3.2 Knowledge Plugin（计划 380 行）

- 可注册：knowledge_source capability。声明语料位置与格式（Markdown、PDF、代码仓库文档、知识库、方法论），解析走平台管道，索引声明 FTS 与向量轴，质量规则可挂 evaluation worker 规则集；必须带许可证与来源（计划 417 行）。
- 不可做：直写主知识库表；知识入主库只能经摄取管道并保留 plugin_id 溯源（原则 2）。
- 现有候选：integrations/rtis kit，含 knowledge.db、125 个技能 md 与 pocs/ 正文（盘点 123 行）；盘点 128 行已裁定该 kit 是内容资产，应作为知识源之一接入 Context Broker 而不是重写。knowledge.db 条数两处口径不一（README 记 14,717，rtis.py 头注记 14664），实际条数待验证（盘点 129 行，本次未查库）。

### 3.3 Tool Plugin（计划 381 行）

- 可注册：tool 与 tool_pack capability。runtime 必须是计划 §6 七种之一：podman_vm、python_worker、native_rust、mcp_client、http_api、cli_adapter、remote_runner（计划 360 至 368 行）。
- 不可做：声明宿主机任意 Shell；镜像 digest、运行参数、网络策略必须来自可信 Registry 元数据，不能由 Agent 拼装（计划 370、210 行）。
- 现有候选：workers/python 三个 worker（Worker Protocol v1 五方法，盘点 96 至 100 行；盘点 102 行与 187 行两处裁定该协议可作 Tool Contract 最接近的现有协议基础）；scripts/ 三个只读脚本是第一批登记对象，天然满足 T0 只读分级（盘点 152 行）。

### 3.4 Project Plugin（计划 382 行）

- 可注册：project_endpoint capability。外部完整项目经 MCP/API/CLI/Adapter 接入：MCP server 地址或启动命令、API base、CLI wrapper、Adapter 类型；声明健康检查方式；把该项目暴露的能力映射进 RIP 能力目录（如 MCP 工具映射为 mcp_client runtime 的 Tool）。
- 不可做：凭据写进 Manifest（决策点 D5）；endpoint 不固定版本（release/commit/digest 缺一即校验失败）。
- 现有候选：rtis_mcp.mjs（stdio JSON-RPC 2.0，工具面 9 个，盘点 123 行）；Phase 6 验收要求至少接入一个外部项目（计划 813 行）。

### 3.5 Agent Plugin（计划 383 行）

- 可注册：agent_definition capability。特定研究领域 Agent 定义、提示词、规划策略、上下文策略（context_policy）与能力绑定（capability_refs 指向已注册的 tool/knowledge/workflow id，字段对齐计划 699 行）、Token 预算与资源限制。
- 不可做：Agent 定义内嵌工具实现；能力由平台配置提供，不由 Agent 临时安装软件获得（计划 588 行）。
- 现状：仓库无任何 Agent 定义资产（盘点差距清单第 2 与第 7 条，217、222 行），此类型为纯前瞻设计。

### 3.6 Workflow Plugin（计划 384 行）

- 可注册：workflow_definition capability。graph（节点与边）、input_schema、版本、模板标记；满足可导入、可编辑、可复用（计划 384 行）。
- 现状：workflow 数据面已存在（003_tools_workflow.sql 的 workflow 与 workflow_registry_meta 表，盘点 136 行）但无 Rust 读写面（盘点 217 行差距 2）；存量 workflow 行可作为首批导入候选（行数待验证，本次未查询 platform.db）。

### 3.7 Integration Plugin（计划 385 行）

- 可注册：integration_connector capability。GitHub/GitLab/Gitee、搜索、模型、向量库、远程 MCP 连接器：endpoint、协议版本、限流、凭据引用（只存引用不存本体）、健康状态。
- 现有候选：search-engine 已消费 catalog 库 github_metadata（topics/language/license/maintenance，盘点 74 行），该外部目录接入可包装为 Integration 插件。

---

## 4. Plugin Manifest 草案

### 4.1 字段规范

文件名 manifest.yaml（或 manifest.yml / manifest.json）。格式 YAML，校验用 JSON Schema draft 2020-12（仓库 schemas/worker-protocol/v1.json 已用同一 draft，盘点 96 行）。未知字段一律拒绝（strict 模式，草案 D4）。

| 字段 | 必填 | 说明 |
|---|---|---|
| schema_version | 是 | Manifest 规范版本，当前 "1"；平台按此选校验器，不支持即 INVALID（对齐计划 177 行 Tool Manifest 同名字段） |
| id | 是 | 插件身份，小写点分 namespace.name，全局唯一且不可变（草案 D1，样例 tool.rip.python-workers） |
| name | 是 | 展示名 |
| version | 是 | 插件版本，semver |
| kind | 是 | knowledge / tool / project / agent / workflow / integration 六选一（计划 380 至 385 行） |
| description | 是 | 一句话摘要，进 L1 目录元数据 |
| authors | 否 | 作者数组 |
| homepage / documentation | 否 | 链接 |
| license | 是 | SPDX 表达式或 UNLICENSED；必填依据是计划 417 行「记录许可证和来源」 |
| source | 是 | 来源对象，见 §6 |
| compatibility | 是 | 平台兼容声明，见 §7 |
| capabilities | 是 | capability 数组，每项含 id、type、entry、summary、provides/requires，见 §3.1 |
| dependencies | 否 | 依赖数组，见 §7 |
| permissions | 否 | 网络与文件访问范围、Agent 可见性（Agent 可见性控制出自计划 249 行） |
| data_retention | 否 | 持久数据声明与迁移钩子位置，见 §5.4 |
| resources | tool 与 tool_pack 必填 | cpu、memory、timeout_seconds（对齐计划 201 至 204 行） |
| context | 是 | summary 与 docs 两个分层加载入口（对齐计划 205 至 207 行） |
| lifecycle | 否 | 可选钩子：pre_install / post_install / pre_upgrade / post_upgrade / pre_uninstall；钩子脚本同样经受控 runtime 执行，不是宿主机 Shell（原则 2 与计划 370 行） |

### 4.2 校验顺序（VALIDATED 状态的动作清单）

1. schema_version 受支持。
2. 全文 JSON Schema 校验（strict）。
3. id 全局唯一，version 合法 semver，kind 与 capabilities[].type 匹配（如 knowledge 插件不得声明 tool_pack）。
4. license 与 source 信息完整（计划 417 行）。
5. 远程来源必须带版本固定，见 §6。
6. compatibility.rip_core 与当前平台版本比对。
7. capabilities[].entry 指向的文件存在于包内。

全部通过才允许进入 INSTALLED；任何失败产出机器可读报告，进 INVALID 并展示于 Plugin Center（计划 584 行「兼容性」列）。

---

## 5. 生命周期状态机

### 5.1 状态定义

主链七态来自计划 413 行原文：DISCOVERED → VALIDATED → INSTALLED → REGISTERED → ENABLED → LOADED → ACTIVE。其余五态为本草案补充（草案 D2）。

| 状态 | 含义 | 进入方式 |
|---|---|---|
| DISCOVERED | 来源扫描发现，Manifest 元数据已解析进 registry 索引 | 系统：discover |
| VALIDATED | §4.2 全部静态检查通过 | 系统：validate |
| INSTALLED | 内容落盘 installed/<id>/<version>/，校验和与 lock 写入 | 系统：install |
| REGISTERED | capabilities 登记进平台数据面，可被搜索与发现，不可被任务匹配 | 系统：register |
| ENABLED | 允许被 Context Broker 匹配与任务调用；运行资源未加载 | 用户：enable（前置依赖检查） |
| LOADED | 插件运行资源已加载（worker 已连、adapter 已就绪） | 系统：按需 load |
| ACTIVE | 至少一个 capability 正在服务调用 | 系统：首次调用成功 |
| INVALID | 静态校验失败 | 系统 |
| DISABLED | 用户禁用；安装内容与数据全保留；撤出匹配面 | 用户：disable |
| UPDATE_PENDING | 远端存在兼容新版本（叠加标记，非独占态） | 系统：update_check |
| ERROR | 健康检查失败或运行期故障 | 系统 |
| UNINSTALLED | 终态：内容删除，保留数据按 §5.4 | 用户：uninstall |

~~~text
主链：DISCOVERED -> VALIDATED -> INSTALLED -> REGISTERED -> ENABLED -> LOADED -> ACTIVE

分支：
DISCOVERED --validate 失败--> INVALID
ENABLED/LOADED/ACTIVE --disable--> DISABLED --re-enable（重过兼容与依赖检查）--> ENABLED
ACTIVE/LOADED --deactivate（释放运行资源）--> ENABLED
ACTIVE/LOADED --健康检查失败--> ERROR --修复后 re-enable--> ENABLED
已安装各态 --update_check 发现兼容新版--> 叠加 UPDATE_PENDING 标记（状态本身不变）
UPDATE_PENDING --upgrade--> 新版本走 VALIDATED/INSTALLED/REGISTERED 到 ENABLED（旧版本目录保留，见 5.3）
REGISTERED/ENABLED/DISABLED --uninstall--> UNINSTALLED（终态）
~~~

### 5.2 跃迁表

| 跃迁 | 触发者 | 前置检查与动作 | 失败处理 |
|---|---|---|---|
| discover | 系统（来源扫描） | 解析 Manifest 元数据；id 冲突检测 | 解析失败则记录并跳过，不影响其他插件 |
| validate | 系统 | §4.2 清单 | 进 INVALID，报告入 Plugin Center |
| install | 用户确认 | 按 source 拉取，校验和，写 plugin.lock 与 checksums.sha256 | 拉取失败可重试；校验和失败进 INVALID |
| register | 系统 | capability 写入平台数据面（走迁移纪律，见 §11） | 失败回退并留审计 |
| enable | 用户 | §7 依赖解析、runtime 可用性、权限确认 | 拒绝并输出缺失依赖与原因清单 |
| load | 系统（按需） | runtime 健康检查（podman_vm 心跳，计划 335 至 341 行） | 停在 ENABLED，不伪装成 ACTIVE |
| activate | 系统 | 首个 capability 服务调用成功 | 保持 LOADED |
| deactivate | 系统或用户 | 释放 worker、连接、缓存 | 无 |
| disable | 用户 | 先 deactivate；capability 撤出匹配面；数据不动 | 无 |
| re-enable | 用户 | 重跑 enable 全部检查 | 同 enable |
| update_check | 系统 | 远端版本与 compatibility 比对 | 无新版则清除标记 |
| upgrade | 用户确认 | 见 §5.3 | 失败自动回滚旧版本 |
| uninstall | 用户确认 | 先自动 disable；按 §5.4 处置数据 | 删除失败则停留原状态并告警 |

### 5.3 禁用、卸载、升级语义（计划 415 行「不同操作」的展开）

禁用（disable）：

- 状态置 DISABLED，capability 从 Context Broker 匹配面与 MCP 目录撤出（计划 658 行：MCP 初始化不暴露全部说明，禁用插件则完全不出现）。
- installed/ 内容、已入主库知识、证据、Run、审计记录全部保留（计划 415 行「禁用不删除数据」）。
- re-enable 必须重过依赖与兼容检查，因为停用期间平台或其他插件可能已变化。

卸载（uninstall）：

- 前置自动 disable；删除 installed/<id>/ 与 cache/ 中该插件产物。
- 不默认删除已进入主知识库的知识和证据（计划 415 行原文语义）：这些数据保留 plugin_id 溯源字段并标记「来源已卸载」，在 Plugin Center 可查看孤儿来源清单。
- Manifest data_retention.persistent 声明的持久数据（如本地构建的索引缓存）默认一并删除，但卸载前列出明细要求显式确认。
- Run、Artifact、Evidence、审计事件一律保留（计划 122 行）。

升级（upgrade）：

1. update_check 发现兼容新版本，叠加 UPDATE_PENDING 标记，当前版本继续运行（对应 Tool Center「更新待处理」状态，计划 241 行）。
2. 用户确认后新版本拉取到 installed/<id>/<version_new>/，与旧版本目录并存；新版本先走完整 VALIDATED 检查。
3. Manifest 声明数据迁移钩子时按序执行；失败即回滚到旧版本，不切换指针（对齐计划 314 行「更新先验证再切换」与 340 行「VM 镜像升级失败可回滚」的既有原则）。
4. 切换 active 版本指针，重新走 REGISTERED 到 ENABLED（重过依赖检查）。
5. 回滚窗口默认保留上一版本一个（草案 D6），窗口结束且用户确认后清理旧目录。

### 5.4 数据保留规则

| 数据类别 | 位置 | 禁用时 | 卸载时 | 升级时 |
|---|---|---|---|---|
| 插件本体与 Manifest | installed/<id>/<version>/ | 保留 | 删除 | 新旧版本目录并存，旧版本保留至回滚窗口结束 |
| 已入主库的知识 | 主知识库数据面（摄取管道写入） | 保留 | 保留（带 plugin_id 溯源，标记来源已卸载） | 保留 |
| Evidence 与 Artifact | 统一证据面 | 保留 | 保留 | 保留 |
| Run 与审计事件 | Run 数据面 + audit_event | 保留 | 保留 | 保留 |
| 插件私有数据 | installed/<id>/<version>/data/ | 保留 | 默认删除；persistent 声明项需显式确认 | 可执行迁移钩子 |
| 下载与构建缓存 | cache/ | 可清理 | 清理 | 可清理 |

---

## 6. 来源类型与版本固定

来源对象 source 的 type 取值五类（计划 417 行列举的收敛；GitHub/GitLab/Gitee 并入 git 用 host 字段区分，草案 D7）。

| source.type | 必填字段 | 版本固定 | 校验 |
|---|---|---|---|
| local_dir | path | pinned.kind=commit 时取 HEAD sha（目录为 git 工作树时） | 包内 checksums.sha256 |
| git | url、host（github/gitlab/gitee/self）、pinned | pinned.kind 为 commit 或 tag 或 release，value 必填；生产安装禁止浮动分支（草案 D8） | commit 存在性；子模块与 LFS 声明 |
| archive | url、sha256（必填）、size | 归档文件 sha256 | 解压后逐文件 checksums |
| mcp_server | endpoint（URL 或启动命令）、protocol_version | initialize 握手结果 + 工具列表快照哈希 | 能力协商记录进 lock |
| http_api | base_url、api_version | API 版本 + 契约文件哈希 | 契约校验（首批契约形态待验证） |

安装时写 plugin.lock（插件级锁定记录）：resolved version、pinned ref、sha256、许可证、来源 URL、安装时间、信任级。升级与回滚一律以 lock 为准。容器镜像类资源固定 digest（计划 185、314 行）。

信任级四档：official / community / local / remote（计划 584 行），影响安装确认提示强度与自动更新默认策略（草案 D10：仅 official 允许 update_check 自动提示，其余须手动检查）。

---

## 7. 依赖与兼容检查

三层检查（草案 D9）：

静态层（VALIDATED）：§4.2 清单，含 compatibility 字段完整性。

激活层（enable 前）：

1. 插件依赖解析：dependencies[] 每项为 {id, requirement}，requirement 用 semver 区间（>=、<、^、~）；缺失或无满足版本则拒绝 enable，输出机器可读缺失清单。
2. capability 依赖：requires 指向其他插件的 provides；被依赖插件被禁用或卸载时，依赖方拒绝 enable 并提示原因。
3. 依赖环检测：存在环即拒绝。
4. 平台兼容：compatibility.rip_core 区间、compatibility.manifest_schema、compatibility.worker_protocol（如 v1，对齐 schemas/worker-protocol/）。
5. runtime 可用性：capability 声明的 runtime 类型必须在计划 §6 七种之内；podman_vm 要求 VM 在线（计划 335 至 341 行心跳机制）；python_worker 要求 worker 协议版本可协商。

运行层（LOADED 与 ACTIVE 期间）：周期健康检查；镜像 digest 与 lock 记录一致；失败进 ERROR 并停止重试，不做密集轮询（对齐用户执行契约「不 tight-poll」）。

兼容矩阵与依赖图在 Plugin Center 展示（计划 584 行「兼容性」「依赖图」）。

---

## 8. 目录布局与位置配置

按计划 390 至 407 行目录骨架细化（草案 D3 补充 state/ 与 logs/ 两个子目录）：

~~~text
<rip-data>/                      # 2.0 目标数据根，仓库当前不存在（本次 ls 实测），位置配置化
  plugins/
    registry/                    # 来源目录与索引：catalog.json、registry-index.json，只存元数据
    installed/
      <plugin_id>/
        <version>/
          manifest.yaml
          capabilities/          # 各 capability 入口描述与 Schema
          docs/                  # L3 完整文档
          data/                  # 插件私有数据（保留规则见 5.4）
          checksums.sha256
    disabled/                    # 计划原文目录；默认实现用状态标记而非物理移动（草案 D3），物理隔离为可选项
    cache/                       # 下载、解压、索引构建缓存，可随时清理
    state/                       # plugin-state.json + transitions.jsonl（Phase 6 起落库，见 §11）
    logs/                        # 插件运行日志
~~~

位置配置（计划 409 行「目录位置通过配置指定，不硬编码到源码」）：

- rip.data_dir：数据根，默认随部署根；环境变量 RIP_DATA_DIR 覆盖。
- rip.plugins.dir：插件根，默认 {data_dir}/plugins；单独配置时其余 rip-data 子目录不受影响。
- 源码中禁止出现 rip-data 绝对路径字面量；测试用内存库与临时目录（对齐 database crate 内存库口径，盘点 53 行）。

---

## 9. 按需加载规则

三级规则来自计划 419 至 421 行，与 §8 四级上下文（计划 437 至 443 行）对齐：

| 级别 | 时机 | 加载内容 | 对应上下文层 |
|---|---|---|---|
| L1 目录级 | 会话启动、目录浏览、搜索 | 仅 Manifest 元数据：id、name、version、kind、description、license、state | L1（计划 440 行） |
| L2 匹配级 | Context Broker 命中、任务规划 | 插件摘要（context.summary）+ capability 清单及各自一句话摘要 | L2（计划 441 行） |
| L3 调用级 | 真正调用 | 完整文档（docs/）、输入输出 Schema、capability entry 激活 | L3（计划 442 行） |

附加规则：

1. 禁止把插件全部正文或全部工具 Schema 预载进上下文（计划 444 至 446 行；Phase 4 验收 794 行）。
2. MCP 初始化只暴露目录与分页发现（计划 658 行）；插件相关 MCP 面为 rip.list_plugins、rip.search_plugins、rip.get_plugin_manifest（计划 642 至 644 行）与 rip://plugins/catalog（计划 664 行）。禁用插件不出现在任何一级。
3. 工具输出以 Artifact 引用进入上下文，不内联正文（计划 794 行）。
4. Knowledge 插件的索引构建属 INSTALLED 后台步骤，不占上下文 Token；向量轴对接 embedding worker（盘点 99 行）与 search-engine 缺口（盘点 202 行）。
5. LOADED 是懒加载态：除声明 eager_load 的轻量插件外，ENABLED 不预载任何运行资源。

---

## 10. 示例 Manifest

两个示例包装的均为本会话实测存在的资产。license 取占位值：仓库顶层无 LICENSE 文件（本次 ls 实测），法律口径待验证（§12）。

### 10.1 Knowledge 插件示例：knowledge.rtis-redteam

~~~yaml
schema_version: "1"
id: "knowledge.rtis-redteam"
name: "RTIS 红队知识包"
version: "0.1.0"
kind: "knowledge"
description: "RTIS kit 的 POC 知识库、技能方法论与编排案例作为 RIP 知识源接入"
authors: ["emoeem"]
license: "UNLICENSED"          # 占位，待许可证裁决（见 12 节）
source:
  type: "local_dir"
  path: "integrations/rtis"
  pinned:
    kind: "commit"
    value: "022a717"           # 安装时以 git rev-parse HEAD 全量 sha 复核（本次实测 HEAD）
compatibility:
  rip_core: ">=0.2.0, <1.0.0"
  manifest_schema: ["1"]
capabilities:
  - id: "knowledge-source.rtis.poc-corpus"
    type: "knowledge_source"
    entry: "capabilities/knowledge_source.yaml"
    summary: "POC 正文与技能文档语料，中文 2-gram 检索与证据等级排序由 kit 自带引擎承担"
    provides:
      corpus_id: "rtis-redteam"
      content_kinds: ["poc", "skill_md", "method_case"]
      index: { fts: true, vector: false }
      parse_pipeline: "tool.rip.text.parse"   # 复用 10.2 示例中的解析工具
  - id: "knowledge-source.rtis.orchestration-cases"
    type: "knowledge_source"
    entry: "capabilities/knowledge_source.yaml"
    summary: "任务状态机与站型画像案例，条数口径待验证（README 与 rtis.py 头注不一致）"
dependencies: []
permissions:
  data_access: "ingest_via_api"               # 只能经摄取管道入主库，不直写（原则 2）
data_retention:
  ingested_policy: "retain_with_provenance"   # 卸载插件不删已入主库知识（计划 415 行）
  persistent: []
context:
  summary: "红队 POC 与技能知识源，安全研究类任务 L2 命中后按需加载"
  docs: "docs/usage.md"
~~~

### 10.2 Tool 插件示例：tool.rip.python-workers

~~~yaml
schema_version: "1"
id: "tool.rip.python-workers"
name: "RIP Python Workers"
version: "0.1.0"
kind: "tool"
description: "把 parsing、embedding、evaluation 三个 Python Worker 登记为 Tool Engine 可调度工具"
authors: ["emoeem"]
license: "UNLICENSED"          # 占位，待许可证裁决
source:
  type: "local_dir"
  path: "workers/python"
  pinned:
    kind: "commit"
    value: "022a717"
compatibility:
  rip_core: ">=0.2.0, <1.0.0"
  manifest_schema: ["1"]
  runtimes: ["python_worker"]
  worker_protocol: "v1"        # schemas/worker-protocol/v1.json
capabilities:
  - id: "tool.rip.text.parse"
    type: "tool"
    entry: "parsing_worker.py"
    summary: "md、txt、html 解析，输出 sections、heading_tree、stats"
    runtime:
      type: "python_worker"
      protocol: "worker-protocol/v1"
      method: "execute"
    input_schema:
      type: object
      required: ["content"]
      properties:
        content: { type: string }
        format: { type: string, enum: ["md", "txt", "html"] }
    output_schema:
      type: object
      properties:
        sections: { type: array }
        heading_tree: { type: object }
        stats: { type: object }
    resources:
      cpu: "1"
      memory: "512Mi"
      timeout_seconds: 60
      input_max_bytes: 10485760               # 10 MiB 硬上限，盘点 98 行口径
    permissions:
      network: "none"
      filesystem: "task_workspace_ro"
  - id: "tool.rip.text.embed"
    type: "tool"
    entry: "embedding_worker.py"
    summary: "双后端向量嵌入：hashing-ngram-v1 默认，bge-small-zh-v1.5 ONNX 可选，维度显式协商"
    runtime: { type: "python_worker", protocol: "worker-protocol/v1", method: "execute" }
    resources: { cpu: "2", memory: "1Gi", timeout_seconds: 120 }
    permissions:
      network: "none"
      filesystem: "model_dir_ro"              # models/bge-small-zh-v1.5 或 EMBEDDING_MODEL_DIR，盘点 99 行
  - id: "tool.rip.knowledge.evaluate"
    type: "tool"
    entry: "evaluation_worker.py"
    summary: "规则集三层判定，只输出建议不改写知识状态"
    runtime: { type: "python_worker", protocol: "worker-protocol/v1", method: "execute" }
    resources: { cpu: "1", memory: "512Mi", timeout_seconds: 60 }
    permissions: { network: "none", filesystem: "task_workspace_ro" }
dependencies: []
data_retention:
  persistent: []
context:
  summary: "平台自带 Python Worker 工具包，经 Worker Protocol v1 五方法调用"
  docs: "workers/python/README.md"
~~~

---

## 11. 与现有资产和已交付能力的对齐

1. 现状：仓库无插件子系统。crates/ 实测仅 6 个 crate（本次 ls 输出），无 plugin-registry 与 plugin-runtime；盘点差距清单第 2 条同口径（盘点 217 行）。本草案是 Phase 6 的前置输入，Phase 0 与 Phase 1 不实现（计划 806 行起、871 行）。
2. 实现落点：plugin-registry 与 plugin-runtime 两个 crate（计划 714 至 715 行）；Plugin 与 PluginCapability 域实体进 crates/domain（盘点 48 行：2.0 新实体唯一合法落点；当前 domain 只收编 2/15 实体）。
3. 数据面：Plugin、PluginCapability 等新表走 migrations 纪律（快照、隔离、幂等、100% 对账，盘点 137 至 141 行与 207 行）；红线「不写 migrations/ 下的 platform*.db」全程生效（盘点 144 行）。Phase 6 之前生命周期状态先用 §8 的 state/ 文件承载（草案 D3）。
4. 审计：install、enable、disable、upgrade、uninstall 全部 Emit 审计事件，经 audit-engine（盘点 200 行确认其为唯一 Emit 出口，Python 与 rtis 侧 JSONL 分叉待收敛）；插件事件从第一天就只走 audit-engine，不新增分叉。
5. 治理扩展：quality-engine 治理对象从 Resource 扩到 Plugin 是既有裁定（盘点 83、203 行）；插件 INVALID 处置可复用其双轴状态机与 Review 队列骨架（草案建议，未实现）。
6. RTIS 红线：把 integrations/rtis 包装为 Knowledge 或 Project 插件是接入不是重写（盘点 128 行）；生产 rtis.py 行为不变，RTIS_RESOLVER_MODE 三态加 backups/ 的既有回滚机制沿用（盘点 126 行）；eri_adapter 生产 Resolve 路径不受插件生命周期影响（盘点 124 行）。
7. Web 入口：Plugin Center 页面（计划 582 至 584 行）与当前只读工作台（盘点 110 行：4 个只读 API）的关系是演进不是并列；插件 API 属 Application API 边界，不复刻「handler 直连 SQL」的既有债（盘点 115 行）。
8. 与 TOOL-CONTRACT-DRAFT 的边界：Tool capability 内部调用协议归该文档（计划 889 至 890 行相邻条目），本文只约束包装层与生命周期。

---

## 12. 草案决策点与待验证清单

草案决策点（均可在 Phase 6 评审时推翻）：

- D1 插件 id 采用 namespace.name 点分格式，kind 前缀仅为约定。
- D2 辅助五态（INVALID、DISABLED、UPDATE_PENDING、ERROR、UNINSTALLED）为本草案补充，主链七态不变。
- D3 禁用默认用状态标记而非物理移动目录，保证 installed 路径稳定；disabled/ 目录保留用于物理隔离可选项；Phase 6 前状态存 state/ 文件。
- D4 Manifest strict 校验，未知字段拒绝。
- D5 凭据只存引用，本体进平台密钥存储（存储位置待 Integration 页面设计，计划 553 行）。
- D6 升级回滚窗口默认保留上一版本一个。
- D7 hosted git 三家并入 git 来源，host 字段区分。
- D8 生产安装禁止浮动分支来源。
- D9 依赖与兼容检查分静态、激活、运行三层。
- D10 仅 official 信任级允许自动 update_check 提示。

待验证清单：

1. integrations/rtis/knowledge.db 实际条数（两处口径不一，盘点 129 行；本次未查库）。
2. 仓库许可证状态（顶层无 LICENSE 文件，本次 ls 实测；示例 license 字段为占位）。
3. workflow 表存量行数（可作为 Workflow 插件首批导入候选；本次未查询 platform.db）。
4. HTTP 来源契约文件形态（首批接入对象未定）。
5. CI runner 上 podman 可用性（影响 podman_vm runtime 类插件的端到端测试，盘点 173 行已列为待验证，本文沿用该口径）。

---

## 13. 本会话实际执行的命令与输出

1. `git -C research-platform rev-parse --short HEAD`，输出 `022a717`。
2. `git -C research-platform status --short`，输出 `?? docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md` 与 `?? docs/phase0/`（docs/phase0/ 整目录未跟踪）。
3. `ls research-platform` 顶层：含 crates、docs、integrations、migrations、schemas、workers 等，无 rip-data；`ls -d rip-data` 返回「没有那个文件或目录」。
4. `ls crates/`，输出 audit-engine、database、domain、quality-engine、resource-engine、search-engine 六项，无 plugin 相关 crate。
5. `ls -la integrations/rtis/*.db`，输出 knowledge.db 28299264 字节（2026-11-09 09:50）。

未执行任何写操作命令：无数据库写入、无源码修改、无依赖安装。
