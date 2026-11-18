# RT-1 ADR：Native Rust Adapter 设计裁决

- 日期：2026-11-19
- 状态：已定稿（RT-1 设计裁决；实现口径冻结，对应实现任务书 [RT-1-TASK.md](RT-1-TASK.md)）
- 依据：[NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md) §3.1（64-76 行）原生准入条件、§4（99-114 行）统一执行架构、§9:223 RT-1 行、§12（255-262 行）；[RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §2/§3/§6/§7/§8；[RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md) §2（31-47 行）与注 A（50 行）
- 代码现状锚点（2026-11-19 只读实测）：crates/tool-registry/src/validate.rs:744-751（native_rust 校验分支仅两字段）、crates/tool-engine/src/adapter/{python_worker,podman_host}.rs（进程范式）、crates/tool-engine/src/envelope.rs:296-327（截断）、crates/tool-engine/src/adapter/mod.rs:32-102（中性终局与码表）；mcp-server/runtime-gateway 中 native_rust 零命中（无消费路径）
- 决策记录形态：D1 到 D8，每条含裁决、论据（引用在位证据）、否决项

## D1：Adapter 挂 tool-engine，不拆新 crate

**裁决**：NativeRustAdapter 落 `crates/tool-engine/src/adapter/native_rust.rs`，随 `adapter/mod.rs` 注册；不新建 crate。

**论据**：
- NATIVE-FIRST §4:105-111 统一执行架构图中 NativeRustAdapter 是 Tool Engine 下的分支之一，与 PythonWorkerAdapter/PodmanHostAdapter 并列；§4.2:126「不为原生 Runtime 新建第二个 Tool Registry/Agent/Workflow Engine」的精神同样适用于不为它建第二个 crate 边界。
- 复用面在 tool-engine 内已齐备：AdapterOutcome/AdapterFailure 中性终局（mod.rs:32-79）、envelope::truncate（envelope.rs:296-327）、Engine.advance/build_envelope 状态机接线（python_worker.rs:504-790、podman_host.rs:701-1023 两个同形态先例）。
- 拆 crate 将引入新的 workspace 成员与 lock 变动面，与任务书红线 1（Cargo.lock 399 条目恒定）冲突，且跨 crate 引用 engine 内部件（Engine/RunRow/TerminalParts）需重新导出，收益为零。

**否决项**：`crates/native-rust-adapter/` 独立 crate，引入依赖面变动、违反零 lock 增量，否决。

## D2：Capability Provider 最小接线，静态目录种子，符合 RT-0.5-F 接口形状

**裁决**：RT-1 落 `crates/tool-engine/src/capability.rs`：数据结构逐字对齐 RT-0.5-F §2（CapabilityId/CapabilityStatus 五态/ProbeErrorKind 五态/BinaryIdentity 含 wrapper_target/PackageProvenance/ProbedFeature/Constraint/SecurityProfile/CapabilityEntry 十二字段/CachedEntry，F:18-93）；StaticCatalogProvider 实现 `get/list/fingerprint/probe(L0)` 最小面；种子数据为 `manifests/capabilities/host-catalog-seed.json`（RT-0.5-C §3 十六条转录）。完整探测（L1/L2/L3）、TTL/失效事件（F §5）不在本批，留 DI-1 前置批（对应 F §8.1/8.2 消费清单第 1/2 条的分期）。

**论据**：
- F §4（文档头）：「只定义接口、数据结构、缓存/失效策略；本批不实现生产 Adapter……实现与消费在 RT-1」，RT-1 是消费批，但消费的最小必要面是「条目存在性/身份校验」（Adapter 派发前 allowlist 校验），不是全量探测。
- C §4:249-250 实测 15 available + 1 missing（pdf.mutool）为静态目录的第一份快照（F:7「目录即本契约的首个数据快照」），转录即种子。
- 指纹纪律：C §5.1:269「完整指纹由 RT-1 Capability Provider 首探时以全量 sha256 重算生成；本目录不预生成、不编造具体指纹值」，静态种子只录实测 sha256（多数为 16 位前缀），`fingerprint()` 仅在 probe(L0) 实测全量 sha256 后按 F §5.1 公式计算返回；无全量值不产指纹，不升格前缀。

**否决项**：
- 全量 Provider（L1/L2/L3 + TTL + 失效事件）塞进 RT-1，超出 Adapter 主线，探测面行为测试依赖宿主状态易碎，否决；入口留 trait 形状（F §3:134-154）供续作实现。
- 种子内嵌 Rust 源码常量，环境事实应与代码分离且可被 pacman 升级流程重探替换，JSON 数据文件承载，否决源码内嵌。

## D3：allowlist 数据形状（binary 钉值 / subcommand 白名单 + 参数模板）

**裁决**：两种形态，可执行路径唯一来源 = Manifest 固定绝对值（经 Capability 目录校验兜底）：

```jsonc
// 形态 A：binary（直呼可执行文件，第三方/宿主 CLI 为主）
{
  "runtime": {
    "type": "native_rust",
    "binary": "/usr/bin/pdftotext",            // 必须绝对路径（"/" 前缀）
    "binary_sha256": "<64hex 全量钉值>",        // 必填；wrapper 场景钉 wrapper 与 exec target 两者（C §5.1:270）
    "binary_target_sha256": "<64hex>",          // 仅 wrapper 形态（file 类型为脚本）必填
    "capability_ref": "pdf.pdftotext"           // 可选：派发前查 Capability 目录（status=available 且 fresh）
  }
}
// 形态 B：subcommand（宿主 CLI 多子命令面，白名单 + 参数模板）
{
  "runtime": {
    "type": "native_rust",
    "binary": "/usr/bin/ffmpeg",
    "binary_sha256": "<64hex>",
    "subcommands": ["ffprobe", "-i", "-hide_banner"],   // 白名单数组（非空，逐项全匹配）
    "argv_template": ["{{input_path}}", "-f", "json"]   // 参数模板，{{slot}} 只由 input_schema 校验后参数填充
  }
}
```

登记面（validate.rs）与执行面分两层：
- **登记面兼容**：validate.rs native_rust 分支（744-751 行现状）增强为 additive，`binary` 升级为必须绝对路径；新增可选字段 `binary_sha256`（64hex 校验）/`binary_target_sha256`/`subcommands`/`argv_template`/`capability_ref`；旧 `subcommand` 单值字段保留接受。现有两份 native_rust Manifest（rp-plugin/rp-knowledge-fetch，名称形态）登记不破，钉值补录属 RT-4 逐项迁移。
- **执行面强制**：Adapter 派发前置校验拒绝，binary 非绝对路径、缺 binary_sha256、sha256 实测不符、子命令越白名单、argv 模板槽位未由 input_schema 校验参数填充，一律 PREPARING 拒绝（ADR D7 映射）。安全属性在执行面强制（NATIVE-FIRST §4:114「执行后端负责落实策略」），登记面渐进收紧（RT-4 补钉）。

**论据**：
- NATIVE-FIRST §3.1:69：「Manifest 固定工具身份、可执行文件身份、允许的子命令和参数 Schema；不得接收任意可执行路径或任意 Shell 字符串」，binary_sha256 钉值即「可执行文件身份」的执行面形态，与 podman_host 镜像 digest 二道校验（podman_host.rs:265-277 image_ref_ok）同构。
- RT-0.5-F §6:235：「原生路径用 entry.executable_path 绝对派发（allowlist 校验 binary_identity.fingerprint）」，capability_ref 可选引用使钉值与目录指纹互为二道校验；目录条目 stale/missing 时拒绝派发（F §6:226-228）。
- C §5.1:270：wrapper 形态（7z/7za/7zr、soffice）指纹必须同时覆盖 wrapper 与 exec 目标，binary_target_sha256 字段承接该实测结论。
- 现状反例即论据：validate.rs:744-751 现分支只查两字段非空，`binary: "rp-plugin"` 名称形态无任何身份校验；`binary_sha256` 钉值是把「来源可信且哈希固定」（§3.1:68）落到机器可判。

**否决项**：
- PATH 名称解析（`binary: "ffmpeg"`），probe-path-audit verdict 明确用户可写目录同名程序不得当作可信能力（F §4:175），否决。
- sha256 只登记 16 位前缀，前缀不构成身份钉值（碰撞面 + C §5.1 自己声明是取证口径非校验口径），钉值必须 64hex 全量，否决前缀钉值。
- allowlist 放 Registry 数据库，第二事实源（NATIVE-FIRST §4.2:126），事实只在 Manifest + 目录两处，否决。

## D4：env 白名单构造，显式集合，不继承父进程

**裁决**：子进程环境用 `Command::env_clear()` 后注入显式白名单，零继承：

- 默认白名单：`PATH`（钉值二进制所在目录最小集）、`TMPDIR`（任务 tmp 目录）、`LANG=C.UTF-8`。
- 追加变量唯一来源 = Manifest `runtime.env` 显式登记（键值均为字符串；validate.rs:703-709 已校验值为字符串的规则全类型生效）。
- 凭据零落子进程：Manifest `secrets` 非空即派发拒绝（native 路径本批不支持 secret 注入）；SSH Agent（`SSH_AUTH_SOCK`）/浏览器 Profile/数据库路径/Podman Socket 类变量因 env_clear 结构性不存在（NATIVE-FIRST §3.1:71「默认不继承不必要的凭据……」；E §2 条目 3/13）。
- 测试断言锚：fixture 工具回显自身环境，断言环境键集 = 白名单 ∪ Manifest env（任务书 §5 判据 4）。

**论据**：python_worker.rs:261-263 现范式只做 `cmd.env(k, v)` 追加（父环境全量继承），该范式对可信自研 Worker 成立，对宿主第三方 CLI 不成立（其解析器漏洞面见 E §1 格式域表）；env_clear 是把 E §2 条目 12「受控环境变量白名单，不继承完整宿主环境」变成结构性事实而非约定。

**否决项**：黑名单剔除式（继承后 unset 敏感键），黑名单永不完备（新增变量面不可枚举），否决；白名单是唯一形态。

## D5：超时轮询 + 进程组 kill

**裁决**：`std::process::Command` + `CommandExt::process_group(0)`（Unix）使子进程独立进程组；主线程 try_wait 轮询 deadline（对齐 podman_host.rs:483-512 范式）；到期 → `killpg(SIGTERM)` → 2s 优雅窗口 → 未退即 `killpg(SIGKILL)` → `wait()` 收尸。进程组口径覆盖孙进程（CLI 工具派生子进程为常态：magick delegate 调 gs、soffice 内部派 soffice.bin，见 C §3.10:157、§3.4:77）。

**论据**：
- E §2 条目 6：「timeout 硬上限 + 协作式取消 + 进程组回收」，进程组回收是条目原文要求。
- podman_host.rs:489-496 先例证明「只杀 client 会泄漏容器」的教训在原生子进程场景的等价物是「只杀直接子进程会泄漏孙进程」；killpg 是 std 可表达的最小正确解。
- 零 tokio：python_worker.rs:154-213 用 recv_timeout 轮询、podman_host.rs:484-511 用 try_wait+sleep 轮询，均为 std 线程模型；RT-1 沿用同模型（任务书红线 2）。

**否决项**：
- tokio::process + tokio::time，违反红线 2，否决。
- 仅 `child.kill()`（单进程 SIGKILL，无 TERM 窗口），孙进程泄漏 + 无优雅收尾，否决。
- SIGCHLD 异步收尸（signal hook 面），引入新依赖/新 unsafe 面，轮询已足够（预算粒度秒级），否决。

## D6：取消策略，cancel_requested_at 轮询 + CANCELLED 终态

**裁决**：RUNNING 期主循环轮询 `tool_run.cancel_requested_at`（podman_host.rs:595-608 同款 SQL，D-TC-04 语义）；命中 → `killpg(SIGTERM)` → 3s 窗口 → `killpg(SIGKILL)` → `AdapterOutcome::Cancelled` → 信封 CANCELLED 终态（-32004，python_worker.rs:734-748 映射先例）。取消窗口内已到达的部分 stdout 帧不作为结果交付（不伪装成功），stderr/部分产物按 partial 语义入 Artifact（podman_host.rs:966-981 四路径统一先例）。

**论据**：NATIVE-FIRST §4.1:118「Run 状态、幂等、预算、取消、超时……统一」，取消必须与 podman_host 同语义同状态机路径，不得分叉；D-TC-04 协作式取消语义已由 `Engine::request_cancel` 落库（podman_host.rs:15-17 注释引用）。

**否决项**：仅听 stdin 管道断裂推断取消，不可靠（管道断裂也可能是进程自亡），否决；以 DB 列为准。

## D7：错误五态映射（RT-0.5-F ProbeError → 信封 §5.3 九类）

**裁决**：五态与信封九类一一映射，不发明新词表值（envelope.rs:14-24 九类为唯一词表）：

| ProbeErrorKind（F:126-132） | 判定 | envelope category | code | retryable | ToolRunStatus |
|---|---|---|---|---|---|
| missing | 钉值路径无文件 / search roots 无该名 / capability 目录 missing | DEPENDENCY | `NATIVE_BINARY_MISSING` | false | FAILED（PREPARING 拒绝） |
| permission_denied | stat/执行位拒绝 | POLICY_DENIED | `NATIVE_PERMISSION_DENIED` | false | FAILED；不自动提权（F §7:251 路由语义） |
| timeout | 预算到期进程组击杀 | TIMEOUT | `-32002`（与 worker/podman 同码） | false | TIMED_OUT（python_worker.rs:719-733 先例） |
| unsupported_output | stdout 帧解析失败 / 形态不符 | EXECUTION | `NATIVE_UNSUPPORTED_OUTPUT` | false | FAILED（podman_host.rs:531-549 帧异常同语义） |
| probe_failed | 探测/派发面其余失败（spawn 失败、非零退出探测） | INFRA | `NATIVE_PROBE_FAILED` | true | FAILED |

- 区分原则：missing 是持久事实（DEPENDENCY 语义对齐 podman_host 镜像缺失 preparing_failed，podman_host.rs:723-753）；probe_failed 是平台/环境面瞬时故障（INFRA retryable=true 对齐 mod.rs:66-67「传输/握手层失败」定义）；timeout/unsupported_output 与既有 runtime 路径同码同态，保证跨 runtime 信封形状一致（NATIVE-FIRST §4.1:118）。
- F §7:256 硬要求「禁止把 timeout/probe_failed 报成 missing」由映射表唯一性保证（五态各一条路径，无兜底合并）。

**否决项**：五态原样透传为自定义 category（如 `PROBE_MISSING`），超出九类词表，envelope::validate 直接拒绝（envelope.rs:160-165），否决。

## D8：输出帽、帧协议与工作区三目录

**裁决**：
1. **帧协议**：stdin 一行 JSON 请求（run_id 不透传工具）、stdout 一行 JSON 结果帧（`{"status":"result","data":{...}}`，D-TC-03 同款；podman_host.rs:449-459、519-530 先例）；stderr 只采尾部 400 字节入错误消息（podman_host.rs:640-647 tail 先例），全量 stderr 走 Artifact（role=log）。
2. **输出帽双层**：(a) 读侧流式帽，stdout 读取线程按块计数，超 `max_output_bytes` 即停止读取并 killpg（防大输出整读 OOM，podman_host.rs:461-481 P4-4 管道死锁修复的教训要求读线程与轮询并发）；(b) 结果侧帽，帧 data 经 `envelope::truncate`（envelope.rs:296-327）降级 + 原始字节落 Artifact + `usage.truncated=true`（podman_host.rs:902-917 同款）。
3. **工作区三目录**：`<workspace_root>/<run_id>/{in,out,tmp}`（podman_host.rs:279-303 同形状）；输入文件由引擎落 in/ 后以绝对路径传入 argv 的 `{{input_path}}` 槽位，**槽位值必须落在本 run 的 in/ 目录前缀内**（前缀校验防任意文件读取）；out/ 产物收集走 podman_host.rs:651-696 同形态（sha256/size 登记 Artifact 表）；tmp/ 用后即焚（四路径统一）。
4. **原生档 CPU/内存强制**：优先 `systemd-run --user --scope -p CPUQuota= -p MemoryMax=` 包裹派发，systemd 用户会话不可用退 prlimit（RLIMIT_CPU/RLIMIT_AS），两机制均不可用即拒绝派发不裸跑（E §2 注 A:50 原文行为裁决；2026-11-18 本机核验 systemd-run 在位）。

**论据**：E §2 条目 1/2/10/14 逐条对应（只读输入、独立输出目录、输出上限、状态统一）；§3.1:70「明确工作目录、环境变量白名单、stdin/stdout/stderr、超时、取消、进程组回收和输出上限」是准入条件原文，D8 逐项兑现。

**否决项**：
- 直接把用户 input.path 传给工具 argv（不落工作区），任意文件读取面，否决；in/ 前缀校验强制。
- 结果超帽静默截断字符串，统一走 envelope::truncate 语义（Artifact 对账 + truncated 标记），否决静默截断。

## 生效与关系

- 本 ADR 为 RT-1 实现批次的设计依据；实现任务书 = [RT-1-TASK.md](RT-1-TASK.md)（S1→S8 步骤与八项判据）。
- 消费链：DI-1（Document Pack 原生首工具）消费本批 NativeRustAdapter + RT-0.5-F 契约，依赖链见 [DI-0-P34-PREMISE-REVISION-ADR.md](DI-0-P34-PREMISE-REVISION-ADR.md) §5。
- 与 RT-3 的边界：Runtime Policy Resolver（信任档位判定）归 RT-3；本批 adapter 只落实「Manifest + capability 校验通过的原生档」，档位升级/拒绝判定（E §1 三档）在 RT-3 接入前由 Manifest permission_level + input 来源声明承载，本批不做风险分类。
