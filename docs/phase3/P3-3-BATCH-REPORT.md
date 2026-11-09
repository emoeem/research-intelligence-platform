# 批次验收报告：P3-3（rip.http.inspect 登记与端到端）

- 批次：P3-3（第 4/7 批，ROADMAP P3-2 本体；2026-11-10）
- 修改范围：containerfiles/rip-tool-webresearch/（http_inspector.py 对齐 §9.1 + tool-manifest 双份新增）、crates/tool-engine（Cargo.toml dev-dep、tests/web_research_e2e.rs 新增、tests/podman_host_egress.rs schema 对齐）、docs/phase0/TOOL-CONTRACT-DRAFT.md（占位回填 + §11.1 消解）、docs/phase0/PHASE-ROADMAP.md（V-10 关闭）、containerfiles/README.md（digest 台账）
- 修改文件：见上；镜像因脚本变更重建（digest sha256:bf910119b26426c107e9b1adfd722def3473fa1e260e0f447a9514e57f365f91，台账与 Manifest 双份与 podman inspect 三处一致）
- 新增接口：无（Gateway::dispatch 与 register_node 为 P2-4 既有面，本批首次承载真实容器工具全链；tool-engine 新增 dev-dependency runtime-gateway）
- 数据变更：开发库副本 /tmp/rip-p33-devcopy/platform-devcopy.db（migrations 001 到 007 应用的新建文件库，非生产库副本）：tool 1 行 + tool_version 1 行（rip.http.inspect 0.1.0，provenance_json 非空）+ tool_run 3 行（SUCCEEDED/HEAD/nosave）+ artifact 行（primary body.bin sha256=25ddf2c8...e484 与落盘文件 sha256sum 一致，577 字节）；生产 platform*.db 零写入（红线）

## 计划 §4.3 三项验证（接入标准，逐项记录）

1. **源码检查**：http_inspector.py 为 Python 纯标准库（urllib.request/json/hashlib，零第三方依赖，镜像内零包安装）；T1 只读面（method 枚举 GET/HEAD，POST 拒绝；scheme 白名单 http/https；逐跳头拒绝）；帧协议 D-TC-03；错误帧 -32700/-32602/-32006 且 stderr 镜像；无凭据处理路径（headers 限键格式，Authorization 类头部按 §9.1 描述走 secrets 机制）。
2. **最小功能验证**：容器内 GET https://example.com/ 返回 200（经 Gateway::dispatch 全链，crates/tool-engine/tests/web_research_e2e.rs 实测）；HEAD 返回 200 且 body_bytes=0；save_body=false 不落响应体；负向：POST 被 input_schema 在提交面拒绝、非白名单 host 的 CONNECT 被代理 403 拒（P3-2 套件回归）。
3. **输出格式验证**：output_schema 与 §9.1 对齐（required status_code/final_url + headers/body_bytes/body_artifact_id）；结果行经引擎 output_schema 校验后入库（不合 schema 即 FAILED，web_research_e2e 内 SUCCEEDED 即校验通过的证明）；body_artifact_id 与 artifact 表行（art-rn-p33-ok-0-body.bin，role=primary）对账一致。

## 测试命令与结果（实测）

- `cargo test --offline -p tool-engine --test web_research_e2e`：2 passed 0 failed（gateway_dispatch_full_chain_get_with_body_artifact + manifest_matches_contract_section_9_1_shape）
- `cargo test --offline -p tool-engine --test podman_host_egress`：5 passed（schema 对齐后回归）
- 开发库副本只读对账（python3 -I，sqlite mode=ro）：tool_run 三行 `('rn-p33-ok','SUCCEEDED','rip.http.inspect')` 等；artifact.sha256 与 `sha256sum` 一致
- 全量门：cargo test --offline --workspace 全绿、clippy -D warnings 零告警、cargo fmt --check 通过、worker unittest 125 全绿（跳过 11）

## 未执行的测试

- VM 面（podman_vm）字面派发：S1 到 S4 维持挂起（非本批范围）
- http_api runtime 首例：TOOL-CONTRACT §11.5 维持待验证（本工具为 podman_host 类型）

## 对 ERI / RTIS / KB / Skills / Tools 的影响

- RTIS 本体零改动（红线）；ERI/KB 无接触；Tool Arsenal 新增首个 podman_host 真实工具登记（开发库副本）

## 风险

- example.com 依赖外网可达性（T1 单请求，低并发纪律）；npmjs.com 重定向依赖见 P3-2 caveats
- 镜像 digest 与本机 containers-storage 绑定：换机重建须按 containerfiles/README.md 台账流程重取同步

## 回滚方式

- 代码：`git revert <批次 commit>`（tool-engine dev-dep 与两测试文件随批回滚，e2e/budget/recovery 的 ContainerCall 构造点在 P3-2 已同步，回滚需成对）
- 文档：TOOL-CONTRACT 回填与 V-10 行随 commit 回滚即恢复占位与待验证原文
- 开发库副本：/tmp/rip-p33-devcopy/ 整目录删除即清净（非生产库，无迁移回滚面）

## 后续工作

- Core Utilities Pack 登记（P3-3 原表内 P3-4 批次：五 Manifest 双份 + 校验九条）
- 端到端收口（P3-4/P3-5）：两包全工具 run + Artifact 回收对账；digest 回填核对其余占位（本批已完成 TOOL-CONTRACT 全部 <digest> 清零，P3-6 复核）
