# RT-1 批次验收报告：Native Rust Adapter 设计与最小实现

- 日期：2026-11-19
- 状态：已收口（S1→S8 串行执行完毕；八项判据逐项测试与实测输出见 §3）
- 依据：[RT-1-TASK.md](RT-1-TASK.md)（任务书）、[RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md)（D1-D8 设计裁决）、[RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md)（Provider 契约）
- 提交链：`9c76f6f9`（S2 校验增强）→ `7146d8f0`（S3 Capability Provider）→ `ff70f6aa`（S4+S5+S6 Adapter/接线/测试）→ 本批 S8（报告与登记，见 git log）

## 1. 交付物对账（任务书 §9）

| # | 交付物 | 落点 | 状态 |
|---|---|---|---|
| 1 | 代码 | `crates/tool-engine/src/adapter/native_rust.rs`（新增，S4/S5）+ `adapter/mod.rs`（注册导出）+ `crates/tool-engine/src/capability.rs`（新增，S3）+ `lib.rs`（pub mod capability）+ `crates/tool-registry/src/validate.rs`（native_rust 分支增强，S2） | ✅ |
| 2 | 测试 | `crates/tool-engine/tests/native_rust_e2e.rs`（15 测试）+ `crates/tool-engine/tests/capability_catalog.rs`（11 测试）+ `crates/tool-registry/tests/sk3_tool_manifests.rs`（+3 测试） | ✅ 29 测试全绿 |
| 3 | 数据 | `manifests/capabilities/host-catalog-seed.json`（16 条转录）+ `schemas/tool-manifest/v1.json`（additive 五可选字段） | ✅ |
| 4 | 文档 | 本报告 + docs/README.md 登记；任务书/ADR/DI-0 ADR（本批已落） | ✅ |
| 5 | 前置输出 | DI-1 可消费面：`execute_native_rust_via_engine` + `NativeRustAdapter/Config/Call` + `StaticCatalogProvider`（get/list/fingerprint/probe L0）+ validate.rs allowlist 登记形状——DI-0 ADR §5 依赖链兑现 | ✅ |

## 2. 门禁实测记录（命令原文 + 输出）

### S1 基线核验（2026-11-19 实测，开工时点）

```
$ grep -c '^\[\[package\]\]' Cargo.lock
399
$ grep -n tokio crates/tool-engine/Cargo.toml
（零命中，exit 1）
$ cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p tool-engine native_rust -- --test-threads=1
（全套件编译通过，native_rust 过滤 0 tests，退出码 0）
$ cargo clippy -p tool-engine --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s（零告警）
```

### S7 收口门（2026-11-19 实测，收口时点）

```
$ cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p tool-engine native_rust -- --test-threads=1
     Running tests/native_rust_e2e.rs (target/debug/deps/native_rust_e2e-…)
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.16s
（其余 17 套件 0 filtered 命中，全部编译通过）

$ cargo test --offline -p tool-engine --test capability_catalog -- --test-threads=1
test result: ok. 11 passed; 0 failed; …; finished in 0.04s

$ cargo test --offline -p tool-registry sk3
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s

$ cargo clippy -p tool-engine --all-targets -- -D warnings
Finished `dev` profile …（零告警）
$ cargo clippy -p tool-registry --all-targets -- -D warnings
Finished `dev` profile …（零告警）

$ rustfmt --edition 2021 --check <本批全部落点文件>
SCOPED-FMT-CLEAN

$ grep -c '^\[\[package\]\]' Cargo.lock
399
$ ls vendor | wc -l
375（与 S1 基线一致）
$ git status --short -- vendor/ Cargo.lock Cargo.toml migrations/ integrations/rtis/
（空输出：vendor/lock/依赖面/迁移/RTIS 零改动）
```

```
$ cargo test --offline --workspace（输出落盘 /tmp/rt1-workspace-test.log 复核）
（146 套件；汇总 888 passed / 0 failed；exit 0）
native_rust_e2e: test result: ok. 15 passed; 0 failed; …; finished in 3.44s
capability_catalog: test result: ok. 11 passed; 0 failed; …; finished in 0.03s
sk3_tool_manifests: test result: ok. 6 passed; 0 failed; …; finished in 0.07s
```

## 3. 八项判据对照表（S6 测试 × S7 实测）

| # | 判据 | 测试（native_rust_e2e.rs 除非注明） | 实测要点 |
|---|---|---|---|
| 1 | allowlist 正例 | `native_rust_allowlist_positive_run_succeeds_and_env_is_whitelist_exact` | 钉值命中（fixture sha256 `282af314…d73aa3`）+ 白名单子命令 `env` 命中 → SUCCEEDED；stdout 帧 `{"status":"result","data":…}` 解析为结构化结果；transcript [Q,P,RUNNING,SUCCEEDED] |
| 2 | allowlist 负例 | `native_rust_rejects_sha_mismatch` / `native_rust_rejects_missing_binary` / `native_rust_rejects_subcommand_not_in_whitelist` / `native_rust_rejects_manifest_with_secrets` / `native_rust_resource_enforcement_unavailable_rejects_dispatch` | sha 不符→POLICY_DENIED/NATIVE_IDENTITY_MISMATCH；缺失→DEPENDENCY/NATIVE_BINARY_MISSING；越白名单→POLICY_DENIED/NATIVE_SUBCOMMAND_NOT_ALLOWED；secrets→POLICY_DENIED/NATIVE_SECRETS_UNSUPPORTED；transcript 全部 [Q,P,FAILED] 且工作区未创建（零子进程拉起的结构性佐证） |
| 3 | 任意路径拒绝 | `native_rust_rejects_relative_binary` / `native_rust_rejects_input_path_escape` / `native_rust_source_has_no_shell_entry`；登记面负例在 sk3_tool_manifests.rs（相对路径/PATH 名/坏 hash/subcommands 越界/argv 注入/capability_ref 越界） | PATH 名解析派发面拒绝；`../` 穿越与 `/etc/passwd` 绝对注入被 in/ 前缀校验拒绝；adapter 源码剥注释后 grep 断言 `sh -c`/`bash -c`/`/bin/sh`/`Command::new("sh")` 等零命中（代码层无 Shell 构造入口） |
| 4 | env 泄漏断言 | `native_rust_allowlist_positive…`（fixture 回显自身环境） | 子进程环境键 ∈ {LANG, PATH, TMPDIR, RIP_RT1_CUSTOM, PWD, SHLVL, _}（前三+Manifest env 为白名单，PWD/SHLVL/_ 为 bash 自设非继承）；宿主会话全部非白名单键（HOME/CARGO_*/ANDROID_HOST/SSH_AUTH_SOCK 等）零透传（env_clear 实证）；PATH=钉值二进制所在目录、TMPDIR=任务 tmp、LANG=C.UTF-8 逐值断言；PWD=任务目录（cwd 固定） |
| 5 | 超时击杀 | `native_rust_timeout_terminates_group_and_kills_grandchild` | 预算 1s 到期 → killpg TERM（fixture `trap '' TERM` 忽略）→ 2s 窗口 → killpg KILL（耗时 ≥2.5s 实证窗口）→ TIMED_OUT/-32002；fixture 派生 `sleep 60` 孙进程，out/child.pid 登记 → /proc/&lt;pid&gt; 轮询至消失（进程组零残留） |
| 6 | 取消 | `native_rust_cancel_partial_output_not_delivered` | cancel_requested_at 轮询命中（RUNNING 期 400ms 后 request_cancel）→ CANCELLED 终态信封（category=CANCELLED，code=-32004）；fixture 先产出完整结果帧再阻塞——信封 result=None（部分输出不伪装成功），stdout 留档 Artifact |
| 7 | 输出帽 | `native_rust_read_side_cap_terminates_group`（读侧）+ `native_rust_result_side_truncate_registers_artifact`（结果侧） | 读侧：200KB 帧 vs 4KB 帽 → 触顶 killpg + 失败语义交付（NATIVE_OUTPUT_CAP_EXCEEDED），帽内字节（4096+64KiB）留档且 Artifact sha256 对账；结果侧：100KB echo 结果 vs 65536 帽 → envelope::truncate 降级 + usage.truncated=true + 原始字节 Artifact（size 对账 + out/stdout.log 实体留档） |
| 8 | 统一信封 | 各路径测试内 `assert_unified_envelope`（含超时/取消/失败/成功四路径 + capability/wrapper 双路径） | 每路径 `envelope::validate` 过；transcript 与 tool_run 行终点一致；`environment.runtime_type="native_rust"` 写入 |

能力面判据（S3/S6 附带）：

| 判据 | 测试（capability_catalog.rs） | 实测要点 |
|---|---|---|
| 种子 16 条加载（15 available + 1 missing） | `seed_loads_16_entries_15_available_1_missing` | 16 条全量；available=15、missing=pdf.mutool；十二字段抽查；wrapper 双钉转录（archive.7z wrapper 全值在案） |
| probe(L0) stat/sha256 校验 | `probe_l0_measures_full_sha256_and_fingerprint` / `probe_l0_wrapper_double_pin_measured` / `probe_l0_missing_entry_rechecked_still_missing` / `probe_l0_missing_file_reports_missing_five_state` | 全量 sha256 实测与独立 `sha256_hex` 重算一致；wrapper exec 目标 /usr/lib/7zip/7z 实测；missing 复核仍 missing；文件缺失报 Missing 五态 |
| 指纹不预生成、不编造 | 同上 + `probe_above_l0_returns_not_probed` | 未实测时 fingerprint=None（16 位前缀不升格）；实测后指纹与 F §5.1 公式测试内独立重算一致（capset 排序 + wrapper 另含 target_sha256）；L1+ 请求返回 NotProbed |
| 五态区分硬要求 | `probe_l0_identity_mismatch_marks_stale_not_missing` | 身份不符 → ProbeFailed + 标 Stale{BinaryChanged}，不报 missing |
| 缓存/失效最小面 | `invalidate_marks_stale_with_counts` / `refresh_after_probe_returns_fresh_entry` / `list_filter_domain_status_feature` | 失效事件计数、stale 不可决策、重探回 Fresh、域/状态/feature 过滤 |

## 4. 实现口径裁决记录（与 ADR 字面的偏差与调和，全部为「实现细节级」，设计裁决零变更）

1. **killpg 经固定路径 `/usr/bin/kill` 负 pgid 投递**：std 无 killpg、libc/nix 为零依赖红线禁止项（Cargo.lock 399 恒定）；kill 工具为 Adapter 自有基础设施固定值（`NativeRustConfig.kill_bin`），非 Manifest/Agent 来源，与 podman_bin 同性质。
2. **读侧帽 = max_output_bytes + 64 KiB 帧封装容差**（D8.2a 字面为「超 max_output_bytes 即停止读取」）：若读侧帽与结果帽相等，超帽帧必在封装开销处被杀、结果侧截断路径（D8.2b）不可达成死代码；容差使双层语义都可达。真帽仍由 envelope::truncate 按 max_output_bytes 强制（判据 7 两测试分别覆盖两侧）。
3. **资源强制探测定案于 PREPARING**（D8.4「机制不可用即拒绝派发不裸跑」）：Auto=systemd-run 探测 → prlimit 探测 → 双不可用 PREPARING 拒绝（INFRA retryable）；拒绝发生在派发前置，RUNNING 期不再回退裸跑。`ResourceEnforcement::Off` 仅限测试载具显式声明（生产默认 Auto），实测 fail-closed 用注入不存在探测路径验证。
4. **派发面平台错误码超出 D7 五码字面**：category 严格九类内（envelope.rs:14-24 词表零越界），code 为平台侧字符串（podman_host "PREPARING_FAILED"/"CONTAINER_TRANSPORT_FAILED" 同先例）：NATIVE_IDENTITY_MISMATCH / NATIVE_SUBCOMMAND_NOT_ALLOWED / NATIVE_ARGV_TEMPLATE_DENIED / NATIVE_IDENTITY_UNPINNED / NATIVE_SECRETS_UNSUPPORTED / NATIVE_CAPABILITY_MISSING / NATIVE_CAPABILITY_STALE / NATIVE_CAPABILITY_UNAVAILABLE / NATIVE_WRAPPER_TARGET_UNRESOLVED / NATIVE_OUTPUT_CAP_EXCEEDED；D7 五码原样承载五态主路径（missing→NATIVE_BINARY_MISSING/DEPENDENCY、permission→NATIVE_PERMISSION_DENIED/POLICY_DENIED、timeout→-32002/TIMED_OUT、unsupported_output→NATIVE_UNSUPPORTED_OUTPUT/EXECUTION、probe_failed→NATIVE_PROBE_FAILED/INFRA retryable）。
5. **CapabilityId 段首放宽为 `[a-z0-9][a-z0-9_-]*`**：F §2 注释「两段小写」与 RT-0.5-C 目录权威数据（archive.7z/7za/7zr 段首数字）冲突，以数据面为准；validate.rs capability_ref 同规则同步（两段形状约束不变）。
6. **种子 ffprobe 前缀按 16 位口径实测修正**：RT-0.5-C §3.2 原文 `a2ae3d0209aec7399` 为 17 位（越出 16 位取证口径一字）；实测 `sha256sum /usr/bin/ffprobe` 首 16 位 `a2ae3d0209aec739` 落种子（C 目录文档不改写，历史登记保留）。
7. **7za/7zr 的 wrapper exec 目标前缀照录 null**（C §3.12/3.13 本批未取证）：不编造；probe(L0) 实测后填充全值（sha256_prefix 为空时跳过对照、只实测）。
8. **`{{input_path}}` 暂存语义**：input.input_content（字符串）在场时由 Adapter 写入 in/&lt;input_path&gt;（「输入文件由引擎落 in/」的最小实现）；无 input_content 时期望文件已由上游备好；slot 值经 in/ 前缀校验（canonicalize 消解符号链接 + 词法归一，绝对路径/穿越/逃逸一律 PREPARING 拒绝）。
9. **stdin 请求帧 `{"input", "budget"}`**：run_id 不透传工具（D8.1 原文遵守）；podman_host 的 request.json 含 run_id 属工作区排障文件差异，两协议面并行不冲突。
10. **probe_timestamp 以 ISO 字符串承载**（F §2 为 OffsetDateTime）：tool-engine 依赖面无 time 库（Cargo.toml 零改动红线），与 domain::time::now_iso 同以字符串承载。
11. **capability_ref 全量等值二道校验未挂 trait**：`measured_sha256`/`measured_target_sha256` 为 StaticCatalogProvider 固有方法（不改 F §3 冻结 trait 形状）；派发面以目录前缀 vs 钉值前缀对照承载二道校验，全量等值由 probe 实测后的 fingerprint 承载，完整探测面留 DI-1。
12. **wrapper exec 目标解析 fail-closed**：仅识别 `exec <绝对路径>` 形态、要求唯一目标，无法唯一解析即拒绝派发（NATIVE_WRAPPER_TARGET_UNRESOLVED）；不做宽松猜测。
13. **`cargo fmt --all -- --check` 为 HEAD 既有红**（与本批无关）：2026-11-19 实测 HEAD（改动 stash 后）全仓 661 处漂移（rustfmt 1.81/1.99 双工具链同值，库内 rp-tool.rs/workflow-engine/tests 等历史文件）；本批按文件范围执行 scoped fmt（全部落点文件 `SCOPED-FMT-CLEAN`，tool-engine crate 内漂移仅 lib.rs:27 use 导入序 + podman_host_budget.rs/security_matrix.rs 两处既有），不触碰任务书 §3.1 落点之外文件。
14. **登记面 additive 兼容的实现口径**：validate.rs native_rust 分支在「无任何钉值/白名单字段」时保持旧两字段语义（rp-plugin/rp-knowledge-fetch 名称形态登记不破，sk3 既有断言全绿）；任一钉值/白名单字段在场时 binary 强制绝对路径（sk3 正负例矩阵覆盖两形态）。

## 5. DI-1 前置条件登记（DI-0 ADR §5 依赖链兑现）

- **NativeRustAdapter 就绪**：`execute_native_rust_via_engine(NativeRustCall { engine, run_id, adapter, manifest, capability })` 即 DI-1 PDF 工具端到端派发入口；D3 两形态 allowlist（binary 钉值 / subcommands 白名单 + argv_template）、D4 env 白名单、D5/D6 进程组与取消、D8 工作区三目录与输出帽全部实测（本报告 §3）。
- **RT-0.5-F 契约消费面就绪**：`StaticCatalogProvider::load(manifests/capabilities/host-catalog-seed.json)` + `CapabilityProvider` trait（get/list/fingerprint/probe L0）+ `manifest.capability_ref` 解析与派发前置校验（available+fresh 才放行）；DI-1 的 `pdf.pdftotext`/`pdf.pdfinfo` 引用直接命中种子条目（C §3.6/3.7，16 条实测口径）。
- **Manifest allowlist 登记形状就绪**：validate.rs native_rust 分支 + schemas/tool-manifest/v1.json additive 字段（binary_sha256/binary_target_sha256/subcommands/argv_template/capability_ref）——DI-1 新工具 Manifest 按此登记，钉值补录规范即 RT-4 逐项迁移的形状模板。
- **DI-1 仍需自备**（非 RT-1 面）：工具 Manifest 编写与登记（capability_refs 具体取值）、钉值实测补录（pdftotext 全量 sha256——probe(L0) 可实测后回填）、`resources.cpu/memory` 资源强制参数取值核验（systemd-run/prlimit 路径在 DI-1 生产派发时以 Auto 实测）。

## 6. 遗留与续作入口

| # | 事项 | 归属 |
|---|---|---|
| 1 | L1/L2/L3 探测、TTL/失效事件完整面（F §5）、探测定时预算 | DI-1 前置批（RT-1 ADR D2 分期） |
| 2 | mcp-server/runtime-gateway 派发链接入 native_rust（现状零命中维持，DI-1 经 tool-engine 派发链验证） | DI-1 |
| 3 | 两份既有 native_rust Manifest（rp-plugin/rp-knowledge-fetch）钉值补录与绝对路径迁移 | RT-4 逐项迁移 |
| 4 | 全仓 fmt 漂移（661 处，HEAD 既有）统一格式化 | 独立卫生批（不属 RT-1 落点） |
| 5 | capability_ref 全量等值二道校验挂 trait 面 | DI-1 完整探测面一并裁决 |
| 6 | `ResourceEnforcement::Off` 生产禁用的配置审查面（如派发层配置校验） | RT-3 Runtime Policy Resolver |
