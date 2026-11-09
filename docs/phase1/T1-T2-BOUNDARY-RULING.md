# Phase 1 裁决：T1/T2 分界线（V-3 关闭）

- 日期：2026-11-09
- 性质：Phase 1 实施裁决记录（P1-7d 登记前定稿，ROADMAP §2.5 风险 1 原文要求）
- 关闭项：TOOL-CONTRACT-DRAFT §2.4 开放裁决项、§11.2 待验证项 2、PHASE-ROADMAP §14 V-3

## 1. 裁决

T1/T2 分界线以**外部目标写副作用**为准：

| 级别 | 行为面 | 派发要求 |
|---|---|---|
| T0 | 无外部副作用：只读本地文件、只读平台库、纯计算 | 无 |
| T1 | 对外只读交互（HTTP GET、DNS、端口探测）；或平台内部数据面/工作区文件的读写，但无外部目标副作用 | 无 |
| T2 | 改动外部目标系统、向外部发送消息（含远端 server 上可能触发的工具调用） | 必须携带 authorization_ref，Tool Policy Resolver 校验缺失即 POLICY_DENIED 拒绝 |

即：**写平台内部库不升 T2**。rtis.py 的知识库写入路径（M6 口径）维持 T1 语义。

## 2. 理由

1. authorization_ref 链的语义是**对外部目标的授权**（engagement scope、授权窗口），对平台内部写无对应物；平台内部写的管控走既有三轴：audit-engine 事件链（每次写 Emit）、quality-engine 治理双轴、Registry 登记校验。
2. 先例一致性：M6 已有口径 readonly=true → T0、rtis.py（平台库唯一写入实现）T1（docs/M6_REPORT.md:38）；P1-7c 已按同一逻辑登记 rtk ledger（T1，写工作区台账）与 rtk scaffold（T1，写工作区骨架）。
3. 从严面保留：凡副作用可能越出平台边界（远端 server、外部 API、目标系统），一律 T2，与 rip.evidence.rtk.mcp（P1-7c，T2）一致。

## 3. 执行约束（随裁决生效）

1. 平台内部写工具（T1）登记时必须在 Manifest provenance/known_limits 写明写入面与事件链要求；派发产生的写动作必须产生 audit-engine 事件（Phase 1 数据面由后续批次接线）。
2. T2 工具派发必须携带 authorization_ref（tool-engine submit 路径已强制，crates/tool-engine/src/lib.rs:181-184）。
3. RTIS 写路径不登记为可调用工具：rtis.py 编排层（任务状态机/落库）与 rt.py 的写子命令（poc add/use/update）不在 B5 登记面内；B5 只登记只读入口（rt.py poc search 检索路径 mode=ro，rt.py:557；rtis_kb.mjs search；eri_adapter.py bridge-gate 只读复查，审计 JSONL 以 RTIS_ERI_ADAPTER_AUDIT=off 关闭以保持 kit 目录零改动）。

## 4. 待验证项联动

- TOOL-CONTRACT-DRAFT §2.4 的「T1/T2 分界线为草案提议，Phase 1 定稿前标注待验证」：本文件即定稿记录。
- PHASE-ROADMAP §14 V-3（B5 登记前必须定稿）：已关闭。
- 后续批次登记新工具时按本裁决标注 permission_level，不再重复裁决。
