# P5-5 批次报告：Provider Adapter（RTIS POC + Nuclei，统一契约测试）

> 批次类型：实现批。规格源：Phase 5 补充计划 §7 P5-5 + §5.6；审计依据：
> P5-0-BATCH-REPORT.md 差距清单 G4。

## A. 本批完成内容

1. **Provider Adapter 层**（crates/verification-engine/src/provider.rs）：
   - `ProviderOutcome`（信封守卫：SUCCEEDED 必有结构化 result 且不带
     error_category；FAILED 必带分类；工具/规则版本恒在）；
   - `VerificationProvider` trait：capability() 产出 provider-capability.v1
     文档（过 P5-2 契约校验）；
   - **RtisPocProvider**（transport=cli_subprocess）：resolve_metadata
     （rt.py poc get --json 受控引用与元数据）+ record_use（rt.py poc use
     使用关系回填——RTIS 现有接口 sanctioned 写路径，与 V-3「Manifest 面
     只读」裁决不冲突：本 Provider 走 CLI 不走 Tool Manifest）；
   - **NucleiProvider**（transport=tool_engine，container_required=true）：
     `execute_nuclei` = submit（D-TC-05 受理）→ podman_host Adapter
     `execute_container_via_engine` 容器派发（digest 钉死镜像 + egress
     白名单 + 每族并发 1）→ 信封交付 → ProviderOutcome 映射
     （TIMEOUT→Timeout，DEPENDENCY→ToolUnavailable，VALIDATION→Assertion）。
2. **rip-nuclei 镜像族**（containerfiles/rip-nuclei/）：
   基础镜像 docker.io/projectdiscovery/nuclei@sha256:582d5546...（v3.11.1，
   MIT，成熟库优先纪律——不自研扫描引擎）；wrapper 脚本承载三件运行面约束：
   keep-id 非 root 写面收敛（HOME/XDG 钉 /task/out）、.nuclei-ignore 空表
   就位（v3.11.1 对缺失 ignore 的 ENOENT 会把全部模板过滤出局的坑位实录）、
   调用面模板（/task/in/template.yaml）装入默认模板目录（不联网装模板不跑
   全量）；D-TC-03 result 帧协议（stdout 第一行 {"status":"result","data":
   {matched_count,nuclei_version}}）；镜像 digest
   sha256:19ecdd8053172cf474641fcd7ecc703de78648b46172245781d700f2537da5c7
   三处同步（Manifest yaml/json + provider 常量）。
3. **Manifest rip.security.nuclei**（yaml+json 双份，rp-tool validate 九条
   全过）：podman_host runtime + image digest 钉定 + entrypoint wrapper +
   egress_allowlist（ports 80/443/8080/8443，派发面按 target 派生临时
   条目）+ input_schema（target/url/template_ref/rate_limit/timeout_seconds/
   max_requests 预算双闸）+ output_schema（matched_count；命中数不是
   Finding，五态判定在 P5-7 evaluator）。
4. **统一契约测试**（tests/provider.rs）：capability 双文档过 v1 Schema +
   信封守卫三态（SUCCEEDED 无 result 拒/FAILED 无分类拒/版本恒在）+
   **Nuclei 容器派发链端到端**（真实 podman：本地模板装载 → 无命中干净跑
   → SUCCEEDED + matched_count=0 + tool_run_id/tool_version 在位；V-9
   守卫 podman/镜像缺席跳过）。
5. **平台级 bug 修复实录**：tool-engine podman_host adapter 写 "POLICY"
   （5.3 词表外）→ 信封校验必炸——修为 POLICY_DENIED（词表内，语义一致），
   属 P4-4 三 bug 同族（egress 派发面首次实跑暴露）。

## B. 架构变化和复用说明

- 执行链零重建：submit/状态机/信封/容器派发/帧协议全部复用 tool-engine
  既有面（verification-engine 单向依赖 tool-engine，反方向禁止）；
  containerfiles digest 台账惯例沿用（rip-parser 等先例）。
- RTIS 红线：POC 正文/利用细节零复制（provider 层只透传元数据引用，
  rtis_poc_id 整数 ID 即锚）；rt.py 零修改（使用关系经 sanctioned CLI 写
  路径，knowledge.db 本身 gitignore 运行时数据）。

## C. 修改文件清单

```text
crates/verification-engine/src/provider.rs             # 新增（Adapter 层）
crates/verification-engine/src/lib.rs                  # provider 注册
crates/verification-engine/tests/provider.rs           # 新增（契约测试 + e2e）
crates/verification-engine/Cargo.toml                  # tool-engine 依赖 + dev-dep tool-registry
crates/tool-engine/src/adapter/podman_host.rs          # POLICY → POLICY_DENIED（词表修复）
manifests/tools/rip.security.nuclei.yaml/.json         # 新增（九条校验过）
containerfiles/rip-nuclei/Containerfile + wrapper.sh   # 新增（镜像族）
crates/context-broker/tests/acceptance_no_stuffing.rs  # 登记面钉值 24→25
crates/context-broker/tests/catalog_sources.rs         # 覆盖面钉值 24→25（四处）
```

## D. 数据库迁移、Schema 和兼容性变化

零迁移。Tool 登记面 +1（25 份 Manifest）——两处登记面覆盖钉值随链更新
（P4-3 钉值先例同族）。output_schema required 收窄为 ["matched_count"]
（jsonl_excerpt 全文走 artifact 承载，不进 result 帧）。

## E. 测试命令及真实结果

```text
target/debug/rp-tool validate --manifest manifests/tools/rip.security.nuclei.yaml   # ok:true
cargo test --offline -p verification-engine    # 25 lib + 5 integration（provider 2 + store 4）全过
cargo test --offline --workspace               # 136 套件 0 FAILED
cargo clippy --offline --workspace --all-targets -- -D warnings  # 零告警
# e2e 实拍（本地靶场无命中面）：nuclei 容器派发 → SUCCEEDED + matched_count=0
#   + tool_run_id=rn-p55-e2e + tool_version=v3.11.1（镜像 -version 实拍 v3.11.1）
# RTIS 实拍（sanctioned 面）:
python3 integrations/rtis/rt.py poc get --id 900008 --json   # 元数据面（verified=1）
python3 integrations/rtis/rt.py poc use --id 900008 --on rip3-p5-provider-smoke --json
# integrations/rtis/ 零修改（git status 核对：跟踪文件零 diff，adapter_audit.jsonl
# 为跟踪态运行时追加，本批未跑 rtis.py test 不触发）
```

## E2. CI 各 Job 状态和运行链接

随批 push 后以 HEAD run 为准（e2e 在 CI 无 podman 守卫下按 V-9 SKIP，
本地真跑为准——与 podman_host_e2e 同口径）。

## F. 性能数据

- Nuclei 容器派发冷跑（含镜像解析 + workspace 准备 + 无命中扫描 + 帧交付）
  实测约 1.2-2.0s（e2e 单测 2.14s 含两次测试用例）；nuclei 扫描本体
  139µs-2.3ms（无命中面）。
- 每工具族并发默认 1（ADR §7 家族闸）继续生效（family=network）。

## G. 未解决问题与风险

1. **egress_allowlist 派发面**：容器端到端实拍走 NetworkPlan::None
   （127.0.0.1 本机面）；EgressAllowlist 链路（pasta -T + 代理 env + host
   派生）的本地靶场真实目标实拍随 P5-6 执行调度批次登记（此前发现并修复
   的 POLICY 词表 bug 即该链路首跑暴露）。
2. nuclei 模板选取当前按 template_ref 锚整文件落位（in/template.yaml）；
   多模板批量与标签过滤（-tags/-severity 词）随真实 POC 模板化批次扩展。
3. ZAP Adapter 未接入（§5.6 优先序第五位，completion 判据「至少一个成熟
   扫描 Provider」由 Nuclei 满足）；如实登记为 Phase 5 收口后的扩展项。

## H. 下一批依赖和明确的非目标

- P5-6 依赖：execute_nuclei（本批）+ PlanStore 状态面（P5-2）+ workflow
  control 语义参照（018 面复用判定）。
- 非目标：不做计划级批量派发（P5-6）、不做五态判定（P5-7）、不接 ZAP、
  不动 runtime-gateway 内核。

## I. UI 截图与浏览器验收证据

不适用（Provider 面无 UI；Scan Center 执行面随 P5-6 落地）。

## J. RTIS 对账和数据安全回归结果

- rt.py poc get/use 实拍走 sanctioned CLI；integrations/rtis 跟踪文件零
  diff（git status 核对）；POC 正文零复制（provider 层只透传元数据引用）。
- 15155 对账口径不受影响（RTIS 侧零行为变更）。
- 镜像 digest 三方一致核对（Containerfile/Manifest/provider 常量）完成。
