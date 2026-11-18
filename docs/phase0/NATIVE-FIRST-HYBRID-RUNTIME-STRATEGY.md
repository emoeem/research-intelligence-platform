# RIP 原生优先与混合 Runtime 策略

- 日期：2026-11-18
- 状态：策略裁决已确认；代码迁移分阶段实施
- 范围：RIP 平台进程、Rust 工具、可信 Python Worker、第三方工具、容器/VM 执行边界，以及 Cargo 构建产物治理
- 前置：Phase 5 RELEASE 已收口；RIP3 WSW-0 已收口；WSW-1 已于 077a8dd4 收口（本文写作时点存在未提交工作，见 §8.2 时点标注），本策略不得覆盖或打断该批次
- 历史依据：`TOOL-RUNTIME-ADR.md`、`phase2/PHASE2-DESIGN.md`、`phase2/PHASE2-ACCEPTANCE.md`

## 1. 决策摘要

RIP 从“工具执行默认容器化”调整为 **Native-first、Isolation-when-needed（原生优先、按风险隔离）**。

1. RIP 平台自身的 Rust 服务、Workflow/Agent/Tool 编排、数据库访问层、Web API、Web 安全工作台代理服务，默认在宿主机原生运行。
2. 已审核、可信、依赖可控的 Rust 工具使用原生进程；可信 Python Worker 使用项目级虚拟环境和既有 Worker Protocol，不要求每个 Worker 都运行在容器中。
3. 只有存在依赖冲突、系统库/运行时版本冲突、重型依赖、明确文件系统/网络隔离需求或不可信代码执行风险时，才选择 rootless Podman。
4. 需要强隔离、特殊内核/设备能力或高风险不受信执行的任务，使用专用 VM 内的容器；不能因为 `podman_host` 已可用就把高风险任务放到宿主共享内核的容器中。
5. 容器是执行后端之一，不是平台架构中心。所有 Runtime 继续复用同一 Tool Registry、Tool Engine、Runtime Gateway、ToolRun、Artifact、Evidence、Trace 和预算/取消语义，不得建立第二套事实源。
6. Cargo、Rust 编译、测试、Clippy、Web 构建和 CI 默认在原生构建环境运行。不得为了“环境一致”把整个开发仓库、`target/`、`vendor/` 或完整 CI 塞入容器。
7. 本策略不要求立即删除既有镜像、容器、VM 设计或 `podman_host` 代码；先分类、测量、实现原生适配器，再逐工具迁移。历史 ADR 和 Phase 2 验收报告保留为历史事实。

## 2. 为什么调整

### 2.1 当前构建磁盘问题

2026-11-18 本机只读实测：

| 项目 | 实测 | 说明 |
|---|---:|---|
| `target/` | 约 70 GiB | Cargo 本地构建目录 |
| `target/debug/deps/` | 约 69 GiB | 绝大部分空间在这里 |
| `target/debug/incremental/` | 约 106 MiB | 不是当前主要占用来源 |
| `target/debug/build/` | 约 516 MiB | build scripts 产物 |
| `vendor/` | 约 386 MiB | 仓库离线依赖源，不是 69 GiB 的来源 |

目录统计显示 `debug/deps` 中存在大量测试可执行文件、`.rlib`、`.rmeta` 和依赖文件；Workspace 有多个 crate、集成测试目标和多种依赖指纹。**不能把这 69 GiB 简单解释为容器镜像或 vendor，也不能把不同 hash 的文件都认定为无用重复文件。** 它主要是 Rust 构建产物累积问题，应单独治理。

本轮不执行 `cargo clean`、不删除 `target/`、不清理 Podman 镜像/容器/卷。先建立可复现的构建配置 A/B 数据，再决定 Profile 和清理策略。

### 2.2 现有 Runtime 能力与缺口

当前代码已具备：

- `python_worker`：`crates/tool-engine/src/adapter/python_worker.rs`，宿主进程池与 Worker Protocol 派发器。
- `podman_host`：`crates/tool-engine/src/adapter/podman_host.rs`，rootless Podman 容器执行适配器。
- `podman_vm`：Registry 词表保留，Phase 2 目标 VM 拓扑；当前不能把设计状态误报为已完成的 VM Runtime。
- `native_rust`：Registry Manifest 校验分支已存在（`binary`、`subcommand`），但 `tool-engine` 当前公开 Adapter 仍只有 Python Worker 与 Podman Host 等适配器，尚无完整、通用、受策略约束的 Native Rust Adapter。
- `mcp_client`、`http_api`、`cli_adapter`：按既有契约作为外部/命令适配类型评估，不得把它们一律转换为 Podman。

因此，“支持 native_rust 的 Manifest”不等于“Native Rust Runtime 已完整落地”。实施计划必须先补能力和安全守卫，再迁移工具登记。

## 3. Runtime 选择矩阵

| Runtime / 组件 | 默认执行方式 | 适用条件 | 不适用条件 |
|---|---|---|---|
| RIP Rust 平台服务 | 宿主原生 | 自有、可信、平台核心服务 | 不执行任意第三方代码 |
| `native_rust` 工具 | 宿主原生受控子进程 | 来源可信、版本/哈希固定、Manifest 登记、资源预算可执行 | 不可信二进制、需要强隔离或特权能力 |
| `python_worker` | 宿主项目级 venv / Worker 进程池 | 可信自研 Worker、依赖可锁定、协议兼容 | 不可信 Python 包/代码、系统依赖冲突严重 |
| `podman_host` | 按需 rootless 容器 | 可信但依赖复杂、需要工具级文件/网络/资源隔离 | 高风险不受信代码；不能将 rootless 等同 VM 隔离 |
| `podman_vm` | VM 内 rootless 容器 | 不受信第三方工具、高风险执行、需要宿主内核隔离 | 普通可信轻量工具，不应成为默认路径 |
| `mcp_client` / `http_api` | 外部服务连接 | 服务边界清晰、身份认证和网络策略明确 | 不应为调用 API 额外启动容器 |
| `cli_adapter` | 依据目标程序信任等级选择受控原生进程或容器 | 仅允许固定可执行文件/参数映射 | 禁止退化为 Agent 任意 Shell |
| WSW Proxy / MITM 服务 | 宿主原生 Rust 服务 | 需要稳定 loopback 监听、与本机浏览器/证书/UI 协作 | 不将平台代理核心塞入通用工具容器 |

### 3.1 原生执行的准入条件

所有条件都满足才可使用宿主原生执行：

- 工具来源可信且经过审核；第三方二进制/包必须记录来源、版本、许可证和 SHA-256/发布校验信息。
- Manifest 固定工具身份、可执行文件身份、允许的子命令和参数 Schema；不得接收任意可执行路径或任意 Shell 字符串。
- 由 Tool Engine 创建受控子进程，明确工作目录、环境变量白名单、stdin/stdout/stderr、超时、取消、进程组回收和输出上限。
- 默认不继承不必要的凭据、SSH Agent、浏览器 Profile、数据库路径或宿主环境变量。
- 网络访问默认关闭或经统一 Egress Policy/代理；不能因为进程在宿主运行就绕过现有网络策略。
- 任务输入/输出仍经既有 Workspace、Artifact、Evidence 和 Trace 路径；不得直接读写生产数据库。
- 明确记录 Runtime 类型、工具版本、二进制指纹、执行参数摘要、退出码和资源用量。

原生进程不具备容器/VM 同等级的安全隔离。准入判断不满足时必须升级到 `podman_host` 或 `podman_vm`，不能为了性能强行原生化。

### 3.2 容器准入条件

至少存在一项可验证理由，才选择容器：

- Python/Node/Java/Go 或系统库版本冲突，项目级 venv/工具链无法经济解决。
- 重型依赖或大型系统运行时需要独立生命周期（例如特定 OCR/文档解析、浏览器自动化、移动/逆向工具链）。
- 工具需要独立文件系统、网络、CPU/内存/PID/临时空间限制。
- 工具本身虽可信，但升级频率/依赖冲突可能影响其他工具。
- 明确的风险模型要求容器隔离。

容器化按工具族/稳定依赖包拆分，避免一个巨型镜像，也避免每个微小工具都创建一个独立镜像。镜像必须 digest 固定、按需构建/拉取、默认不常驻空闲容器。

### 3.3 VM 准入条件

- 不受信第三方代码或工具；
- 可能利用漏洞、需要真实浏览器/内核/设备交互或高风险逆向分析；
- 需要与宿主共享内核的 `podman_host` 无法提供的隔离边界；
- 明确的安全策略要求 VM 级边界。

VM 是高风险执行后端，不是所有工具的基础设施前提。既有 Phase 2 VM 设计保留为可用隔离能力，具体是否启用按任务风险决定。

## 4. 统一执行架构

```text
RIP Platform (native host processes)
  ├─ API / Agent / Workflow / Research / WSW Proxy
  ├─ Tool Registry + Policy Resolver
  └─ Tool Engine + Runtime Gateway
       ├─ NativeRustAdapter ──> allowlisted native executable
       ├─ PythonWorkerAdapter ──> project venv / worker process pool
       ├─ PodmanHostAdapter ──> rootless tool-family container
       ├─ PodmanVmAdapter ──> VM runner (when implemented/enabled)
       ├─ McpClient / HttpApi / Cli adapters
       └─ shared ToolRun / Artifact / Evidence / Trace / budget / cancel
```

Runtime 选择由 Registry Manifest + Tool Policy Resolver 决定。Agent/Workflow 不得自行选择任意宿主命令、镜像或挂载路径；执行后端负责落实策略，不能让 UI 直接拼接执行命令。

### 4.1 必须统一的行为

- Run 状态、幂等、预算、取消、超时、重试、输出截断与错误信封统一。
- Artifact 的 hash、大小、来源、run_id 和 partial 状态统一。
- Evidence/Trace 的关联与查询统一。
- 网络策略和凭据注入由平台统一控制；容器与原生执行不得各自实现一套权限事实。
- Runtime 健康检查、能力声明和版本信息统一暴露。

### 4.2 不允许的做法

- 不为原生 Runtime 新建第二个 Tool Registry、Agent、Workflow Engine 或 Evidence 数据库。
- 不把任意 Shell、任意 Python 代码或任意二进制路径开放给 Agent。
- 不把生产 `platform*.db`、RTIS 私有数据、用户浏览器 Profile、SSH/Podman Socket 或宿主家目录挂入工具容器。
- 不默认把平台服务、Rust crate、Cargo 构建、测试和 CI 放入容器。
- 不为了实现原生优先而降低不受信工具的隔离等级。

## 5. Python 与其他语言依赖管理

1. 可信 Python Worker 优先使用仓库/Worker Pack 自己的虚拟环境（例如项目级 `.venv` 或等价隔离环境），依赖通过 lock 文件固定；禁止全局 pip 安装。
2. 仅因“Python 依赖多”不足以判定必须容器化。若依赖能在项目级 venv 稳定安装、运行和测试，且 Worker 可信，可原生执行。
3. 当依赖包含冲突的系统库、GPU/浏览器运行时、重量级二进制或明显不可信第三方代码时，才使用容器/VM。
4. Node/Java/Go 同样先评估项目级版本管理和本地隔离；不要把运行时管理与安全隔离混为一谈。
5. Python Worker Protocol v1 保持唯一协议；容器 Worker 与原生 Worker 继续复用相同协议，不分叉业务语义。

## 6. Cargo 构建产物与磁盘治理

### 6.1 目标

- 把 `target/` 视为可再生的本机构建缓存，不纳入容器镜像、不入 Git、不作为交付资产。
- 控制 Debug 信息体积，同时保留可用的调试/回溯能力。
- 区分开发定向测试与阶段收口全量测试，减少无必要的重复构建。
- 不通过删除测试、放宽 CI、减少 Clippy 范围或移除依赖来“优化”磁盘。

### 6.2 先做 A/B，再决定 Cargo Profile

当前根 `Cargo.toml` 只有 `[profile.release] strip = "debuginfo"`，尚无 workspace 级 `[profile.dev]` / `[profile.test]`。建议候选配置：

```toml
[profile.dev]
debug = 1

[profile.test]
debug = 1
```

该配置**暂不直接写入 Cargo.toml**。先在独立、可删除的 probe target 目录，对代表性 crate（至少 `wsw-message`、`context-broker`、`tool-engine` 中选择小/中/大目标）比较默认 profile 与 `debug=1`：

- 构建时间、测试时间；
- target 子目录总字节数；
- 测试失败时的 backtrace / `RUST_BACKTRACE=1` 可诊断性；
- Clippy 与测试结果一致性。

只有实测空间收益明显、诊断能力可接受、测试全部通过，才在独立批次调整 Profile。不得在 WSW-1 未收口时改其 Cargo 配置或锁文件。

### 6.3 构建与测试节奏

- 日常改动：`cargo fmt --check` + `cargo check -p <affected-crate> --offline` + `cargo test -p <affected-crate> --offline` + 对应 Clippy。
- 跨 crate 契约变更：增加直接依赖 crate 的定向测试。
- 阶段/批次收口：按任务书执行完整 `cargo test --offline --workspace` 与 `cargo clippy --offline --workspace --all-targets -- -D warnings`。
- GitHub CI 继续保留完整 Workspace 测试和 Clippy 门禁；不得为了本机省空间而缩小 CI 覆盖。
- 不在每个编辑循环都重复跑全 Workspace；若必须重跑，记录触发原因。

### 6.4 构建缓存与清理

- 先检查 `target/debug/deps` 最大文件、文件类型、构建指纹、最近访问/修改时间和 Cargo target/profile 组合。
- 优先使用 Cargo 自身的增量失效机制；不得用脚本按文件名猜测删除“重复 hash”产物。
- `cargo clean` 仅在用户明确授权或独立维护任务中执行；执行前记录 `du`、`df`、当前构建状态和可复建条件。
- 不将 `target/` 移入 tmpfs；本机 `/tmp` 是有限 tmpfs，不适合作为 70 GiB 级构建目录。
- 如需独立 target-dir，使用稳定 SSD 路径并按 workspace/profile/目标平台控制数量；避免每个 Agent/任务创建永久独立 target 目录。
- 不启用未经验证的全局 `RUSTFLAGS`、`-C strip` 或跨 crate LTO；这些设置可能改变指纹、触发全量重编译或影响调试。

## 7. 容器镜像与存储治理

- 镜像按工具族复用，避免一个巨型镜像，也避免同一运行时重复打包多份。
- 仅为实际登记且近期要执行的工具构建/拉取镜像；planned Manifest 不自动触发镜像构建。
- 保留 digest、构建来源、许可证、SBOM/依赖清单和更新时间。
- 运行容器使用 rootless、非 root 用户、只读 rootfs、最小挂载、capabilities 收敛、CPU/内存/PID/时间/输出上限；默认网络关闭，出网走明确策略。
- 不挂载 Docker/Podman socket；不挂载宿主家目录、生产库、RTIS、凭据目录。
- 对镜像、容器、卷、构建缓存分别计量，不用单一 `podman system df` 总数代替归因。
- 预算触发先报告具体镜像/标签/容器/卷归属，再清理明确由 RIP 创建且已无引用的对象；禁止无差别 `podman system prune -a --volumes`。
- 当前 ADR 的 50 GiB 是历史预算上限，不是容器必须占满的目标；后续按实测镜像族重新定预算与告警阈值。

## 8. 对既有 ADR 与当前 WSW 的关系

### 8.1 历史 ADR 的处理

`TOOL-RUNTIME-ADR.md`、`PHASE2-DESIGN.md` 和 `PHASE2-ACCEPTANCE.md` 保留，不改写历史验收事实。本文仅修订“未来默认 Runtime 策略”：

- Phase 2 已验收的 `podman_host` 能力继续保留，作为需要隔离的工具后端。
- Phase 2 `podman_vm` 目标设计继续保留，作为高风险执行隔离后端；不再宣称所有工具最终都必须迁入 VM。
- `python_worker` 保持现有协议和派发语义，后续按可信度/依赖复杂度分类部署。
- `native_rust` 从“仅 Manifest 可登记”补齐到可控的真实 Adapter，完成前不把它描述为已落地能力。

### 8.2 WSW-1 不受影响

当前工作树已有 WSW-1 未提交修改（本轮开工核验：`Cargo.toml`、`Cargo.lock`、`crates/wsw-message/`、`migrations/023_wsw_body_blob.sql`；该快照截至 2026-11-18 上午——WSW-1 已于 077a8dd4（2026-11-18 19:18）收口提交，上述文件已在收口提交内，后续以前置行收口口径为准，勿据本句误判 WSW-1 未收口）。这些文件不属于本策略文档修改范围，必须保留。

WSW-1 当前任务书的八条红线继续生效，尤其是零 vendor 增量、不改变 tokio feature 面、不碰生产库、不建立第二套事实体系。不得借本 Runtime 策略调整 WSW-1 的依赖、迁移或实现范围。

WSW Proxy/MITM 是 RIP 自身可信平台服务，按 Rust-First 规划原生运行；它不是待容器化的第三方工具。只有它调用的高风险外部工具才按工具风险选择容器/VM。

## 9. 分阶段实施计划

| 批次 | 内容 | 交付 | 前置/阻断关系 |
|---|---|---|---|
| RT-0 | 只读基线审计 | Runtime Manifest 分类表、Adapter 能力矩阵、Podman 镜像/卷归属、Cargo 构建体积基线 | 本文档；不改代码 |
| RT-0.5 | Host Capability Discovery | 宿主软件只读盘点、能力目录、安全探测与文档处理路由设计；不安装软件、不改 Manifest，详见 RT-0.5-HOST-CAPABILITY-DISCOVERY.md |
| RT-1 | Native Rust Adapter 设计与最小实现 | 固定 binary/subcommand allowlist、受控环境/工作区/超时/取消/输出上限、统一 ToolRun 信封 | RT-0、RT-0.5 通过；不允许任意 Shell |
| RT-2 | Python Worker 原生环境治理 | 项目级 venv、lock、启动探测、版本/哈希记录、协议一致性测试 | 保持 Worker Protocol v1；不做全局 pip |
| RT-3 | Tool Runtime Policy Resolver | 按信任等级、依赖复杂度、隔离要求选择 native/python/container/VM；拒绝不安全降级 | RT-1、RT-2 |
| RT-4 | 工具 Manifest 逐项迁移 | 对每个工具给出原生/容器/VM 的理由和实测，分批迁移 | RT-3；不批量改写 Manifest |
| RT-5 | Cargo Profile A/B 与磁盘治理 | profile 对比报告、debug=1 决策、缓存/清理流程 | 与 WSW-1 分离；先 probe 后改配置 |
| RT-6 | Podman 资源与镜像预算 | 按镜像族/卷/容器归属的报表、阈值告警和安全清理流程 | RT-0 基线 |
| RT-7 | CI 与发布策略核验 | 原生构建矩阵、完整测试/Clippy 保持、容器工具仅在需要时构建 | 不缩小现有质量门 |

RT-0 基线已完成；RT-0.5 与后续 RT 实施须在当前 WSW-1 稳定收口后启动。RT-1 以 RT-0.5 的宿主能力目录和安全探测设计为前置输入；RT-1 至 RT-7 按依赖顺序推进，不插入 WSW-1 当前工作树，不改变 WSW-1 → WSW-2 的既定批次顺序。Runtime 迁移作为独立维护轨，不得暗中扩大 WSW 子批次范围。

## 10. 验收标准

- [ ] 文档与 `AGENTS.md` 明确 Native-first、按风险隔离，且没有把原生执行等同安全沙箱。
- [ ] 现有 `python_worker`、`podman_host`、Tool Registry、ToolRun/Artifact/Evidence/Trace 语义保持兼容。
- [ ] Native Rust Adapter 有 allowlist、参数 Schema、环境白名单、超时/取消、进程回收、输出限制和安全测试。
- [ ] 不受信工具无法选择 native/python 路径绕过隔离策略。
- [ ] 可信 Python Worker 在项目级 venv 原生运行的正反例和依赖锁定测试通过。
- [ ] Manifest 分类有证据，不进行“一律容器化”或“一律原生化”的批量迁移。
- [ ] Cargo Profile A/B 报告包含磁盘、时间、诊断能力和测试结果；没有未验证就改 Profile。
- [ ] GitHub CI 完整 Workspace 测试与 Clippy 保持原有范围。
- [ ] 生产 `platform*.db`、RTIS 私有数据和现有用户容器零误触。
- [ ] Podman 清理只作用于确认归属 RIP 且无引用的对象，不执行全局 prune。

## 11. 本轮变更边界

本轮允许的变更：新增本文策略文档、在 `AGENTS.md` 增补执行规则、在 `docs/README.md` 增加索引。

本轮禁止的变更：修改 Cargo manifests/lock/vendor、修改 Runtime Adapter 业务代码、修改数据库迁移、修改现有 Manifest、修改 WSW-1 任务书或工作树文件、清理 `target/`、清理 Podman 对象、commit/push。


## 12. Host Software First：优先复用宿主机成熟软件

Native-first 不仅表示 Rust/Python Worker 可以在宿主机运行，也表示 RIP 应优先复用宿主机已经安装、来源可信、版本与能力满足要求的成熟软件，而不是为了“统一环境”重复打包。

- FFmpeg/FFprobe、LibreOffice/soffice、Tesseract、Poppler/MuPDF、Pandoc、ImageMagick、7-Zip/bsdtar、ExifTool 等列入 Host Capability Discovery 候选目录；此列表不是当前机器已安装证明，必须逐项实测。
- 对媒体、OCR、Office、PDF、图像、归档与元数据处理，Rust 负责路由、策略、预算、任务生命周期和统一结果；成熟软件负责专业解析/转换，不为追求 Rust 化而重写成熟工具。
- 宿主能力存在且满足版本/子能力要求时，优先受控原生子进程；可信 Python Worker 可使用项目级 venv。依赖冲突或重型运行时才使用专用容器；不可信代码或高风险输入按威胁模型使用 Podman/VM。
- “系统已安装”不等于“可以无约束处理任意输入”。Office/PDF/图片/压缩包/媒体解析都可能受到恶意文件攻击，仍须依据输入来源和风险施加只读输入、独立输出目录、非特权身份、CPU/内存/时长/页数/像素数/解压大小/输出上限与网络限制。
- Host Capability Discovery 必须验证绝对路径、文件身份、权限、包来源、版本和可选能力；不能盲目信任 PATH、当前目录或用户可写目录中的同名程序。探测只读、固定参数、短超时、有输出上限，并缓存指纹与失效状态。
- 缺失能力只报告与提供替代方案，不自动安装/升级/卸载系统软件；禁止全局 pip 和未经授权系统级安装。

### 12.1 RT-0.5 前置设计

RT-0.5 专门完成宿主软件只读盘点、能力目录、探测安全模型、文档智能处理路由与 Runtime Policy 输入设计。详细任务、数据模型、探测层级和验收标准见 [RT-0.5-HOST-CAPABILITY-DISCOVERY.md](RT-0.5-HOST-CAPABILITY-DISCOVERY.md)。RT-0.5 在 WSW-1 稳定收口后执行，作为 RT-1 的设计前置；本批不安装软件、不迁移 Manifest、不实现 Adapter。
