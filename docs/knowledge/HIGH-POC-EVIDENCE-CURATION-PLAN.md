# High 自有 POC 证据审核计划

日期：2026-11-26

## 项目现状审计

- 分支：`main`
- 工作区在任务开始前已有未提交修改；本批未执行 reset、clean、stash、checkout、rebase、提交或推送。
- RTIS 原始数据库：只读打开，SHA-256 保持 `7b6083f8e1e84c9aff60a3d4f429f29cfc436ef1aafa7221d4d3cd945df641a7`。
- 当前 Overlay：`integrations/rtis/curation/k_meta_overlay.json`，Schema 1。
- K0-K4 与 Lifecycle 的权威口径复用已有 overlay validator：
  - K0：只有描述或不足以证明原理的材料。
  - K1：可靠来源，但没有关键现场或实验链。
  - K2：有请求/响应、命令回显或具体证据引用。
  - K3：可重跑的实验及实验/重跑引用。
  - K4：至少两个环境及充分交叉证据。
- Lifecycle 与证据等级独立维护：`Candidate`、`Researched`、`Lab-validated`、`Field-validated`、`Verified` 等不得互相推导。
- 审核机制复用 `k_meta_overlay.json` + 投影中的 `k_meta` 风格元数据，不写 RTIS 原库。
- 当前搜索投影复用 plugin-runtime 的 `knowledge_ingest.meta_json`，并保留 `curation_relations`。

## 真实名单

实时只读查询的 28 条队列为：

`10, 19, 27, 31, 35, 36, 40, 43, 45, 48, 56, 60, 66, 83, 59648, 59649, 59650, 59651, 59655, 900009, 900031, 900033, 900034, 900035, 900042, 900044, 900046, 900048`

筛选口径：`poc.source=self`、`severity=high`、没有 RTIS authoritative `k_meta`、且不在既有 overlay。

## Batch 计划

- Batch 1：接口、鉴权、代理与会话：10, 19, 27, 31, 56, 60, 59655, 900042
- Batch 2：加密、密钥与 Token：35, 36, 43, 45, 48, 66, 83, 900009
- Batch 3：重定向、注入与第三方支付/消息：59648, 59649, 59650, 59651, 900044, 900046, 900048
- Batch 4：隐私、身份与 SSRF：900031, 900033, 900034, 900035, 40

## 证据规则

- 不重做任何公网目标扫描、登录、短信触发或真实数据读取。
- 不把 `poc.verified=1` 或叙述性验证说明直接提升为 K2/K3。
- 每条记录保留原 POC 正文和来源路径；缺失证据明确标记，不猜测补全。
- 历史现场证据与当前生命周期分开维护；修复或回归结果不得抹掉历史证据。
- 本阶段不增加脱敏机制、不扫描凭据、不审查个人信息。
