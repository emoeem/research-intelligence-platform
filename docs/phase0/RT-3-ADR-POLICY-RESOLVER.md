# RT-3 设计裁决：Tool Runtime Policy Resolver（ADR）

- 日期：2026-11-21
- 状态：设计裁决已定稿（RT-3 设计裁决员产出；实现批次按 [RT-3-TASK.md](RT-3-TASK.md) 执行）
- 依据：
  - [NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md) §3:51-62（选择矩阵总表）、§3.1:64-76（原生准入全条件）、§3.2:78-88（容器准入）、§3.3:90-97（VM 准入）、§4:114（「Runtime 选择由 Registry Manifest + Tool Policy Resolver 决定」）、§9:225 RT-3 行（「按信任等级、依赖复杂度、隔离要求选择 native/python/container/VM；拒绝不安全降级」）、§10:238（「不受信工具无法选择 native/python 路径绕过隔离策略」）、§11:250（本轮禁止修改现有 Manifest）
  - [RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §6:229-236（Resolver 输入=Manifest(runtime.type, permission_level, network)+CapabilityEntry+输入风险分类，输出=执行后端+资源预算+隔离要求+拒绝不安全降级——本 ADR 输入输出契约的直接上位依据）、§7:248-257（五态→拒绝派发路由）
  - [RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md) §1:13-30（三档路由表 + 格式域默认档位 + 「podman_vm 未启用一律拒绝、禁止降级」）、§2:31-50（14 项资源限制清单，所有档位统一强制）、§3:62-66（路由图中「Runtime Policy decision」步 = 本 Resolver 落位）
  - [RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §5.3:290（stale 条目不可供 Runtime Policy 决策使用）、§6:292-303（目录=环境事实，Registry=工具事实，单向引用）
  - [RT-0.5-D-TOOL-MAPPING.md](RT-0.5-D-TOOL-MAPPING.md) §1-§5（Resolver 期望输出的手工版映射表 = 集成正例 oracle）
  - [RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md) D7:110-125（五态→九类信封映射，禁发明词表外 category）、生效节:145（「Runtime Policy Resolver（信任档位判定）归 RT-3」——本批与 adapter 的边界原文）
  - [RT-2-ADR-VENV-GOVERNANCE.md](RT-2-ADR-VENV-GOVERNANCE.md) D4（解释器三档）、D8 缝 5:194（rp-workflow venv 接线属 RT-3/RT-4 承接 + 验收断言原文）、与 RT-3 关系节（venv 五态 + VenvIdentity 是 RT-3 输入之一）
- 前置：RT-1 已收口（ae21a0a8，2026-11-19）；RT-2 已收口（6899feb8，2026-11-21）；DI-1/DI-2 已收口（659ed1e8/22c7adb0，新增 9 份 native_rust Manifest）
- 设计裁决：本文件 D1-D10 为实现口径唯一依据，实现任务书不重复论证

## D1：决策层落位 —— tool-engine 内 `pub mod policy`，不建独立 crate

**裁决**：Resolver 落 `crates/tool-engine/src/policy.rs`（`pub mod policy`），不建 `crates/policy-resolver` 独立 crate。

**论据**：Resolver 四维输入的既有承载全部在 tool-engine 内——`InputRisk`/`CallMode`/`SecurityProfile`/`CapabilityEntry`/`Freshness`（capability.rs:478-613，RT-1 落点）、`VenvStatus`/`VenvIdentity`（venv.rs:83-104，RT-2 落点）。独立 crate 只有两形态：policy→engine 依赖（把引擎拖进决策常量）或 engine→policy 依赖（本决策就是 engine 内模块，倒置无意义）；决策输出（目标 runtime + 约束）的直接消费方是同 crate 的 Adapter 层（native_rust.rs / python_worker.rs / podman_host.rs）与后续 RT-4 迁移面，同 crate 免去跨 crate trait 中转。NATIVE-FIRST §4:114「Tool Registry + Policy Resolver」是架构位描述，不是 crate 边界要求。

**否决项**：独立 crate（依赖面反向/正向均不合理，否决）；落 `crates/tool-registry`（决策需要 capability/venv 环境事实，registry 是纯工具事实面，C §6:292-303 边界表否决）；落 mcp-server dispatch 内联（决策要被 workflow-engine 缝 5 与 RT-4 消费，dispatch 私有函数不可复用，否决）。

## D2：四维输入 Schema 与词表冻结（含既有 enum 对齐映射）

**裁决**：Resolver 输入 = 四维词表枚举 + 宿主能力事实，全部为纯数据、零 IO。词表冻结如下（实现 enum + `as_str`/`parse` 双向，词表外值一律拒绝）：

| 维 | 词表 | 语义定义 | 与既有代码对齐 |
|---|---|---|---|
| Trust Level | `audited` / `registered` / `untrusted` | audited=来源可信且经审核（自研，或第三方且来源/版本/许可证/身份钉值齐全，策略 §3.1:68）；registered=Manifest 在册可追责但身份证据不全；untrusted=未登记/来源不明/运行期动态取回代码 | 新 enum（Rust 词表 = snake_case 同名）；派生规则见 D3 |
| Input Risk | `low` / `unknown` / `high` | E §1:13-17 三档：low=可信低风险；unknown=来源不明；high=高风险/对抗性 | **复用 capability.rs:510-538 既有 `InputRisk`**：`trusted_low_risk`↔low、`untrusted`↔unknown、`high_risk`↔high（本文档短名 = 代码枚举别名映射表，不建第二 enum；文档叙述用短名，代码与测试断言用既有 as_str 串） |
| Dependency Complexity | `simple` / `locked` / `conflicting` / `heavy` | simple=单源宿主依赖无冲突面；locked=依赖可哈希钉定且锁定可验证（RT-2 lock 面）；conflicting=版本/系统库冲突项目级隔离不能经济解决（§3.2:82 第 1 条）；heavy=重型运行时/GPU/浏览器/大型二进制需独立生命周期（§3.2:83 第 2 条） | 新 enum；派生规则见 D4 |
| Isolation Requirement | `none` / `fs` / `net` / `kernel` | none=无超出 E §2 全局清单的隔离要求；fs=文件系统边界要求（workspace 三目录/容器挂载可承载）；net=受控出网要求（统一 Egress 承载）；kernel=内核级边界要求（共享内核容器不可承载） | 新 enum；派生规则见 D5 |
| Host Capability（第 5 输入） | CapabilityEntry 五态 + Freshness + VenvStatus 五态 | C 目录条目（capability_ref 在场时）或 venv 探测身份（python_worker 声明 venv 时）或 EgressProxy 接线事实（net 维核验） | 直接复用 capability.rs:561-587 与 venv.rs 既有结构，零复制 |

**论据**：InputRisk 不建第二词表是红线级约束——capability.rs:510 既有 enum 已被 SecurityProfile.max_input_risk 与种子目录消费，另立 `{low,unknown,high}` enum 即两套事实源（NATIVE-FIRST §4.2:126「不建第二事实源」）；文档短名与代码枚举的别名映射表落在本 ADR 与实现单测（双向 parse 断言）。

**否决项**：词表用字符串自由文本（无编译期穷尽性，144 格全覆盖测试无法静态枚举，否决）；Isolation 增加 `cpu/mem` 档（资源预算是 E §2 全局清单非隔离维，策略 §3.2:84 第 3 条由 constraints 承载，否决）；Trust 直接复用 permission_level T0/T1/T2（T 档是授权面语义：TOOL-CONTRACT-DRAFT.md:132-134「T0 只读/T1 写/T2 高风险需授权引用」，与来源信任正交，混维即授权判级与运行时选择互相污染——T 档只进 D8 输出的 constraints/reason，否决并入）。

## D3：Trust Level 派生规则（本批纯函数推导，零新登记字段）

**裁决**：Trust 由 Manifest 现有字段纯函数推导（输入=`provenance.origin` + `runtime` 身份钉值 + `provenance` 证据完备性），本批不改任何登记面：

```
origin == "rip-native"                          → audited   // 自研代码在仓，RT-1/RT-2 治理面覆盖
origin == "internal-kit"                        → audited   // 平台发行 kit 自研（license/provenance 在册佐证）
origin == "opensource" 且身份钉值在位            → audited   // 第三方但 §3.1:68 四要素齐（来源/版本/license/sha256）
    身份钉值 = native_rust: binary_sha256(64hex)
             podman_host: image ref 内 @sha256:<64hex> digest
origin == "opensource" 且钉值缺失/残缺           → registered
origin 未声明 / provenance 缺 license            → registered
未登记 Manifest / 动态取回产物 / 未来显式声明     → untrusted（词表保留，现仓无成员，负例矩阵 fixture 承载）
```

**论据**：2026-11-21 全 29 份 Manifest 实测派生（本会话 `grep`/python 只读模拟）：9 份 DI-1/DI-2 native_rust 全部 opensource+binary_sha256 → audited；2 份 rip-native native_rust（rip.knowledge.document.fetch/source.sync）→ audited；16 份 python_worker（rip-native/internal-kit）→ audited；nuclei opensource+image digest → audited；rip.http.tls_inspect（YAML，rip-native）→ audited。`registered`/`untrusted` 现仓零成员，词表由负例矩阵与未来登记（RT-4）消费——两档保留是矩阵全覆盖（D6）的形式要求，不是现状描述。

**否决项**：新增 Manifest `trust_level` 登记字段（本批禁改 manifests/——RT-4 才落，红线 2；且派生面已可判定，提前落字段=第二事实源，否决）；把「缺钉值」直接判 untrusted（钉值缺失是 §3.1 准入缺口，走 D6 R6 inconsistent 通道；来源信任与身份核验混判会把 2 份 rip-native 工具误降为不可信，否决）。

## D4：Dependency Complexity 派生规则

**裁决**：

```
runtime.type == python_worker → locked   // 判据 = 仓库级事实：workers/python/requirements.in +
                                         // requirements.lock.txt 在位（RT-2 S2 已落地，本会话 ls 实测两文件存在）
                                         // + venv 声明在场时 VenvStatus==available；
                                         // 未声明 venv（现状 16 份全裸 python3，grep '"venv"' 零命中）
                                         // 不改变 complexity——lock 是仓库单例事实，声明缺位是部署缝（RT-2 D8 缝 4）
runtime.type == native_rust   → simple   // 单源宿主包（pacman 单包，C 目录 package_provenance 佐证）/ 自研静态二进制
runtime.type == podman_host   → heavy    // 镜像族独立生命周期语义（§3.2:83 第 2 条形态）
conflicting / heavy 例外升级面：lock 解析失败或依赖集冲突实证 → conflicting；
GPU/浏览器/大型系统运行时依赖声明 → heavy（两词表值现仓零成员，保留给 §3.2:82-83 第 1/2 条准入）
```

**论据**：complexity 是工具依赖面的事实维，不是部署维——16 份 python_worker 的 venv 声明缺位（RT-2 缝 4 留给 RT-4 补录）若误判为 conflicting 会把全部自研 Worker 判向容器，直接矛盾 RT-0.5-D §2:24-41「16 份全部保持 python_worker」的手工映射；以仓库 lock 单例（RT-2 D1 仓库级 venv 单例裁决同构）为判据使派生与手工映射一致。rip.text.embed 的 ONNX/模型权重依赖按 RT-0.5-D:27 口径仍是 locked（「不因依赖多直接容器化」），heavy 仅保留给项目级隔离不能经济解决的形态。

**否决项**：per-Manifest 声明 complexity 字段（同 D3 否决项 1，RT-4 前禁改 Manifest，否决）；把「未声明 venv」判 conflicting（推导与 RT-0.5-D oracle 全量矛盾，否决）。

## D5：Isolation Requirement 派生规则

**裁决**：Isolation Requirement = 「超出 E §2 全局清单（所有档位统一强制）之外的隔离要求」，由网络声明与输入风险联合派生：

```
base = none                                   // E §2 清单 14 项是全局底线，不计入本维
network.mode == "egress_allowlist"  → 至少 net   // 受控出网要求：统一 Egress Policy/代理承载（E §2:45 条目 11）
network.mode == "vm_local"          → kernel      // 登记语义「仅 VM 内部互通」（TOOL-CONTRACT-DRAFT.md:106）——
                                                  // 宿主进程与共享内核容器均不可承载，唯一合法后端 = VM
input_risk == unknown               → 至少 fs      // E §1:16「隔离执行：rootless Podman 优先」的文件系统形态
input_risk == high                  → kernel       // E §1:17「需要宿主内核隔离边界」
secrets 非空                        → 至少 fs      // native 档禁 secret 注入（RT-1 D4 / native_rust.rs:829-831），
                                                  // secret 承载面需容器/VM 档
多来源取 max（none < fs < net < kernel；kernel 蕴含 fs/net，net 蕴含 fs）
```

**论据**：派生只吃 Manifest 声明 + 输入风险分类（F §6:230-233 契约原文的两类输入）；`vm_local` → kernel 是本 ADR 对 RT-0.5-D:37「RT-3 Policy Resolver 复核 vm_local 边界」悬案的显式裁决——rip.evidence.rtk.mcp 声明 python_worker + vm_local + T2，宿主 python_worker 进程结构性无法承载「仅 VM 内部互通」，D6 判 inconsistent（详见 D7 N6 与 RT-3-TASK §2 A9 集成期望预判）。`net` 在 audited + EgressProxy 接线在位时不强制升级（rip.knowledge.document.fetch/source.sync、rip.http.tls_inspect 现状形态，RT-0.5-D:25/43 保持裁决），无接线事实时 fail-closed 升级容器——接线事实属 Host Capability 输入的 EgressProxy 子事实（tool-engine egress_proxy.rs:1-779 既有面）。

**否决项**：`net` 一律升级容器（会使 3 份 audited+egress_allowlist 工具与 RT-0.5-D 保持裁决矛盾，否决——升级与否由接线事实承载而非一刀切）；fs 档强制容器（audited+low 的 fs 要求由 RT-1 workspace 三目录承载，D8.3:132 原文形态，否决）；input_risk 不进 isolation 派生（E §1 三档路由就是风险→隔离映射原文，漏掉即三档路由表脱钩，否决）。

## D6：决策矩阵 —— R0-R7 有序瀑布纯函数，144 格全覆盖（totality）

**裁决**：决策 = 有序规则瀑布的纯函数（同输入必同输出，无时钟/无环境读取/无随机）；目标档位以 floor 合成，floor 序 `native_rust/python_worker(0) < podman_host(1) < podman_vm(2)`。策略 §3.1/§3.2/§3.3 三表逐行映射进规则命中理由（reason 必须引用命中行号）：

```
resolve(inputs) -> Target | Reject：
  R0 词表/完整性闸（§3 总表行语义合法面）：四维任一缺省或词表外 → Reject(RT3_INPUT_INCOMPLETE)
  R1 宿主能力闸（C §5.3:290 / E §3:75）：
     capability_ref 在场 → CapabilityProvider.get：Freshness=fresh 且 status=available 才继续；
       missing / stale / 五态非 available → Reject(RT3_CAPABILITY_UNAVAILABLE, DEPENDENCY)
     python_worker 声明 venv → VenvStatus=available 才继续；否则 Reject(RT3_VENV_UNAVAILABLE, DEPENDENCY)
     EgressProxy 接线子事实随输入携带（net 维核验用）
  R2 风险闸（E §1:13-17 逐行）：high → floor=vm；unknown → floor≥container；low → floor=0
  R3 信任闸（§3.1:64-76 全条件；§3 总表:56-59）：
     untrusted → floor≥container（§3.1:68「来源可信且经过审核」不满足，§10:238「不受信工具无法选择
       native/python 路径」）；audited/registered → 不设 floor，准入缺口留给 R6 逐条核验
  R4 隔离闸（D5 派生值消费）：
     kernel → floor=vm（§3.3:92-95；E §1:17）
     net → audited 且 EgressProxy 接线在位 → 不升级；否则 floor≥container（fail-closed：无承载能力
       的受控出网需求一律容器/VM，E §2:45 条目 11）
     fs / none → 无强制升级（fs 由 native workspace 三目录或容器挂载承载）
  R5 依赖闸（§3.2:82-85 准入条件逐条）：conflicting → floor≥container（§3.2:82 第 1 条）；
     heavy → floor≥container，且 untrusted+heavy → floor=vm（§3.3:92 第 1 条）；locked → 候选
     python_worker（NATIVE-FIRST §5:134-135）；simple → 候选 native_rust（§3.1 全条件后置 R6 核验）
  R6 目标合成 + §3.1 准入核验：
     floor=vm → 目标 podman_vm；VM 未启用/未实现 → Reject(RT3_VM_NOT_READY)（E §1:17「未启用时
       一律拒绝派发并报告，禁止降级 podman_host 或受限宿主原生」原文行为）
     floor=container → 目标 podman_host，§3.2 准入理由（命中 R2/R3/R4/R5 行）写入 reason
     floor=native → 按 R5 候选定 native_rust/python_worker；逐条核验 §3.1:68-74 准入（身份钉值
       绝对路径/allowlist 形态/预算机制可用面/egress 面/secrets 空）→ 任一缺口 → Inconsistent
       （缺口清单进 reason，与 RT-1 adapter PREPARING 拒绝行为同源：native_rust.rs:330-437）
  R7 交叉校验（§3:114 与 D9 决策/派发分离裁决）：
     前置：Manifest runtime.type 词表外（如 mcp_client/http_api/cli_adapter）→ Reject(RT3_RUNTIME_VOCAB)
     目标 == Manifest runtime.type → Assign；≠ → Inconsistent（迁移建议 + 差距列表）；
     health.probe.type=none 元数据条目（如 rip.ops.worker-runtime）→ NotApplicable（不派发面）
```

floor 合成公式使 3×3×4×4=144 格全部有定义（totality 是硬性验收：实现测试必须枚举全积断言每格产出 Target 或 Reject，无未处理格）。代表性格点（audited 行）：

| InputRisk \ Isolation | none | fs | net（接线在位） | kernel |
|---|---|---|---|---|
| low | native（按 R5 候选） | native | native | vm→VM 未就绪 Reject(RT3_VM_NOT_READY) |
| unknown | container | container | container | vm→Reject(RT3_VM_NOT_READY) |
| high | vm→Reject(RT3_VM_NOT_READY) | vm→Reject | vm→Reject | vm→Reject |

**论据**：三表映射的行号引用是可解释性输出（D8）的事实锚；floor 合成使矩阵可静态枚举——144 格遍历测试是「决策表全维覆盖正例」验收的机器可判形态；VM 未就绪一律 Reject 不降级是 E §1:17 原文行为裁决，与「unknown 无镜像族时拒绝或降级 VM」（E §1:16）的拒绝语义对齐（本批 podman_vm 未实现，全部 kernel/high 格点在现状宿主上=Reject，inconsistent 与否由声明值决定）。

**否决项**：无序 if-else 堆叠（规则间优先级不可审计，144 格无法静态枚举，否决）；决策读数据库/时钟（纯函数破坏可重放性与离线测试，否决）；把 Assign/Inconsistent/Reject 合并成单一布尔（RT-4 需要「不一致差距清单」作迁移输入，布尔丢信息，否决）。

## D7：不安全降级定义与硬拒绝矩阵

**裁决**：不安全降级 = 「为可用性/性能把输入组合路由到隔离强度低于该组合下限的后端」。硬拒绝矩阵（N1-N10，逐条为负例矩阵测试用例）：

| # | 输入组合 | 裁决 | 依据 |
|---|---|---|---|
| N1 | trust=untrusted × 任意 InputRisk × 任意目标 ∈ {native_rust, python_worker} | 硬拒绝（floor≥container） | §3.1:68、§10:238、§3 总表:57-58 |
| N2 | input_risk=high × 目标 podman_host | 硬拒绝（rootless≠VM 隔离） | §3:58「不能将 rootless 等同 VM 隔离」、E §1:17 |
| N3 | input_risk=high 或 isolation=kernel × VM 未启用 | Reject(RT3_VM_NOT_READY)，禁降级 podman_host/native | E §1:17 原文行为 |
| N4 | input_risk=unknown × 目标 native_rust/python_worker | 硬拒绝（不静默原生化） | E §1:16 |
| N5 | isolation=kernel × 目标 ∈ {native, python, podman_host} | 硬拒绝（仅 vm_isolated） | §3.3:92-95 |
| N6 | network.mode=vm_local（派生 kernel）× 声明 native/python | 硬拒绝目标 + 现状声明记 Inconsistent（现仓命中：rip.evidence.rtk.mcp） | D5、TOOL-CONTRACT-DRAFT.md:106、RT-0.5-D:37 悬案裁决 |
| N7 | capability_ref 在场 + 五态非 available 或 stale | Reject(RT3_CAPABILITY_UNAVAILABLE)，不猜测、不静默换实现 | C §5.3:290、E §3:75、F §7:248-257 |
| N8 | secrets 非空 × 目标 native_rust/python_worker 原生档 | 硬拒绝（floor≥container） | RT-1 D4（native_rust.rs:829-831 同源） |
| N9 | dependency=conflicting/heavy × 目标 native/python | 硬拒绝（§3.2 准入不成立）；untrusted+heavy → vm | §3.2:82-83 |
| N10 | 四维词表外值 / 任一维缺省 / 词表外 runtime.type 声明 | Reject(RT3_INPUT_INCOMPLETE / RT3_RUNTIME_VOCAB) | D2 词表冻结 + R0 |

拒绝语义细则：Reject 输出沿用 RT-1 D7:112 纪律——信封 category 只用九类词表（envelope.rs:14-23），不发明新 category；决策码 `RT3_` 前缀（先例：RT-1 `NATIVE_` 前缀码）。N1-N5/N7-N10 在纯函数输出层为 Reject；N4/N6/N9 在「声明值与目标冲突」语境下落 Inconsistent（差距=隔离升级），两态都阻断「按声明派发」的合法性主张。

**论据**：硬拒绝矩阵是策略 §9:225「拒绝不安全降级」的机器可判展开；每条 N 规则独立可测（一个 fixture 一个断言），且与 adapter 执行面已有强制（native secrets/钉值拒绝、venv 五态拒绝）互为纵深——Resolver 决策拒绝在派发前，adapter 拒绝在派发中，两层独立不互为实现。

**否决项**：警告不拒绝（降级必须阻断合法性主张，warn-and-continue 即静默降级变体，否决）；按 tool_id 硬编码豁免名单（规则必须由维度驱动，名单即绕过面，否决）。

## D8：决策输出 Schema

**裁决**：输出为可序列化结构（serde），三/四态 verdict + 证据链 reason 必填：

```json
RuntimeDecision {
  "tool_id": "rip.doc.pdf.text",
  "verdict": "assign" | "inconsistent" | "reject" | "not_applicable",
  "runtime": "native_rust" | "python_worker" | "podman_host" | "podman_vm",   // reject 时为 null
  "reason": [                                  // 有序规则命中链，每条含行号锚
    {"rule": "R2", "hit": "input_risk=low → floor=0", "anchor": "RT-0.5-E §1:15"},
    {"rule": "R6", "hit": "§3.1 准入核验通过（binary_sha256 在位）", "anchor": "NATIVE-FIRST §3.1:68"}
  ],
  "inputs": {                                  // 引用输入证据（决策可复现的最小集）
    "trust_level": "audited",                  // + 派生来源：provenance.origin=opensource + binary_sha256 在位
    "input_risk": "low",                       // 代码枚举串 trusted_low_risk（D2 别名映射）
    "dependency_complexity": "simple",
    "isolation_requirement": "none",
    "host_capability": {                        // capability_ref 在场时
      "capability_id": "pdf.pdftotext", "status": "available",
      "freshness": "fresh", "fingerprint": "…16hex 或 null"
    } | {"venv": {"status": "available", "fingerprint": "…"}} | null
  },
  "cross_check": {"manifest_runtime": "native_rust", "consistent": true},
  "constraints": [                              // E §2 清单适用项 + C 目录 constraints 直通 + 授权面引用
    "e2:network-default-off", "e2:output-cap", "catalog:页数帽由 pdfinfo 前步承载",
    "permission_level=T0（授权面归既有 Tool Policy，不入四维）"
  ],
  "reject": {                                   // verdict=reject 时必填
    "code": "RT3_VM_NOT_READY",                 // RT3_ 前缀词表
    "category": "POLICY_DENIED",                // 九类信封词表内（envelope.rs:14-23）
    "message": "…"
  },
  "gap": ["…"]                                  // verdict=inconsistent 时必填：声明↔目标差距清单（RT-4 迁移输入）
}
```

可解释性硬要求：reason 非空且至少含 R0-R7 命中链与四维取值回显；每条 hit 携带文档/代码锚（策略 §3.x 行号 / E §1 行 / C 条目 id / Manifest 字段路径）；inputs 与 reason 联合构成「决策可复现最小集」——给定同 inputs 必得同 verdict/runtime/reason（D6 纯函数性质的可观测面）。

**论据**：F §6:233「输出=执行后端+资源预算+隔离要求；拒绝不安全降级」→ runtime+constraints+reject 三字段一一对应；RT-4 逐项迁移需要 gap 清单（策略 §9:226「对每个工具给出原生/容器/VM 的理由和实测」——reason 链即理由部分的机器形态）。

**否决项**：自由文本 reason（不可断言、不可测试，否决）；reason 只给结论不给锚（可解释性验收不可判，否决）；constraints 自造词表（E §2 条目引用 `e2:*` + C 目录直通 + 既有 T 档语义，词表外即第二事实源，否决）。

## D9：决策与派发分离（本批不改派发链、不改 Manifest；RT-4 落地）

**裁决**：本批 Resolver 是**纯决策面**：(a) 派发链零改动——`crates/mcp-server/src/dispatch.rs:224-296` 直读 `runtime.type` 分流的现状行为逐字节不变，Resolver 不接入派发路径；(b) Manifest 零改动——`manifests/tools/*` 本批零 diff（策略 §11:250 本轮禁改 + §9:226「不批量改写」）；(c) Resolver 独立决策——输入只吃四维 + 宿主能力事实 + Manifest 声明，输出与声明交叉校验，**决策不因声明值而偏置**（Resolver 先derive 目标再对比声明，不是「校验声明是否合规」）；(d) RT-4 按 D8 gap/reason 清单逐项落 Manifest，落地后才允许派发链消费 Resolver（决策覆盖声明 = RT-4 之后的批）。缝 5 承接：`crates/workflow-engine/src/bin/rp-workflow.rs:1030-1033` 的 `venv_config: None` 补上同款 `runtime.venv` 读取（dispatch.rs:925/rp-agent.rs:320 同款构造），验收断言 = workflow 派发链对 venv 声明 Manifest 必须同样探测拒绝（RT-2-ADR D8 缝 5:194 原文），这是本批唯一允许触碰派发构造点的改动（additive：未声明 venv 的 Manifest 行为零变化）。

**论据**：策略 §9 RT-3 行交付物是「选择 + 拒绝」的决策能力与依据，不是迁移执行（迁移是 RT-4 行）；本批若把 Resolver 接进派发，28 份未迁移 Manifest 的行为面即时漂移（3 处 inconsistent 立即阻断现役工具），违反 additive 纪律与 RT-2-TASK §3.2:68 同款红线；决策/派发分离让 Resolver 的 144 格矩阵与 29 份 Manifest 集成期望表先在测试面站稳，RT-4 迁移时有 oracle 可依。

**否决项**：本批接入 dispatch 决策点（行为面漂移 + 违反策略 §11:250，否决）；Resolver 仅做「校验器」（只回答声明是否合规，不给独立目标——RT-4 需要独立决策作迁移依据，F §6:233 契约也是决策形态，否决）；把缝 5 推给 RT-4（RT-2-ADR 把验收断言显式挂 RT-3/RT-4，且缝 5 是「补录 venv 声明前的硬性前置」（RT-2-ADR 生效节），RT-3 不承接则 RT-4 首份 venv 声明即带旁路面，否决）。

## D10：宿主能力输入纪律与决策只读红线

**裁决**：(a) Host Capability 缺失/missing/stale/五态非 available = 决策 Reject（N7），不猜测、不静默换实现、不用旧探测值（C §5.3:290「stale 条目不可供 Runtime Policy 决策使用」原文）；(b) Resolver 零写入——不写 Tool Registry、不写 Manifest、不写数据库（决策结果只落调用方内存/批次报告/测试断言），模块面无 `&mut` 出口（红线 1）；(c) 决策输入的 capability 查询走 RT-1 既有 CapabilityProvider 面（capability.rs StaticCatalogProvider:1028 起 + freshness 判定），venv 输入走 RT-2 既有 `venv::probe` 面，本批不复制探测逻辑；(d) 测试离线——144 格矩阵与负例矩阵全用内存 fixture（CapabilityEntry/VenvStatus 合成值），29 份 Manifest 集成测试只读 `manifests/tools/` 与 capability 种子（`manifests/capabilities/host-catalog-seed.json`，RT-1 已入库 16 条），零网络零安装。

**论据**：D10a 是任务书红线「Host Capability 缺失=决策拒绝而非猜测」的 ADR 化；D10b/c 使 Resolver 成为无副作用的纯决策库——红线「决策只读不写 Registry/Manifest」在模块 API 层结构性成立而非靠纪律；D10d 与 RT-2 红线 7（测试离线 + --test-threads=1）同门。

**否决项**：Resolver 内重探宿主能力（RT-1 Provider 已有缓存/失效面，复制即分叉，否决）；决策结果落库（无消费场景 + 违反只读红线，否决）；stale 条目刷新后沿用旧结论（C §5.3 重探失败回退错误态语义，否决）。

## 生效与关系

- 本 ADR 为 RT-3 实现批次的设计依据；实现任务书 = [RT-3-TASK.md](RT-3-TASK.md)（S1→S8 步骤、完成标准判据表、测试面命令）。
- 与 RT-1 的关系：消费其 CapabilityProvider/五态映射/NATIVE_ 拒绝码先例；不触碰 native_rust.rs/podman_host.rs（RT-2-TASK §3.2:68 同款零触碰红线延续）。
- 与 RT-2 的关系：消费其 VenvStatus/VenvIdentity/lock 面作 python_worker 输入；承接缝 5（rp-workflow venv 接线 + workflow 链探测拒绝验收断言）；不触碰 venv.rs。
- 与 RT-4 的关系：本批产出 29 份 Manifest 的 RuntimeDecision（含 3 处 inconsistent 的 gap 清单）= RT-4 逐项迁移的机器可判输入；RT-4 落 Manifest 后派发链才消费 Resolver；RT-4 前不得给经 workflow-engine 派发的工具补 venv 声明（RT-2-ADR 缝 5 硬性前置由本批 S 步骤解除）。
- 与 DI-1/DI-2 Router 的关系：E §3:62 路由图中 input-risk classification 在 Router 侧、Runtime Policy decision 在本 Resolver；本批不触碰 Router，风险档位作为决策输入参数由测试面合成承载。
- NATIVE-FIRST §10:238 验收行（「不受信工具无法选择 native/python 路径绕过隔离策略」）由本批 D7 N1 硬拒绝 + 144 格矩阵测试兑现；§9:225 RT-3 行由本 ADR D1-D10 整体兑现。
