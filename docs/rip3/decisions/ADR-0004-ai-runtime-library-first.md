# ADR-0004：AI Runtime 与知识检索采用成熟库优先

- 状态：Proposed（async-openai Provider 与 LLM Planner 已接入并通过本地离线测试；真实模型在环验收后转 Accepted）
- 日期：2026-11-27
- 范围：Agent Planner、模型 Provider、Token 计量、知识检索与上下文装配

## 背景

RIP 的目标是提供可被外部 AI 客户端/MCP 与内部 Agent 稳定使用的研究知识库和安全工具库。当前代码已有 Tool Engine、MCP Server、Context Broker、统一检索、Evidence/Artifact/Trace 等基础设施；在本 ADR 实施前，`rp-agent` CLI 与 `rp-workflow` Agent 节点固定使用 `KeywordSearchPlanner`。该规划器是确定性规划器，不是真正的 LLM Planner。因此，模型在环完成真实验收之前，现有 Agent 闭环仍不能被描述为已具备完整自主研究能力。

同时，Context Broker 已依赖 Hugging Face `tokenizers`，但默认 tokenizer 是 `bge-small-zh-v1.5`，其用途是 Embedding 输入，不应冒充 DeepSeek、Kimi 或其他生成模型的准确 Token 计量器。

## 决策原则

1. 优先采用维护活跃、被广泛使用、许可兼容且可离线锁定的成熟库。
2. 不重复实现 HTTP Provider 协议、SSE 流解析、Tokenizer 算法、向量存储、全文检索、ONNX 推理或通用 Agent 编排。
3. RIP 自研范围限定在领域模型、权限/策略、检索质量规则、工具治理、证据链、预算策略与业务工作流。
4. 不因引入框架而替换现有 Tool Engine、MCP、Context Broker、Evidence/Artifact/Trace；新库必须通过现有边界接入。
5. 任何新依赖必须先完成版本、许可证、维护状态、传递依赖、MSRV、漏洞公告、离线 vendor 与体积评估；禁止未经评估直接引入大型框架。

## 候选组件与边界

| 能力 | 优先复用 | 在 RIP 中的职责 | 禁止重复实现 |
|---|---|---|---|
| 多模型 Provider | `async-openai 0.43.0` | 通过 OpenAI-compatible API base 接入 DeepSeek、Moonshot/Kimi、OpenAI 与本地兼容端点；JSON mode 与 Provider usage | 自写各厂商 HTTP/SSE 协议 |
| 多模型 Tokenizer | 现有 HF `tokenizers`（0.21.4） | 通过 `RIP_LLM_TOKENIZER_DIR` 加载与实际生成模型匹配的 tokenizer.json；Provider usage 在进程内对后续 preflight 估算做单向保守校准 | 把 BGE tokenizer 当成所有 LLM 的 tokenizer；把估算值冒充精确计数 |
| Embedding/Rerank | 现有 BGE ONNX 模型与容器 Worker | 向量化与重排 | 重写模型推理与预/后处理 |
| 全文检索 | 现有 SQLite FTS5 `search-engine` | BM25/全文召回 | 再造 FTS 索引引擎 |
| 向量存储 | 现有 PostgreSQL + pgvector | 向量持久化、近邻检索 | 自研向量数据库 |
| Agent 执行 | 现有 `agent-engine` + `tool-engine` + MCP | 权限、工具调用、运行状态、审计、证据与恢复 | 同时再引入另一套 Agent Loop/Tool Registry |
| 文档解析 | 现有 Document Router 与已选解析 Worker | 文档格式路由、提取与结构保真 | 自写 PDF/DOCX/HTML 解析器 |

## 实施要求

### A. 模型在环

- 保留 `Planner` trait，新增 LLM Planner 实现；`KeywordSearchPlanner` 仅保留为 deterministic fallback 与测试工具。
- Provider 由配置选择，API Key 仅从环境/密钥文件读取，不写入数据库、日志、Artifact 或前端。
- LLM 只能提出工具调用；实际调用必须经过现有 Tool Engine 的 capability、workspace、caller、预算、审批/执行策略和审计闸门。
- 优先使用 Provider JSON mode；模型同时接收响应 JSON Schema，输出再经严格 Serde 结构与 capability allowlist 校验，不手写 JSON 解析器。
- 模型在环测试使用可选本地测试配置，不进入无密钥 CI；CI 使用 deterministic fake provider 测试协议与安全边界。

### B. Token 计量与上下文预算

- Tokenizer 必须由实际 provider/model 配置选择；通过 `RIP_LLM_TOKENIZER_DIR` 加载匹配的 Hugging Face `tokenizer.json`；Context Broker 与 LLM Planner 共用同一模型 tokenizer 配置。BGE tokenizer 只服务 Embedding/Reranker，绝不复用作生成模型 tokenizer。
- 已审查 `tiktoken 5.5.0`，但本批不引入：DeepSeek/Kimi vocab 子 crate 未提供可被 cargo-deny 识别的 license expression，许可证门禁 fail-closed。待上游补齐许可证元数据或明确 license clarification 后再复核；不通过放宽 deny 规则绕过。
- 优先使用 Provider 返回的 prompt/input token usage 校准实际请求；发送前计量 system、user context、结构化 JSON Schema，并为消息/协议封装保留显式估算开销。
- 未配置模型匹配 tokenizer 时使用保守 Unicode 比例估算，日志必须标记 `estimated` 及算法来源；不得将估算值报告为真实 token 数。配置 tokenizer 时也明确区分 tokenizer 内容计数与消息/schema 封装估算。
- 超预算时拒绝请求并给出诊断，不静默丢弃证据或扩大预算掩盖问题。后续应在检索层按相关性与来源多样性压缩，并保留 citation/evidence 锚。

### C. 知识检索与武器库

- 保留 FTS5 + pgvector + BGE Embedding/Reranker 混合架构，先以真实 QREL 数据验证召回/排序，再考虑替换底层。
- 知识、工具、POC、模板、检测规则、漏洞事实保持不同领域实体；使用统一搜索契约聚合，不合并成无类型大表。
- Agent 默认只读取通过质量、来源、许可、状态与证据治理的条目；原始资料、未审核 POC 和过期模板必须可检索但不能默认执行。

## 验收门槛

1. DeepSeek 与 Moonshot/Kimi 至少各完成一次真实模型调用（用户配置密钥后），并支持结构化工具调用。
2. LLM Planner 的每次工具调用均通过现有 Tool Engine；越权 workspace、未绑定 capability、超预算和未授权执行均被拒绝并留下审计。
3. 同一任务可跨知识卡片、RTIS、POC、Nuclei/ZAP、文档和工具目录检索，输出来源、版本、证据等级、状态与引用。
4. Token 计量报告明确区分 tokenizer 真实计数、Provider usage 和估算值；不得将 Embedding tokenizer 数值当成生成模型用量。
5. 使用真实 QREL 验证 Recall@K、MRR/nDCG；质量门禁通过后才允许扩大 Agent 默认召回规模。
6. 新增依赖通过 `cargo-deny`、MSRV、离线 vendor、`cargo test --offline --workspace`、clippy 与构建门禁。

## CLI 配置示例

```bash
# DeepSeek（API key 本身不要写入命令行参数或仓库文件）
export RIP_LLM_MODEL=deepseek-chat
export RIP_LLM_API_BASE=https://api.deepseek.com/v1
export RIP_LLM_API_KEY_ENV=DEEPSEEK_API_KEY
# DEEPSEEK_API_KEY 由本机 Secret/环境管理器注入
rp-agent run <agent_id> --goal "研究目标" --planner llm

# Moonshot/Kimi
export RIP_LLM_MODEL=moonshot-v1-8k
export RIP_LLM_API_BASE=https://api.moonshot.cn/v1
export RIP_LLM_API_KEY_ENV=MOONSHOT_API_KEY
```

`RIP_LLM_PLANNER=auto|keyword|llm` 为 Workflow Agent-node runtime 提供规划器选择（默认 auto）；当前 `rp-workflow run` 仍按 P8-3 策略 fail-closed 拒绝 Agent 节点，接线不绕过该门。`rp-agent` 可用 `--planner` 直接选择规划器。`RIP_LLM_TOKENIZER_DIR` 可选，指向与模型匹配的 Hugging Face tokenizer 目录；未配置时会明确记录保守估算模式；收到 Provider input usage 后，会在当前 Planner 实例内单向提高后续估算系数，不跨模型持久化。`--planner auto` 在 `RIP_LLM_MODEL` 已设置时使用 LLM，否则带告警回退关键词规划器；`--planner keyword` 可显式选择确定性规划器。

## 当前实施状态与已知差距

- 已接入 `async-openai 0.43.0`（关闭默认功能，仅启用 `rustls` TLS），新增 `LlmPlanner`；CLI 支持 `--planner auto|keyword|llm`，Workflow Agent-node runtime 通过 `RIP_LLM_PLANNER` 复用同一规划器工厂（CLI P8-3 gate 仍保持拒绝）。`auto` 在 `RIP_LLM_MODEL` 存在时启用模型，否则明确告警并回落关键词规划器。
- LLM Planner 每轮只允许一个 next-step；模型只能从 capability_refs 与 Broker L2 tool_briefs 的交集中选择工具，版本只由 CapabilityRef 决定（None 按 Registry current 语义解析，不把 L2 `current` 哨兵转为 SemVer），输入必须是 JSON object。所有执行仍走现有 Tool Engine。Provider 超时、Token 预算拒绝、响应格式错误等进入独立 `Decision::Abort`，Agent Run 终态为 `FAILED` 并写 `agent.run.guardrail` 审计，绝不伪装成正常 `Finish`。
- 新依赖供应链：async-openai 0.30.1 曾因 backoff/instant 的 unmaintained advisories 被拒绝，已升级到 0.43.0；MIT 许可证与 advisories 检查通过；Cargo.lock 从 440 增至 475 个 package，vendor 从 414 增至 449 个目录。选择 OpenAI-compatible client 是为了使用成熟协议实现，同时不引入 `serde_json/preserve_order` 全局 feature 副作用；`canonical_call_key` 仍显式递归排序，确保未来 feature 变化时稳定。
- `agent-engine` 46 项库单测、CLI 测试及闭环集成测试通过；新增真实 AgentLoop 的 Planner Abort→FAILED 状态与审计回归。全 workspace `cargo check --all-targets`、`cargo clippy --all-targets -D warnings`、`cargo test --workspace --no-fail-fast -- --test-threads=1`、格式检查均通过。Provider JSON mode、usage 解析和请求超时由 loopback mock API 测试覆盖。尚未配置真实模型密钥，因此 DeepSeek/Kimi 的真实模型调用和端到端研究任务仍待模型在环验收。运行时配置为 `RIP_LLM_MODEL`、`RIP_LLM_API_BASE`（默认 OpenAI `/v1`）、`RIP_LLM_API_KEY_ENV`（默认 `OPENAI_API_KEY`）、`RIP_LLM_REQUEST_TIMEOUT_SECS`（默认 90 秒）；API key 只从指定环境变量读取，不写入配置/数据库/日志。DeepSeek 示例 base 为 `https://api.deepseek.com/v1`，Moonshot/Kimi 为 `https://api.moonshot.cn/v1`。
- 生成模型 Tokenizer 目前通过 `RIP_LLM_TOKENIZER_DIR` 显式配置；未配置时使用标记为 estimated 的保守估算，不把 BGE tokenizer 伪装成生成模型精确计数。
- 当前跨源统一搜索工具已接入工具目录与 Agent capability/L2 allowlist；本地 Fake Provider 已完成 Broker→LLM Planner→Tool Engine→Artifact/Evidence 的 AgentLoop 闭环集成测试。仍需配置真实模型密钥后，对 DeepSeek/Kimi 执行真实模型在环和跨源研究任务验收；未执行真实 API 请求以避免未经确认产生费用。
- 46 条知识库资料只是当前可见的知识卡片计数，不代表原始来源、RTIS、插件资源、POC 与检测模板总量；必须建立分域库存与质量覆盖统计，禁止只用单一条目数宣称知识库完整。
