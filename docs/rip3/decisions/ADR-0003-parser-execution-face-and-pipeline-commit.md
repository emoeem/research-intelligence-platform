# ADR-0003：Phase 2 解析执行面与管线提交模式（P2-1）

- 状态：已裁决（2026-11-15，P2-1 批内形成）；关联：R2 §4.1/§4.3/§4.6/§10.3、
  12/13 号冻结物、ADR-0001（绞杀者）、ADR-0002（Docling/PyMuPDF 选型台账）、
  交接文档第七节（开批范围五项）
- 评审入口：docs/rip3/batches/P2-1-TASK.md（任务书）

## 一、背景

Phase 2 需要决策三件事：解析容器族如何落地；Parser Registry/Provider 接口落在
哪个 crate、经哪条通道执行容器；解析任务生命周期（九态）为 workflow 定义时，
PG 侧 blocks/assets 落库发生在管线哪一步。

## 二、候选与裁决

### 1. 容器族形态

- 候选 A：按 Tool Pack 惯例建 `containerfiles/rip-parser/`（一镜多 op，argv 分派
  入口脚本 + manifest 双份，digest 台账三处同步）。
- 候选 B：每格式一族镜像。
- **裁决 A**。理由：拆分原则（不为全部工具造巨型容器的前提是同构依赖；解析各族
  依赖同为 Python/ML 栈，异构拆分无收益）；继承镜像族既有惯例（基座 digest 同
  `python:3.12-slim@sha256:4840d514…`、network=none、构建期受控拉取）；Docling
  模型与 tessdata 单点烘焙，模型升级 = 换 digest 重登记。

### 2. Parser Registry 落点与执行通道

- 候选 A：新建 `crates/doc-intel` 独立子系统 crate。
- 候选 B：`crates/intel-db` 增 parser 模块（traits + registry + IR 校验 + 提交
  服务），容器派发复用 tool-engine/runtime-gateway 既有 T1 podman_host 面。
- **裁决 B**。理由：交接文档验收骨架指定 `-p intel-db --test parser_pipeline`
  （管线测试随 document domain 同 crate）；intel-db 已是情报核 crate（R2 4.1
  「独立子系统」落在模块边界与 ADR 登记，不以新 crate 为必要条件）；新增依赖
  （tool-engine/tool-registry/runtime-gateway）与 intel-db 无环（三者均不依赖
  intel-db）。Provider trait（DocumentParser/OCRProvider）仍是领域级接口，容器
  派发实现与 trait 分离，换执行面不动 Registry。

### 3. 解析执行是否复用 tool-engine Gateway

- 候选 A：intel-db 内直调 podman（自管隔离参数）。
- 候选 B：解析容器登记为 rip.parser.* 工具（T1 podman_host manifest），经既有
  Gateway::dispatch 派发。
- **裁决 B**。理由：原则 G（Podman 隔离执行面是地基）：隔离全套（read-only/
  cap-drop/network none/资源帽/tmpfs）、看门狗、预算帽、FamilyLimiter、tool_run
  审计与 trace 六 ID 关联全部继承，候选 A 是第二套容器执行逻辑（R2 §17.3 禁止
  事项邻接面）。rp.parser.docling 多 op 一 Manifest（argv 分派），T1 权限。

### 4. 九态管线与 PG 落库的提交点

- 候选 A：扩展 workflow-engine 新节点类型（如 ir_commit 节点）在 run 内落库。
- 候选 B：run 内 Tool 节点产出 IR（artifact）；驱动侧提交服务在 run 终态后落库
  （幂等键 revision_id + IR content_hash；IR JSON 文本进 CAS 可重放）。
- **裁决 B**。理由：新节点类型 = 词域扩展 + 016 迁移 CHECK 同步 + 六执行器面
  改动，为单一管线引入执行器变体违反最小可审计修改（R2 §17.4）；checkpoint/
  断点恢复/重试继承 workflow-engine 既有面（R2 §4.6.1 硬性），提交服务无状态、
  可重放，run 重启不丢任务。失败态（FAILED/CANCELLED/QUARANTINED）走 run 终态
  与 quarantine 分支，失败原因在节点日志（017 面）。
- 九态映射表（审计任务 5 补交付）见 P2-1-TASK.md 第一节 3 项，不再重复。

## 三、成本与风险

- intel-db 依赖面增重（+tool-engine/tool-registry/runtime-gateway 传递 database
  等既有 workspace 内 crate，无新外部 crate 则 vendor 零增量；有新增按手工增量
  vendor 口径）。
- Docling 镜像体积大（torch 栈，预估 4 到 7 GiB）：在 50 GiB 镜像预算内，构建后
  `podman system df` 复核入批次报告 F 节。
- 提交服务在 run 终态后落库：若驱动进程在 run 成功与落库之间死亡，落库由重放
  （幂等键）补齐，不重解析；该窗口在 selftest 用例覆盖。

## 四、放弃的方案与原因

- 每格式一族镜像（候选 1B）：镜像数量爆炸，违背拆分原则。
- doc-intel 新 crate（候选 2A）：骨架测试落点与最小修改原则冲突；待 Phase 3
  知识抽取面展开后再评估是否独立（届时 ADR 滚动）。
- 直调 podman（候选 3A）：第二套隔离实现，原则 G 禁止。
- run 内落库节点（候选 4A）：执行器面改动面大，checkpoint 语义被提交副作用复杂化。
