//! Broker 链接层：Agent 每轮上下文的唯一入口（计划 §9:454-477 第 2 步）。
//!
//! 装配全量复用 context-broker `Broker::assemble`（L0 规则、L1 目录、L2
//! 知识摘要与工具 brief、L3 Artifact 引用与按需 expand，预算与清单留痕在
//! Broker 内完成）；本层只补两件事：
//! - ambiguous 意图确认回路（[`IntentConfirmer`] 端口）：装配意图为
//!   ambiguous 时回调确认者，确认者给出确认词即重装配并留痕轮次；放弃
//!   确认则按 ambiguous 继续并在 warnings 留痕（不静默放过）；
//! - [`BrokerLink::expand`] 透传：L3 按需展开记账（EXPANDED 事件进
//!   context_ref_log，l3_reserved 扣账在 Broker）。
//!
//! 边界：本层不读任何库表（Agent 禁自查库，P7-3 判据同源）；确认回路不
//! 伪造意图状态，确认后的 re-assemble 意图仍 ambiguous 就如实保持。

use crate::error::AgentError;
use context_broker::{AssembleRequest, AssembledContext, Broker, Expanded, IntentConfig};

/// 意图确认者端口（P7-2：CLI/测试注入；P7-3 起接模型在环或人工回路）。
pub trait IntentConfirmer {
    /// ambiguous 时回调：返回确认词（追加进目标文本后重装配）或 None
    /// （放弃确认，按 ambiguous 继续）。
    fn confirm(&mut self, alternatives: &[String], goal: &str) -> Option<String>;
}

/// 内置确定性确认者：取第一候选族在 IntentConfig 中的第一个触发词回填
/// （IntentConfig 公开字段；族命中置信度 +0.4，规则面见 intent.rs:388）。
#[derive(Clone)]
pub struct FirstFamilyConfirmer {
    config: IntentConfig,
}

impl FirstFamilyConfirmer {
    /// 用意图配置构造。
    pub fn new(config: IntentConfig) -> Self {
        Self { config }
    }
}

impl IntentConfirmer for FirstFamilyConfirmer {
    fn confirm(&mut self, alternatives: &[String], _goal: &str) -> Option<String> {
        let first = alternatives.first()?;
        self.config
            .families
            .iter()
            .find(|f| &f.id == first)
            .and_then(|f| f.triggers.first().cloned())
    }
}

/// 装配结果（最终上下文 + 确认回路留痕）。
#[derive(Debug, Clone)]
pub struct AssembleOutcome {
    /// 最终装配上下文（确认回路每轮新 manifest，只追加语义）。
    pub context: AssembledContext,
    /// 实际发生的确认轮次（确认词原文，按发生顺序）。
    pub confirmations: Vec<String>,
}

/// Broker 链接层。
pub struct BrokerLink {
    broker: Broker,
    max_confirm_rounds: usize,
}

impl BrokerLink {
    /// 构造（确认轮次上限默认 2）。
    pub fn new(broker: Broker) -> Self {
        Self {
            broker,
            max_confirm_rounds: 2,
        }
    }

    /// 覆盖确认轮次上限。
    pub fn with_max_confirm_rounds(mut self, rounds: usize) -> Self {
        self.max_confirm_rounds = rounds.max(1);
        self
    }

    /// 装配（含 ambiguous 确认回路）：
    /// 1. 按原文装配；意图 ok 即返回；
    /// 2. ambiguous：回调确认者；给确认词则追加进目标文本重装配（新
    ///    manifest，确认词留痕 [`AssembleOutcome::confirmations`]），弃权
    ///    或达轮次上限则按 ambiguous 继续 + warnings 留痕。
    pub fn assemble_confirmed(
        &self,
        req: &AssembleRequest,
        confirmer: &mut dyn IntentConfirmer,
    ) -> Result<AssembleOutcome, AgentError> {
        let mut text = req.text.clone();
        let mut confirmations: Vec<String> = Vec::new();
        let mut context = self.broker.assemble(req)?;
        for _round in 0..self.max_confirm_rounds {
            if context.intent.status != "ambiguous" {
                return Ok(AssembleOutcome {
                    context,
                    confirmations,
                });
            }
            let Some(word) = confirmer.confirm(&context.intent.alternatives, &text) else {
                context.warnings.push(
                    "intent ambiguous: confirmation declined; proceeding with ambiguous context"
                        .to_string(),
                );
                return Ok(AssembleOutcome {
                    context,
                    confirmations,
                });
            };
            confirmations.push(word.clone());
            text = format!("{text} {word}");
            context = self.broker.assemble(&AssembleRequest {
                task_id: req.task_id.clone(),
                text: text.clone(),
                correlation_id: req.correlation_id.clone(),
                trace_id: req.trace_id.clone(),
                actor: req.actor.clone(),
            })?;
        }
        if context.intent.status == "ambiguous" {
            context.warnings.push(format!(
                "intent ambiguous after {} confirmation rounds; proceeding with ambiguous context",
                confirmations.len()
            ));
        }
        Ok(AssembleOutcome {
            context,
            confirmations,
        })
    }

    /// L3 按需展开透传（EXPANDED 记账进 context_ref_log，l3_reserved 扣账
    /// 与拒绝面在 Broker）。
    pub fn expand(&self, manifest_id: &str, item_id: &str) -> Result<Expanded, AgentError> {
        Ok(self.broker.expand(manifest_id, item_id)?)
    }

    /// 清单视图透传（对账/测试用）。
    pub fn manifest(&self, manifest_id: &str) -> Result<context_broker::ManifestView, AgentError> {
        Ok(self.broker.manifest(manifest_id)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use context_broker::{ArtifactSource, ManifestStore, RegistrySource, RulepackSource};
    use database::{DbPath, Pool, PoolConfig};
    use std::path::PathBuf;
    use std::time::Duration;

    fn migrated_pool() -> Pool {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .unwrap();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(&pool, &dir).unwrap();
        pool
    }

    fn broker_link(pool: &Pool) -> BrokerLink {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rulepack_dir = root.join("rulepacks/rip-rules");
        let rulepack = RulepackSource::from_dir(&rulepack_dir).unwrap();
        let intent_config = IntentConfig::from_dir(&rulepack_dir).unwrap();
        let index =
            context_broker::ManifestIndex::from_dirs(&[root.join("manifests/tools")]).unwrap();
        let registry_source =
            RegistrySource::new(tool_registry::Registry::open(pool.clone()), index);
        let sources: Vec<Box<dyn context_broker::ContextSource>> = vec![
            Box::new(rulepack),
            Box::new(registry_source),
            Box::new(ArtifactSource::new(pool.clone())),
        ];
        let broker = Broker::new(
            sources,
            context_broker::BudgetPolicy::default_for(8000).unwrap(),
            intent_config,
            ManifestStore::new(pool.clone()),
            None,
        );
        BrokerLink::new(broker).with_max_confirm_rounds(1)
    }

    /// 弃权确认：ambiguous 保持 ambiguous 且 warnings 留痕（不伪造 ok）。
    #[test]
    fn ambiguous_declined_confirmation_stays_ambiguous_with_warning() {
        let pool = migrated_pool();
        let link = broker_link(&pool);
        let outcome = link
            .assemble_confirmed(
                &AssembleRequest {
                    task_id: "t-broker-1".to_string(),
                    // 无族触发词、无槽位：置信度 0，ambiguous 无候选族。
                    text: "zzzq 无关词串".to_string(),
                    correlation_id: "c-broker-1".to_string(),
                    trace_id: None,
                    actor: "test".to_string(),
                },
                &mut DeclineConfirmer,
            )
            .unwrap();
        assert_eq!(outcome.context.intent.status, "ambiguous");
        assert!(outcome.confirmations.is_empty());
        assert!(
            outcome
                .context
                .warnings
                .iter()
                .any(|w| w.contains("confirmation declined")),
            "warnings: {:?}",
            outcome.context.warnings
        );
        assert!(!outcome.context.manifest_id.is_empty());
    }

    /// 弃权确认者（回调恒 None）。
    struct DeclineConfirmer;
    impl IntentConfirmer for DeclineConfirmer {
        fn confirm(&mut self, _alternatives: &[String], _goal: &str) -> Option<String> {
            None
        }
    }

    /// 固定确认词确认者（回调恒 Some(word)）。
    struct WordConfirmer(String);
    impl IntentConfirmer for WordConfirmer {
        fn confirm(&mut self, _alternatives: &[String], _goal: &str) -> Option<String> {
            Some(self.0.clone())
        }
    }

    /// 确认回路：给确认词后重装配（新 manifest），确认词留痕且意图族随之
    /// 变化；二次装配仍在阈值下时如实保持 ambiguous（不伪造 ok）。
    #[test]
    fn ambiguous_confirmation_round_records_word_and_reassembles() {
        let pool = migrated_pool();
        let link = broker_link(&pool);
        let outcome = link
            .assemble_confirmed(
                &AssembleRequest {
                    task_id: "t-broker-2".to_string(),
                    // 无族触发词：首轮 families 空。
                    text: "zzzq 无关词串".to_string(),
                    correlation_id: "c-broker-2".to_string(),
                    trace_id: None,
                    actor: "test".to_string(),
                },
                &mut WordConfirmer("http".to_string()),
            )
            .unwrap();
        assert_eq!(outcome.confirmations, vec!["http".to_string()]);
        // 确认词回填后重装配：Web / HTTP 族命中（families 由空变非空）。
        assert!(
            outcome
                .context
                .intent
                .families
                .iter()
                .any(|f| f == "Web / HTTP"),
            "families: {:?}",
            outcome.context.intent.families
        );
    }
}
