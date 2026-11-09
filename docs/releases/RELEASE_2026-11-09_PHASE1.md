# 更新说明 · 2026-11-09（RIP 2.0 Phase 1：Tool Contract 与 Registry）

> 工具装备体系的地基落成：Tool Manifest/调用/结果三份 v1 Schema 冻结，28 份首批工具清单登记在册，tool-registry 与 tool-engine 两个新 crate 落地（含 python_worker 真实适配器），005/006 两份迁移进库。从此每个工具都有统一身份、Schema、版本与校验路径，Agent 不能再拼装任意宿主机命令。

| 项         | 值                                                                  |
| ---------- | ------------------------------------------------------------------- |
| 阶段交付   | git `962cfa9`（22:24 CST）                                          |
| 数据库迁移 | `005_tool_contract.sql`、`006_agent_data_plane.sql` 在 migrations/dev-*.db 测试副本验证（幂等重放通过）；生产 platform.db 未触及 |
| 自动化测试 | 提交内专项实测 105 测试全绿（cargo test -p tool-registry -p tool-engine -p domain -p database --offline）；整仓门禁 cargo test --workspace 全绿、worker unittest 125 全绿（跳过 11） |
| 静态检查   | cargo clippy --workspace --all-targets `-D warnings` 零告警          |
| 独立验收   | 通过（对照 2.0 计划 §14 Phase 1 三条验收逐条给证据，含复用检查）     |

## 一、交付了什么

1. **三份 v1 Schema 与示例**（schemas/tool-contract/request-v1.json、result-v1.json；schemas/tool-manifest/v1.json 加 examples/）：jsonschema 库级验收通过（SCHEMA_VALID；缺 provenance 反例被正确拒绝）。与附录 A 有两处已文档化偏差：runtime.image 与 provenance.image_digest 的 pattern 允许 `<digest>` 占位形态（否则设计稿示例无法过校验），占位与 tag 引用的拒绝属 Registry 校验器职责。
2. **domain 扩展**（crates/domain/src/tool.rs、agent.rs、ids.rs）：ToolRunStatus 八态枚举（含四终态判定）、RunId/ArtifactId 新类型（maxLength 128 对齐 request-v1）、DomainError 三个新变体。
3. **crates/tool-registry**：manifest 校验（含落库九条）、注册、哈希、目录查询；首批 28 份工具清单登记（manifests/tools/，json/yaml 双份）。
4. **crates/tool-engine**：envelope、request、state（跃迁校验，TOOL-CONTRACT §3.2 跃迁表）、mock_runner、python_worker Runtime Adapter（790 行：进程池、initialize/get_capabilities 握手、execute/cancel 转发、stderr 日志采集，打通真实 parsing worker）、structured_output 适配（722 行）；契约测试 contract_min_set.rs 与 first_batch_b2/b5。
5. **迁移 005/006**：tool_run（八态 CHECK）与 artifact 两表幂等 DDL、agent 数据面表、schema_version 与 PRAGMA user_version 双写。
6. **vendor 增量重生成**：新依赖 jsonschema 等走增量 cargo vendor，`--offline` 构建保持绿（2433 文件变更的大头是 vendor 目录，源码变更见提交清单）。
7. **边界裁决**：docs/phase1/T1-T2-BOUNDARY-RULING.md 记录本阶段范围裁决依据。

## 二、为什么这样做

- Registry 先于执行面：Phase 1 的验收红线是「不执行任意宿主机 Shell」，所以本阶段只有 Mock Runner 与 python_worker（既有 worker 进程，非 shell 派发）；容器执行留到 Phase 2 的 podman_host 适配器。
- Schema 校验用库（jsonschema）不手写：通用逻辑优先成熟开源库，依赖增量 vendor 保证离线可构建。

## 三、已知取舍

- 两个示例 Manifest 的镜像 digest 保持 `sha256:<digest>` 占位，待 Phase 2/3 镜像构建后回填。
- evidence 抽样 4/26、research_task 核心字段映射等 M8 已知限制不在本阶段范围（属清账工作流）。
- 跃迁合法性校验落位 tool-engine（P1-5），domain 只承载状态身份，这是有意分层。

## 四、回滚方式

`git revert 962cfa9`；迁移回退：005/006 为纯新增表，测试副本上 drop 对应表即可，生产库未应用无需回退。
