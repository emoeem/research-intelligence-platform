# MA 轨参考研究 · dsh-agent-teams 与 DSH 插件生态（2026-11-21）

> 用户推荐参考源（2026-11-21）：
> - https://github.com/NanmiCoder/dsh-agent-teams/blob/main/README_ZH.md （v0.1.31，MIT，单插件：队长-团队协作协议）
> - https://awesome-dsh-plugin.com/zh/ （DSH 插件目录站，宣称收录 12352，23 分类）
>
> 性质：外部形态参考；只借鉴协议与机制设计，不引入其代码/依赖/宿主形态（RIP 是 Rust 核心平台，DSH 是 npm/TS 插件宿主）。本页结论进 MA-2/MA-3 任务书的设计输入。

## 一、dsh-agent-teams 可借鉴点（逐条映射 MA 批次）

| # | dsh 机制 | 内容要点 | 映射到 RIP | 处置 |
|---|---|---|---|---|
| R1 | **原子领取任务** | 调度器基于真实 running/idle/ready 状态原子领取一项就绪任务；依赖未完成的任务不可领取 | MA-3 TaskClaim：`claim` 用 SQLite/PG 事务（UPDATE ... WHERE status='ready' RETURNING，行锁保证原子）——**RIP 用数据库事务天然优于 dsh 的单进程串行状态文件**（其 README 明认"多进程并发不保证一致"，RIP 的并发正确性是超越点） | MA-3 采纳，事务化实现 |
| R2 | **持久化邮箱直达消息** | 成员间直达消息，队长无需中转；投递/读取分别记录；未投递消息持久保存后重投 | MA-3 成员消息表（ma_message：to/from/payload/delivered_at/read_at；未投递行由 sweep 重投）；事件推送面（MA-4 SSE）推投递通知 | MA-3 采纳（邮箱模型 + MA-4 推送） |
| R3 | **attempt 模型** | 转派/接管先撤销旧 attempt、等原成员安静再启动；迟到结果无法覆盖；冷重启遗留任务生成新 attempt | MA-2 子任务执行记录（attempt 递增语义，P8-2 workflow_run_node attempt 先例直接沿用）；MA-3 转派/接管（claim 竞争失败方接新 attempt，旧 attempt 结果作废位标记） | MA-2/MA-3 沿 P8-2 attempt 语义扩展 |
| R4 | **toolPolicies 按成员裁剪** | 按成员 id 裁剪全局工具（allow/deny）；队长只能选已声明策略 id，**不能运行时新建** | 与 RT-3 Policy Resolver 输出完全同构（constraints 裁剪成员可用工具面）；"只能选已声明策略"= RIP 的 Tool Registry 既有约束（不进运行时自造）——MA-3 成员建队时绑定 RT-3 决策输出的 constraints，无新机制 | MA-3 绑定 RT-3 输出，零新概念 |
| R5 | **审批流** | 草案 →「确认并启动团队」才真正创建会话调度任务；automatic 保留立即执行路径 | RIP 已有 workflow approval 节点（P8-3 awaiting_approval + resume --input）；MA-2 并行派发加同款预检面：fan-out 计划先出预估（任务数/并发帽/预算），显式确认才拉起——对位 WSW Fuzzer"启动前显示预估"纪律 | MA-2 采纳（派发前预估+确认，预算面强制） |
| R6 | **kind=review 质量复审任务** | 任务类型 work/review 二分；失败自动修复/复审；默认顺序需求→实现→验证→审查→集成 | MA-3 任务类型词表加 review（复用既有 evaluator/verification 五态语义，不自造复审体系）；默认管线顺序进 MA-3 任务模板 | MA-3 词表采纳 |
| R7 | **异构模型成员** | add_member 可选 provider/model/reasoning_effort；默认零交互继承队长配置 | MA-2 子代理规格面：成员 agent 可绑定不同模型/推理档（对位本会话 workflow 子代理 model 机制）；RIP 的模型接入走既有 provider 面 | MA-2 采纳（成员模型可配） |
| R8 | **固定协议驻留，不重写系统提示词** | 队长协议固定驻留，建队/结束不重写，压缩会话不丢核心规则 | 对位 context-broker 装配面：协作协议作为稳定上下文层（非每次拼装）；MA-3 设计注意点：协议文本进 stable 上下文层，成员切换消息不冲刷 | MA-3 设计注意点 |
| R9 | **实时面板（可交互 DAG、成员树、模型标注）** | 团队执行可视化 | MA-4 Runs 页扩展（caller/成员维度 + DAG 状态）；沿 docs/ui-ux 体系不另建 | MA-4 UI 面 |

**dsh 的弱点（RIP 的超越点，写进 MA 任务书的反面清单）**：
1. 状态文件单进程串行，多进程并发不保证一致 → RIP 用数据库事务（MA-3 claim/消息全事务化）。
2. 一个队长同一时间只带一个活动团队 → RIP 无此限制（per-caller 预算帽管并发即可）。
3. 模型可能完成工作但未按协议更新任务状态 → RIP 的 ToolRun/attempt 行由引擎侧落账，不依赖成员自觉（R2/R3 引擎侧记账）。
4. 成员 memberMaxDepth=0 不能再建子代理 → RIP 允许受预算约束的嵌套（per-caller 剩余预算传递），深度帽进约束。

## 二、awesome-dsh-plugin 生态形态可借鉴点（对位 Plugin Center / P10-2 已有面）

| # | 形态 | 内容 | 对 RIP 处置 |
|---|---|---|---|
| E1 | **目录站分类**：23 分类（工具与能力 1748 / 会话与消息 962 / 工作流与自动化 750 / 安全与权限 315 / 记忆 519…） | 插件元数据单分类 + 多维筛选 + 下载/Star 排序 | P10-2 已有 catalog.json 发布形态；MA-3 之后如果做 Plugin Center 扩展，采纳"固定分类词表 + 单分类归属"口径（不自造 23 类，按 RIP 工具族词表）——登记为 Plugin Center 后续批参考，不在 MA 轨内 |
| E2 | **收录即 PR + YAML 元数据**（data/plugins/*.yaml） | 声明式收录 | 对位 RIP Manifest JSON 形态，已有；无需动作 |
| E3 | **安装即运行第三方代码的警示 + 建议 lock commit** | 安全姿态 | P10-2 trust_policy 四档确认强度已覆盖且更严（RIP 有信任分级确认面）；无需动作 |
| E4 | **市场内浏览/搜索/安装**（dsh-market） | 宿主内市场 | 同 E1，Plugin Center 后续批参考 |
| E5 | 有参考价值的单插件形态（非 MA 相关）：cc-safety-net（破坏性命令拦截）、dsh-ops、free-search（多 key 轮换）、mnemon/hindsight（记忆银行按仓库隔离） | — | 登记观察项：safety-net 的拦截清单思路可对照 RT-3 不安全降级负例词表；记忆隔离口径对位 caller 维度——均不在 MA 轨内实现，登记进 Plugin Center 观察池 |

## 三、MA 轨设计输入增补（本页结论的消费方式）

1. MA-2 任务书新增设计输入：R5（派发前预估+确认）+ R7（成员模型可配）+ attempt 沿 P8-2 语义。
2. MA-3 任务书新增设计输入：R1（事务化原子领取）+ R2（邮箱消息表+重投）+ R4（成员工具面=RT-3 constraints 裁剪）+ R6（review 任务类型）+ R8（协议驻留 stable 层）；反面清单四条作为验收对照（RIP 必须逐条优于 dsh 实态）。
3. MA-4 UI：R9 DAG/成员树形态进 Runs 页扩展设计。
4. Plugin Center 扩展（E1/E4/E5）登记观察池，不进 MA 轨范围。
5. 引用纪律：外部形态参考只借鉴语义；实现全部 Rust-First 既有栈，零新依赖方向（dsh 是 npm/TS，代码零复制）。

## 四、核实记录

- 两源均为 2026-11-21 WebFetch 实抓（README_ZH 全文概要与目录站首屏 24 插件 + 23 分类计数）；dsh-agent-teams 版本 0.1.31 / 宿主 0.2.2-rc.5 / MIT；目录站 12352 声称收录量未核验（非本页结论依赖项）。
- 本页为形态参考记录，无代码引用；后续 MA-2/MA-3 任务书撰写时把 §三各行作为"设计输入必读"引用本页。
