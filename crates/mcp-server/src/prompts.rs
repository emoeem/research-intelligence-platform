//! P9-4 prompt 面（MCP 2024-11-05 prompts/list + prompts/get 子集）：
//! 外部 AI 的操作剧本三条（计划 §11「外部 AI 没有工具时的工作方式」八步的
//! 提示词化），内容为操作指引文本，不含平台数据；数据一律经 tools 与
//! resources 读面按需获取。
//!
//! 形状：prompts/list 返回摘要（name/title/description/arguments）；
//! prompts/get 按 name + arguments 返回 { description, messages }（role=user
//! 单条 text 消息，子集口径）；未知 prompt 或缺必填参数回 -32602。

#![forbid(unsafe_code)]

use serde_json::{json, Value};

/// prompt 参数定义。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PromptArg {
    /// 参数名。
    pub name: &'static str,
    /// 一行说明。
    pub description: &'static str,
    /// 是否必填。
    pub required: bool,
}

/// prompt 定义（prompts/list 条目）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PromptDef {
    /// prompt 名（rip. 前缀，与工具命名空间同口径）。
    pub name: &'static str,
    /// 标题。
    pub title: &'static str,
    /// 一行说明。
    pub description: &'static str,
    /// 参数清单。
    pub arguments: Vec<PromptArg>,
}

/// prompt 词表（固定三条；次序 = prompts/list 次序）。const 不可堆分配，
/// 以函数返回 Vec（list() 单消费面，无热路径）。
fn prompt_defs() -> Vec<PromptDef> {
    vec![
        PromptDef {
            name: "rip.tool_run_playbook",
            title: "工具执行剧本",
            description: "无本地工具的外部 AI 完成一次工具调用的八步剧本（发现目录 → 取 Schema → 执行 → 查状态 → 读证据）",
            arguments: vec![PromptArg {
                name: "tool_id",
                description: "目标工具 id（可选；缺省给通用剧本）",
                required: false,
            }],
        },
        PromptDef {
            name: "rip.run_audit_review",
            title: "运行审计链复核",
            description: "按 run_id 走审计链复核剧本（run 行 → evidence → artifact → trace 锚跨面回查）",
            arguments: vec![PromptArg {
                name: "run_id",
                description: "tool_run.run_id",
                required: true,
            }],
        },
        PromptDef {
            name: "rip.knowledge_brief",
            title: "知识背景简报",
            description: "按主题词生成知识背景简报的取材剧本（knowledge 目录资源 + 检索词设计）",
            arguments: vec![PromptArg {
                name: "q",
                description: "主题词（中文或英文关键词）",
                required: true,
            }],
        },
    ]
}

/// prompts/list（摘要条目）。
pub fn list() -> Vec<PromptDef> {
    prompt_defs()
}

/// prompts/get：按 name 分发生成消息文本；未知 prompt 或缺必填参数回 Err。
pub fn get(name: &str, args: &Value) -> Result<(String, Vec<Value>), String> {
    let arg = |key: &str| -> Option<String> {
        args.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    match name {
        "rip.tool_run_playbook" => {
            let tool_id = arg("tool_id");
            let target = tool_id
                .as_deref()
                .map(|t| format!("目标工具：{t}。"))
                .unwrap_or_default();
            Ok((
                "RIP 工具执行剧本".to_string(),
                vec![json!({
                    "role": "user",
                    "content": { "type": "text", "text": format!(
                        "{target}你是通过 MCP 接入 RIP（Research Intelligence Platform）的外部 AI，\
                         没有本地工具，平台能力由 RIP 提供。按以下八步完成一次工具调用：\n\
                         1. tools/list 分页发现目录（条目为摘要，无完整说明，不要期待 inputSchema）；\n\
                         2. rip.get_tool_schema 按 tool_id 取入参 Schema（按需获取，不整目录拉取）；\n\
                         3. 按 Schema 构造 input，T2 工具必须携带 authorization_ref（缺省按 POLICY_DENIED 拒绝）；\n\
                         4. rip.run_tool 执行（tool_id + input）；\n\
                         5. 从响应取 run_id 与 status（SUCCEEDED 才有 result；失败读 error.category 词表：\
                         VALIDATION / POLICY_DENIED / INFRA / EXECUTION）；\n\
                         6. rip.get_run_status 查终态详情（artifacts / evidence_refs 引用面）；\n\
                         7. resources/read rip://evidence/<evidence_id> 读证据与 Artifact 快照；\n\
                         8. 根据结构化结果继续规划，不猜测未返回的字段。"
                    )}
                })],
            ))
        }
        "rip.run_audit_review" => {
            let Some(run_id) = arg("run_id") else {
                return Err("rip.run_audit_review 需要 run_id".to_string());
            };
            Ok((
                "运行审计链复核".to_string(),
                vec![json!({
                    "role": "user",
                    "content": { "type": "text", "text": format!(
                        "按以下顺序复核 run {run_id} 的审计链，每步都有据可查，不凭记忆下结论：\n\
                         1. rip.get_run_status {{\"run_id\": \"{run_id}\"}} 取 run 行（trace_id / status / duration）；\n\
                         2. 读返回的 evidence 列表（evidence_id / evidence_type / captured_by）与 artifacts 列表；\n\
                         3. resources/read rip://evidence/<evidence_id> 逐条读证据内容与 Artifact 快照文本；\n\
                         4. 用 trace_id 作为链锚：REST 侧 GET /api/traces?id=<关联 ID> 回查同一链（workflow / agent 面），\
                         MCP 侧 resources/read rip://runs/<run_id> 同链引用，两通道同一审计链；\n\
                         5. 结论只写实际读到的字段，缺失面如实标注。"
                    )}
                })],
            ))
        }
        "rip.knowledge_brief" => {
            let Some(q) = arg("q") else {
                return Err("rip.knowledge_brief 需要 q".to_string());
            };
            Ok((
                "知识背景简报".to_string(),
                vec![json!({
                    "role": "user",
                    "content": { "type": "text", "text": format!(
                        "为主题「{q}」生成知识背景简报，全部取材自平台知识面，不凭先验编造：\n\
                         1. resources/read rip://knowledge/catalog 读知识目录（lifecycle/governance 词表在条目内）；\n\
                         2. 从目录条目挑 lifecycle=VERIFIED 且标题/类别命中「{q}」的行（不足时如实说明覆盖缺口）；\n\
                         3. 简报结构：背景一段、可引用条目清单（knowledge_id + title + 生命周期）、缺口说明；\n\
                         4. rip.search_knowledge 尚未接线（isError），不要依赖它取正文。"
                    )}
                })],
            ))
        }
        other => Err(format!("未知 prompt：{other}（词表见 prompts/list）")),
    }
}
