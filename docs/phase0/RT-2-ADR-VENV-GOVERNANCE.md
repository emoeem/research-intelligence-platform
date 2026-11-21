# RT-2 ADR：Python Worker venv 治理设计裁决

- 日期：2026-11-21
- 状态：已定稿（RT-2 设计裁决；实现口径冻结，对应实现任务书 [RT-2-TASK.md](RT-2-TASK.md)）
- 依据：[NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md) §3.1（64-76 行）原生准入条件、§5（132-138 行）Python 依赖管理五条、§9:224 RT-2 行、§10:239 验收行、§4.2（126-130 行）不允许的做法；[RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §2/§4/§5.1/§5.3（条目形态与指纹公式）；[RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §2（18-93 行）数据结构；[RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md) D3/D4/D7/D8（钉值/env/五态/进程范式先例）；[RT-0.5-D-TOOL-MAPPING.md](RT-0.5-D-TOOL-MAPPING.md):5、24-36（16 份 python_worker 保持 → RT-2 venv 裁决）
- 代码现状锚点（2026-11-21 本会话只读实测，含命令原文）：见 §A 现状锚点表
- 决策记录形态：D1 到 D8，每条含裁决、论据（引用在位证据与本会话实测）、否决项

## A. 现状锚点（2026-11-21 本会话实测）

| # | 锚点 | 事实 | 出处 |
|---|---|---|---|
| A1 | 16 份 python_worker Manifest 全部裸 `python3` 派发 | `command: ["python3", "-I", "workers/…py"]`，无一份指向 venv 解释器 | manifests/tools/rip.doc.parse.json:13-17、rip.text.embed.json、rip.kb.rtis.json、rip.ops.worker-runtime.json（本会话逐一读取） |
| A2 | 派发器现状 | `Command::new(&self.config.command[0])` 裸名走 PATH 解析；env 只做 `cmd.env(k,v)` 追加（父环境全量继承）；握手 worker_name/能力面校验已齐 | crates/tool-engine/src/adapter/python_worker.rs:253、261-263、314-357 |
| A3 | 仓库级 venv 已存在且在工作 | `workers/python/.venv`：Python 3.14.10 + pip 26.2.2；包面 numpy 2.5.4 / onnxruntime 1.29.0 / tokenizers 0.23.3 / huggingface_hub 2.4.0 等（`pip list` 实测）；gitignore:16 已忽略 | 本会话 `./workers/python/.venv/bin/python -m pip list`、`.gitignore:16` |
| A4 | venv 身份漂移（三条实据） | ① 两份 `.venv/pyvenv.cfg` 的 `command` 均记录 `/home/emo/code/pensting/...`（仓库已迁至 `/media/emo/...`）；② `version = 3.14.9` vs 基础解释器现值 3.14.10（建 venv 后宿主解释器已升级）；③ `workers/python/.venv` numpy 2.5.4 vs 系统 `python-numpy 2.5.5-1.1`（pacman） | 本会话 `cat workers/python/.venv/pyvenv.cfg`、`cat rip-data/plugins/installed/tool.rip.python-workers/0.1.0/.venv/pyvenv.cfg`、`pacman -Qo`、`pip list` 对比 |
| A5 | 系统 python3 与 venv 的依赖分裂 | 系统 `/usr/sbin/python3`（3.14.10）：numpy 2.5.5 + onnxruntime 1.29.0 由 pacman 包提供（`python-numpy`/`python-onnxruntime-opt-cuda`，非 pip），**tokenizers 缺失**（`ModuleNotFoundError` 实测）→ 系统解释器下 embedding_worker 只能走 hash-fallback | 本会话 `python3 -I -c "import tokenizers"`；workers/python/embedding_worker.py:22、145-161（延迟 import + fallback 标注） |
| A6 | Worker Pack 侧 venv = 无治理快照 | `rip-data/plugins/installed/tool.rip.python-workers/0.1.0/.venv` 整目录字节复制（checksums.sha256 含 8273 条 `.venv/` 条目；pyvenv.cfg 与 A2 ①② 同漂移）；plugin-runtime 源码 grep `.venv` 零命中（复制来自通用目录拷贝 paths.rs:333，非 venv 感知流程） | 本会话 `head checksums.sha256`、`grep -c venv`、crates/plugin-runtime/src/paths.rs:263、333 |
| A7 | 依赖声明面现状 | workers/python 无任何 requirements/lock 文件（全仓 `find -name "requirements*.txt"` 仅命中 containerfiles/rip-parser/requirements.txt——那是容器镜像面，ADR-0002 台账，torch/docling 家族，与本批无关）；Manifest 依赖仅 provenance.dependencies 自由文本（rip.text.embed.json:139「onnxruntime…位于仓库 workers/python/.venv」；rip.doc.parse.json:144 `[]`） | 本会话 find + manifests 读取 |
| A8 | 校验面现状 | validate.rs python_worker 分支只校验 protocol_version/worker_name/command/restart 四字段（755-781 行）；v1.json python_worker 分支 required 同三字段（140-155 行） | crates/tool-registry/src/validate.rs:755-781、schemas/tool-manifest/v1.json:140-155 |
| A9 | 接线点 | 生产构造点实勘三处：mcp-server/src/dispatch.rs:925（`build_python_adapter`）与 agent-engine rp-agent.rs:320 从 Manifest runtime.command 直接构造 `PythonWorkerConfig`、无任何解释器解析层（本批两处接线 runtime.venv）；workflow-engine rp-workflow.rs:1023（`resolve_python_worker_value`）hardcode `venv_config: None` 且不读 `runtime.venv`——本批不接线，记为缝 5（D8）；另有测试文件构造点 5 处（4 个测试文件，additive 补 None） | 本会话 `grep -rn "PythonWorkerConfig {" crates/` 全仓实测 |
| A10 | lock 工具实测 | `pip-compile`（pip-tools）**缺失**；`uv` 0.13.9 在位（pacman 包 uv 0.13.9-1.1，/usr/bin/uv）；`uv pip compile requirements.in --generate-hashes -o requirements.lock.txt` 退出码 0，解析 20 包、产 239 条 `--hash=sha256:` 行的 pip 消费型文件；无约束解析漂移实证：解析出 numpy 2.5.5 / onnxruntime 1.32.0 ≠ 当前 venv 2.5.4/1.29.0；`python3 -m venv` 裸建（3.14.10）实测成功；系统/venv pip 均 26.2.2 | 本会话 scratch 实测（/tmp 临时目录，已清理，零安装动作） |
| A11 | CI 分工现状 | python-workers job = setup-python **3.12 裸解释器** + `python -m unittest discover`（零第三方依赖安装）；Rust job 全 `--offline` | .github/workflows/ci.yml:85-101、52、55 |
| A12 | 五态/映射先例 | AdapterOutcome 四态 + map_worker_code 码表（-32002→TIMEOUT 等）+ 信封九类；RT-1 D7 已定 ProbeError 五态→九类映射表 | crates/tool-engine/src/adapter/mod.rs:32-64、90-102；[RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md) §D7 |

## D1：venv 落位 = 仓库级 `workers/python/.venv` 单例；每 Worker Pack 独立 venv 否决

**裁决**：可信 Python Worker 的项目级 venv 落仓库级单例 `workers/python/.venv`（现状已在位，A3）。Worker Pack 分发面**排除 .venv**：pack 只携带源码 + checksums.sha256/plugin.lock 身份面，目标机的 venv 由 lock 文件 + 供给命令按 D2 重建，不再快照复制。

**论据**：
- 依赖面事实：六个 worker（parsing/embedding/evaluation/catalog/extension_audit/tls_inspect + runtime 基座）共享 `worker_runtime.py` 基座（workers/python/runtime/worker_runtime.py），直接依赖集收敛为 numpy/onnxruntime/tokenizers 三件（A5/A7），无任何冲突性系统库——正是 NATIVE-FIRST §5.1「仓库/Worker Pack 自己的虚拟环境（例如项目级 `.venv` 或等价隔离环境）」与 §5.2「仅因 Python 依赖多不足以判定容器化」的原生档标准形态。
- Manifest 面事实：16 份 python_worker Manifest 全部 `working_dir: "repo://research-platform"`（A1），仓库根单一定位面已统一，仓库级 venv 与之零阻抗。
- per-pack venv 现状即反例：pack 的 `.venv` 是通用目录拷贝的意外快照（A6：8273 条 checksums、pyvenv.cfg 携带 /home 旧路径与 3.14.9 旧版本），无人治理、随安装时刻漂移——把 venv 复制进 pack 不是「每 Worker Pack 独立隔离」，是「每个 pack 一份无人负责的过期快照」。
- per-pack venv 的收益（依赖互斥）在本仓不存在：全部 worker 可信自研、依赖集同族，拆分只会倍增 lock/供给/探测面，不增加任何安全边界（venv 不是安全隔离，NATIVE-FIRST §5.4「不要把运行时管理与安全隔离混为一谈」）。

**否决项**：
- 每 Worker Pack 独立 venv（`installed/<id>/<version>/.venv` 按包重建）：现状快照漂移反例 + 收益为零，否决；pack 打包面排除 .venv 作为本批动作之一（S2）。
- 容器化替代（依赖装进镜像）：NATIVE-FIRST §5.3 明文「依赖包含冲突的系统库、GPU/浏览器运行时、重量级二进制…才使用容器」，本仓 worker 依赖面不满足，否决。
- 系统 site-packages 直装（pip 全局）：§5.1 明文「禁止全局 pip 安装」，否决。

## D2：lock 选型 = `uv pip compile --generate-hashes` 产 pip 消费型 lock 文件；pip-tools 与 uv.lock 均否决

**裁决**：lock 供给链为三件套，全部入 git：

1. `workers/python/requirements.in`：直接依赖**钉定到当前 venv 实测工作版本**（首版：`numpy==2.5.4`、`onnxruntime==1.29.0`、`tokenizers==0.23.3`，A3 实测值；禁止无约束解析——A10 已实证无约束 `.in` 会解析出 numpy 2.5.5 / onnxruntime 1.32.0 的静默升级面）。
2. `workers/python/requirements.lock.txt`：`uv pip compile requirements.in --generate-hashes -o requirements.lock.txt` 产物（全传递闭包 + 每包 `--hash=sha256:` 多值，A10 实测 20 包 239 hash 行），uv **只在锁定时**需要（宿主作者动作）。
3. 消费/安装面 = 裸 pip：`python3 -m venv <root>` + `<root>/bin/python -m pip install --require-hashes -r requirements.lock.txt`，零工具依赖（pip 26.2.2 系统与 venv 均在位，A10）。

**论据**：
- 本机可用性实测（A10）：pip-tools 缺失——「不许预装新工具」约束下 pip-compile 路线需先装机，直接出局；uv 0.13.9 为 pacman 包（uv 0.13.9-1.1）已在位，`--generate-hashes` 一次通过。
- CI 离线/裸解释器约束（A11）：CI python job 是裸 3.12 零安装面，**不消费 lock 安装**；lock 文件只需可被校验（S6 的纯 stdlib 静态完整性校验）。若选 uv.lock 专有格式，CI 与任意消费环境都必须有 uv 二进制才能做任何校验/安装，凭空引入工具依赖；`uv pip compile` 输出的恰好是 pip 原生 `--require-hashes` 格式，lock 内容与消费工具解耦——这是本机可用性与 CI 约束双满足的唯一组合。
- 哈希钉定满足 §3.1:68「第三方包必须记录来源、版本、许可证和 SHA-256/发布校验信息」与 NATIVE-FIRST §9:224「lock」的发布校验要求：wheel sha256 全量入 lock，供给面 `--require-hashes` 强制。

**否决项**：
- `pip-compile`（pip-tools）：本机缺失，预装违反任务红线（S 红线 1），否决。
- `uv.lock` 专有格式：消费面锁死 uv 二进制（CI 无 uv，引入 setup-uv 步骤即新增依赖面），否决。
- 无 lock 裸 requirements.txt（无 hash）：§3.1:68 哈希校验要求落空，否决。
- 以 `uv lock`（pyproject 项目态）替代：同 uv.lock 否决理由，且把 workers/python 变成 uv 项目面改动更大，否决。
- 版本不钉、随解析浮动：A10 实证静默升级面，否决。

## D3：探测面挂 RT-0.5 目录范式（L0/L1/L2 + 指纹公式实例化 + 五态沿用）；venv 状态 = 环境事实，禁入 Tool Registry

**裁决**：新增 venv 探测面（落点见 D5），数据形状与流程对齐 RT-0.5-C/F 范式：

- 探测分层沿用 C §5.2：**L0** = 路径/身份静态面（`pyvenv.cfg` 可读、`bin/python` 存在、symlink 目标解析、解释器文件全量 sha256、owner/mode）；**L1** = 版本面（`pyvenv.cfg` version 字段 + 解释器实际版本对拍，漂移即 incompatible 证据——A4② 即现网实例）；**L2** = 依赖集面（`site-packages/*.dist-info` 目录名解析出 name+version 排序集 → capset）。L3 smoke（真实 embed 推理）不进常规探测，留工具启用时（C §5.2 L3 同口径）。
- 指纹公式实例化 C §5.1（261-267 行冻结公式）：`fingerprint = sha256(canonical_json({bin_sha256: 解释器全量 sha256, version: 解释器版本, capset: 依赖集排序 canonical}))` 前 16 hex；venv/bin/python 为 symlink → wrapper_target 先例（C §5.1:270、F §2 BinaryIdentity.wrapper_target）要求同时记录 symlink 目标身份。
- 五态沿用 C §4：available / missing / incompatible / probe_failed / permission_denied（ProbeErrorKind 五态，F:126-132）；`disabled` 本批不产出（C §3.17「disabled 属 Tool Registry 工具事实，不属本目录」同口径）。
- **事实边界（硬性）**：探测结果只进探测面自身缓存与批次报告，不进 Tool Registry、不进 Manifest、不建第二事实源——C §6 职责边界表原文「目录条目不携带工具语义，Registry 条目不携带环境探测事实」；stale 语义沿用 C §5.3（pyvenv.cfg/解释器/依赖集/lock 文件任一 sha256 变化即 stale，stale 不可供派发判定使用，fail-closed 重探）。
- host-catalog-seed.json 本批**零改动**（RT-1 已入库 16 条为准；python3/venv 条目并入目录留 Provider 演进缝，见 §D8 边界注）。

**论据**：
- python3 解释器不在 RT-0.5-C 十六条内（该目录覆盖媒体/OCR/文档/归档域），但 C §2 字段映射表（capability_id/provider/executable_path/version/binary_identity/package_provenance/capabilities/status/probe_timestamp/fingerprint/constraints/security_profile）是现成的条目形态——RT-2 探测面按同一形态承载 `runtime.python3-venv` 等价身份，范式对齐而不动 RT-1 种子。
- A4 的三条漂移实据（/home 旧路径、3.14.9 vs 3.14.10、numpy 2.5.4 vs 2.5.5）证明「venv 在 ≠ venv 可信」：无探测面时这些漂移全部不可见，D2 的 lock 在供给后同样需要 L2 指纹对账防止装完漂移。

**否决项**：
- 探测结果写 Tool Registry / 工具 Manifest：第二事实源（NATIVE-FIRST §4.2:126），否决。
- 把 venv 条目并进 host-catalog-seed.json：改 RT-1 产物语义，且 seed 是静态转录面而 venv 是动态重建面，生命周期不同，否决（留 Provider 演进缝）。
- spawn `pip freeze` / `pip list` 取依赖集：依赖 pip 子进程（pip 升级会改变输出形态）+ 每次探测拉解释器进程；改用 dist-info 目录名静态解析（PEP 376 标准 `name-version.dist-info`），离线、零子进程，否决 spawn 面。

## D4：解释器解析优先级 = Manifest 显式绝对路径 > venv 探测 > 系统 python3 固定探测值；裸名不做 PATH 任意解析（仅对声明 venv 的 Manifest 强制）

**裁决**：`PythonWorkerAdapter` 派发前对 `command[0]` 做解释器解析（`resolve_interpreter`），优先级三档：

1. **Manifest 显式**：`command[0]` 为绝对路径（`/` 前缀）→ 直接使用并纳入身份记录（RT-1 D3「可执行路径唯一来源 = Manifest 固定绝对值」同精神）。
2. **venv 探测**：Manifest 声明 `runtime.venv`（D5 additive 声明块）→ 探测 venv；`available` 且 direct_deps 覆盖 → `command[0]` 解析到 `<venv>/bin/python` 绝对路径；探测非 available → **PREPARING 拒绝（fail-closed，不回退）**。
3. **系统 python3**：未声明 venv 的 Manifest（现状 16 份全部如此）保持现状行为——裸名 `python3` 由现有 `Command::new` 语义解析（A2），**本批不改**（零回归红线）；探测面以 L0 固定值（`/usr/sbin/python3`）记录系统解释器身份供信封/报告引用，不构成派发强制。

**论据**：
- additive 演进纪律沿用 RT-1 D3 的「登记面兼容、执行面强制」分档：钉值补录/venv 声明补录属 RT-4 逐项迁移（RT-0.5-D:24-36 已把 16 份工具逐项裁决为「保持 python_worker → RT-2 venv」，迁移动作在 RT-4 按该表逐项做，本批只建承载面）。
- fail-closed 对齐 NATIVE-FIRST §4.2:130「不为了实现原生优先而降低不受信工具的隔离等级」与 E §2「能力查找失败路由终止不降级」：声明 venv = 声明「我要锁定依赖集」，探测不过还回退系统解释器就是把 lock 语义静默作废（A5 的 tokenizers 缺失即真实回退形态：系统解释器下 embedding 恒为 hash-fallback，质量静默降级）。
- 现状裸名 PATH 解析是已识别暴露面（A2），但收紧它必须同时改 16 份 Manifest，超出本批 additive 边界——记为缝（§D8 边界注 3），RT-3 Policy Resolver / RT-4 迁移时随逐项登记收敛。

**否决项**：
- 声明 venv 但探测失败时回退系统解释器（prefer 模式）：静默质量降级 + lock 语义作废，否决；拒绝是唯一路径。
- 本批全局收紧裸名 `python3` 的 PATH 解析：改 16 份现有 Manifest 行为面，违反 additive/零回归红线，否决（记录为缝）。
- 以 cwd 相对路径（`workers/python/.venv/bin/python`）作派发值：进程 cwd 可变，非绝对派发，venv 探测面内部归一为绝对路径后使用，否决相对形态。

## D5：接线点 = PythonWorkerConfig additive 字段 + 两处构造点接线；声明块 schema additive，登记面兼容

**裁决**：

- 声明块（schemas/tool-manifest/v1.json python_worker 分支 additive 可选字段 + validate.rs 同规则）：

```jsonc
"runtime": {
  "type": "python_worker",
  "protocol_version": "1",
  "worker_name": "embedding-worker",
  "command": ["python3", "-I", "workers/python/embedding_worker.py"],
  "venv": {                                        // additive 可选对象；在场任一字段即触发执行面强制
    "path": "workers/python/.venv",                // 必填；相对 working_dir 或绝对路径；禁 ".."
    "interpreter": "bin/python",                   // 可选；venv 根内相对路径（禁绝对路径与 ..——绝对值在 Path::join 下整体替换 base 逃出 venv 根，见下「三面冻结」），缺省 bin/python
    "requirements_lock": "workers/python/requirements.lock.txt",  // 可选；lock 文件路径（working_dir 相对），在场即做 sha256 对账
    "direct_deps": ["numpy", "onnxruntime", "tokenizers"]         // 必填非空数组；capset 覆盖检查触发器
  }
}
```

- 执行面（crates/tool-engine/src/adapter/python_worker.rs additive）：`PythonWorkerConfig` 增 `venv_config: Option<VenvConfig>` + `venv_identity: Option<VenvIdentity>`；`ensure_ready()`（PREPARING 期，python_worker.rs:102-116）前置 venv 探测，探测失败按 D7 五态映射拒绝；spawn 用解析后绝对解释器路径替换 `command[0]`（argv 其余项原样，`-I` 保留——隔离模式语义不变）。
- **interpreter 相对性三面冻结**（登记面/探测面/schema 三处同规则）：① validate.rs 拒绝 interpreter 绝对路径（`/`、`\`、盘符前缀，`..` 穿越既有检查保留）；② venv.rs probe 纵深防御把 interpreter 纳入同款检查（复用 UnsupportedOutput 拒绝态）；③ v1.json 补 pattern（首字符禁 `/`/`\` + 盘符/`..` 段 not-pattern）。依据：`root.join(config.interpreter_rel())`（venv.rs:298）在 interpreter 为绝对路径时按 std `Path::join` 语义整体替换 base——派发目标逃出 venv 根而 capset/lock 对账仍按 venv 面通过，「声明 venv = 锁定依赖集」被旁路，正是 D4 否决项 1「回退系统解释器」要堵的静默降级面的旁路形态。sk3 负例（`interpreter=/usr/bin/python3` 必须拒绝）+ venv.rs 单测双守护。
- 构造点接线（A9 生产三处中的两处）：mcp-server `build_python_adapter`（dispatch.rs:925）与 rp-agent.rs:320 从 Manifest `runtime.venv` 构造 `VenvConfig` 并注入；未声明 venv 的 Manifest 走现状路径零改动。第三处 rp-workflow.rs:1023 不接线（缝 5，D8）。
- 探测模块：`crates/tool-engine/src/venv.rs`（VenvIdentity/VenvConfig/VenvProbe + 五态 + D3 指纹公式），纯 `std::fs` 读面（D3 否决项 3 同款），零新依赖（Cargo.lock 399 恒定红线）。

**论据**：
- 接线面封闭性：A9 实测生产构造点三处 + 测试文件构造点五处（4 个测试文件）；本批接线其中两处（dispatch/rp-agent），rp-workflow 构造点 venv 读取接线留缝 5（RT-3/RT-4 承接，验收断言随缝登记），缝外零隐藏构造点——接线面封闭、可测。
- `-I` 保留：worker 纪律是脚本级隔离（isolated mode 禁 site 与用户目录注入），venv 解释器下 `-I` 仍生效且 venv site-packages 属解释器安装面不受 `-I` 影响——`-I` 隔离的是用户环境面不是 venv 安装面（venv 依赖通过绝对解释器路径的默认 sys.path 进入，`-S` 才会关掉它，本批不用 `-S`）。这是用 venv 解释器 + `-I` 与现状唯一需要论证的组合面，S6 对拍测试覆盖。

**否决项**：
- 在 adapter 外建独立 venv-resolver 服务/进程：探测是 L0/L1/L2 静态读面，进程化无收益，否决。
- 用 shell 包装（`source venv/bin/activate && python3 …`）做激活：activate 脚本只是环境变量面，绝对路径调用 venv 解释器等价且无 shell 面（禁 Shell 红线同 RT-1 D3），否决。
- `VIRTUAL_ENV` 环境变量注入替代路径替换：变量注入改变 env 语义面（A2 env 继承现状），路径替换是零 env 变更的最小解，否决。

## D6：版本/哈希记录 Schema = VenvIdentity 对齐 RT-0.5-F §2 条目形态；批次报告承载，不进 Registry

**裁决**：`VenvIdentity`（venv.rs）字段冻结如下，对齐 F §2 CapabilityEntry/BinaryIdentity 形态（A12 引用 F:18-93）：

```text
VenvIdentity {
  venv_root: PathBuf,                 // 归一绝对路径
  interpreter_path: PathBuf,          // <venv>/<interpreter> symlink 解析后绝对路径
  interpreter_identity: {             // = BinaryIdentity 形态（F §2）
    sha256_hex: String,               // 64hex 全量
    file_type: String,                // "elf-pie" | "symlink(<target>)"
    owner_mode: String,               // "emo:emo 755" 等
  },
  base_interpreter: PathBuf,          // pyvenv.cfg home/executable
  interpreter_version: String,        // L1 实测（venv/bin/python -V，短超时受控探测）
  python_version_cfg: String,         // pyvenv.cfg version 字段（与上一字段并录，漂移证据面——A4②）
  deps_capset: Vec<(name, version)>,  // dist-info 解析，排序 canonical
  deps_fingerprint: String,           // sha256(canonical_json(capset)) 前 16 hex
  requirements_lock_sha256: Option<String>,   // D5 声明块在场且 requirements_lock 在场时必填
  direct_deps_satisfied: bool,        // direct_deps ⊆ capset 判定
  status: VenvStatus,                 // available|missing|incompatible|probe_failed|permission_denied
  probe_timestamp: OffsetDateTime,
  fingerprint: String,                // C §5.1 公式前 16 hex（D3）
}
```

- 记录去处：进程内探测缓存（stale 失效，C §5.3 语义）+ RT-2 批次验收报告（首探快照全字段实测值落报告，含 lock sha256 与 fingerprint 16 位前缀）；**不入 Tool Registry、不入 Manifest**（D3 硬性边界）。
- 取证口径沿用 C §5.1:269：探测记录 sha256 全量（供指纹复算），指纹只取前 16 hex；16 位前缀不升格为校验钉值（RT-1 D3 否决项 2 同理由）。

**论据**：RT-1 capability.rs 已按 F §2 全字段落地数据结构（crates/tool-engine/src/capability.rs），VenvIdentity 与其同形状使后续 Provider 并入（D3 否决项 2 留的缝）零改形。

**否决项**：把 VenvIdentity 写进 `tool_run` 行或信封新字段：信封九类/字段面冻结（envelope.rs:14-24；NATIVE-FIRST §4.1 统一行为 ≠ 新增字段面），执行环境事实经批次报告与探测缓存承载，否决。

## D7：失败五态映射沿用 RT-1 D7 表（venv 场景判定实例化），不发明新词表值

**裁决**：venv 探测/派发失败五态 → 信封九类映射逐条复用 [RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md) §D7 表，错误码专属化（`VENV_` 前缀，同 `NATIVE_` 前缀先例）：

| 探测态 | venv 场景判定实例 | envelope category | code | retryable | ToolRunStatus |
|---|---|---|---|---|---|
| missing | `<venv>/bin/python` 不存在 / venv 目录缺失 / lock 文件缺失 | DEPENDENCY | `VENV_NOT_PROVISIONED` | false | FAILED（PREPARING 拒绝） |
| permission_denied | venv 根/解释器文件 stat 或读拒绝 | POLICY_DENIED | `VENV_PERMISSION_DENIED` | false | FAILED；不自动提权 |
| timeout | L1 `-V` 受控探测子进程超预算 | TIMEOUT | `-32002`（同码） | false | FAILED（状态机约束：`is_legal(Preparing, TimedOut)=false`，state.rs:95 测试钉死，PREPARING 期探测超时不可能落 TIMED_OUT；category/code/retryable 三轴兑现，实现注记 python_worker.rs:681-691） |
| unsupported_output | pyvenv.cfg 形态不符 / dist-info 名不可解析 / direct_deps 覆盖失败 / lock sha256 对账不符 | EXECUTION | `VENV_IDENTITY_MISMATCH` | false | FAILED（身份不符 ≠ 环境瞬态） |
| probe_failed | L0/L2 读面 IO 异常等其余失败 | INFRA | `VENV_PROBE_FAILED` | true | FAILED |

- direct_deps 覆盖失败（声明依赖不在 capset）与 lock 对账不符归 unsupported_output→EXECUTION：这是「身份与声明不符」的确定性事实，不是基础设施瞬态，retryable=false；与 RT-1 D7「sha256 实测不符 → POLICY_DENIED」的区分理由一致——钉值不符是策略面拒绝（POLICY_DENIED），而 venv 依赖集是自研供给面产物（非宿主第三方二进制），不符属交付物执行缺陷面（EXECUTION），不伪装成策略拒绝。两条路径都 fail-closed，分歧仅在 category 语义归类。

**论据**：五态各一条路径、无兜底合并（RT-1 D7「映射表唯一性保证」原文）；envelope::validate 九类词表为唯一词表（mod.rs:90-102 + envelope.rs:14-24），发明第九类外词表值直接被信封校验拒绝。

**否决项**：`VENV_MISSING` 等自造 category：超九类词表，否决；五态原样透传：同 RT-1 D7 否决项。

## D8：协议一致性对拍与测试分工；五条边界缝显式登记

**裁决**：
1. **协议零分叉对拍**：Worker Protocol v1 语义（schemas/worker-protocol/v1.json + worker_runtime.py 行为）零改动；S6 落对拍测试——同一 stdlib-only fixture worker 分别以 venv 解释器与系统解释器派发，initialize/get_capabilities/execute/timeout/cancel 响应逐字段一致（venv 解释器只是 `sys.executable` 变化，协议面无差异断言）。
2. **CI 分工**：python-workers job 保持 setup-python 3.12 裸解释器零安装（A11），仅 additive 挂 lock 完整性静态校验（纯 stdlib 解析 `requirements.lock.txt`：每个非注释包行 ≥1 个 `--hash=sha256:`、直接依赖集与 `.in` 一致、无 `..` 路径）——CI 不装依赖、不跑 uv、不碰 venv；Rust 探测面测试用 fixture venv（测试内 `python3 -m venv` 临时目录，离线零安装）+ 真实 `workers/python/.venv` 存在性双分支断言（在 → available 正例；不在 → missing 态断言，测试不依赖宿主 venv）。
3. **边界缝显式登记**（本批不做、不静默）：
   - 缝 1：未声明 venv 的 Manifest 裸名 `python3` 仍走 PATH 解析（A2/D4 论据）——收敛随 RT-4 逐项迁移（每份 Manifest 迁移即换成绝对/venv 派发）。
   - 缝 2：`python_worker` env 继承语义（父环境全量继承 + extra_env 追加，A2）本批不改；env_clear 化是安全面扩展，属后续安全批/RT-3，理由：venv 绝对路径调用不需要 PATH/激活变量注入，改动 env 语义有 worker 运行面回归风险，超出 venv 治理范围。
   - 缝 3：venv 条目并入 RT-0.5-C 目录/Provider：留 Provider 演进（D3 否决项 2），并入时按 F §3 trait 形状扩展。
   - 缝 4：16 份 Manifest 的 `runtime.venv` 声明补录属 RT-4（RT-0.5-D:24-36 逐项表为迁移依据）；本批交付承载面 + embed 一份的试点声明**仅存在于测试 fixture**，不落正式 Manifest（零 Manifest 语义改动红线）。
   - 缝 5：rp-workflow.rs 构造点（workflow-engine 派发链，`resolve_python_worker_value`，rp-workflow.rs:1023，A9）hardcode `venv_config: None` 且不读 `runtime.venv`——RT-4 给任一 Manifest 补 venv 声明后，经 workflow-engine 派发的该工具将**静默不触发探测/fail-closed**（D4 fail-closed 语义在该派发链旁路）。现状 16 份 Manifest 零 venv 声明故行为面无差（批次报告偏差 3 显式记账，非静默）。承接：RT-3/RT-4 先行把 `resolve_python_worker_value` 接上同款 `runtime.venv` 读取（dispatch.rs:925/rp-agent.rs:320 同款构造），**验收断言 = workflow 派发链对 venv 声明 Manifest 必须同样探测拒绝**（PREPARING 拒绝落 FAILED + D7 五态信封，零 worker 进程拉起、零系统解释器回退——判据 5 同口径在 workflow 链复验）。
4. **供给命令文档化**：`requirements.in` → `uv pip compile --generate-hashes` → venv 重建 + `pip install --require-hashes` 三条命令写进任务书 §4 S2 与批次报告；供给是作者/部署动作，测试面不依赖网络（红线 7）。

**论据**：A5 的 tokenizers 缺失 + A4 三漂移证明「同一 worker 不同解释器下行为面已经实际分叉」（系统解释器恒 hash-fallback）——对拍测试把「venv 不改变协议语义、只改变依赖可用性」变成机器可判事实；CI 裸解释器分工保持 worker 测试的零依赖可移植性（embedding 双后端 fallback 设计已为此铺路，embedding_worker.py:22）。

**否决项**：
- CI 挂 uv 安装 + venv 供给 + 真实依赖测试：CI 零依赖面是现状纪律（A11），且 GitHub runner 装依赖需网络面扩展，否决。
- 对拍断言 `sys.executable` 相等：平台路径不同是预期差异，对拍面只断言协议响应字段，否决把环境差异当协议差异。

## 生效与关系

- 本 ADR 为 RT-2 实现批次的设计依据；实现任务书 = [RT-2-TASK.md](RT-2-TASK.md)（S1→S8 步骤与八项判据）。
- 与 RT-1 的关系：沿用其进程范式（std::process + 线程 + mpsc）、五态映射（D7 表）、登记面/执行面分档纪律（D3 additive）；不触碰 native_rust.rs。
- 与 RT-3 的关系：Runtime Policy Resolver 按信任等级选择 native/python/container 的判定不属本批；本批交付的 venv 探测五态与 VenvIdentity 是 RT-3 的输入之一；D8 缝 5（rp-workflow 派发链 `runtime.venv` 接线 + workflow 派发链对 venv 声明 Manifest 同样探测拒绝的验收断言）随 RT-3/RT-4 承接。
- 与 RT-4 的关系：16 份 Manifest 的 venv 声明补录与 command 形态迁移按 RT-0.5-D §表逐项执行，本批零 Manifest 语义改动；缝 5 验收断言（workflow 派发链同样 fail-closed）为任一 Manifest 落 venv 声明前的硬性前置——未接线前 RT-4 不得给经 workflow-engine 派发的工具补 venv 声明。
- NATIVE-FIRST §10:239 验收行（「可信 Python Worker 在项目级 venv 原生运行的正反例和依赖锁定测试通过」）由本批 S6 测试面兑现。
