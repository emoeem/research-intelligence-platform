-- RIP3 情报核迁移链 V0007：知识供体署名面（RIP3 MA-3，MA-3-TASK.md §0 D5；
-- MULTI-AGENT-ACCESS-PLAN.md §四 MA-3 行 :47「PG V0007 knowledge 来源扩展」）。
--
-- knowledge_cards 纯追加两列 + 一索引，零既有列改动、零 CHECK 域改动：
-- - donor_caller_id TEXT（NULL = 既有路径产出——既有行为零回归；供体署名
--   入来源字段 = 既有 source_trust='agent_derived' + ai_derived=TRUE
--   （V0004:21-23 词表在位）之上补 donor 身份两列；027 SQLite 归属列
--   同款「可空 TEXT，既有行不回填」口径）。
-- - donor_channel TEXT CHECK (donor_channel IN ('mcp','cli'))：供体写入
--   通道词表（D4：外部 AI 供知识走 MCP 面 rip.contribute_knowledge →
--   spawn rp-kc donor-submit；本地 CLI 同一子命令）。词表两值冻结，MA-4+
--   扩词值须升 V0008+ 不回改。
-- - idx_cards_donor：供体行按 caller 对账索引。
--
-- 写入口唯一 = rp-kc donor-submit（create → 即转 CANDIDATE 两步 +
-- knowledge_review_log 留痕 reviewer=donor_caller；供体永不直写 APPROVED
-- ——approve 只能经既有 rp-kc review 双闸：硬标记阻断 + 质量地板）。
-- 去重治理 = 既有 D2 语义（相同 normalized_hash 产 SAME_CONTENT 候选边，
-- 不拒绝创建——合并走审核；禁加 UNIQUE(normalized_hash) 改 D2 语义）。
-- 并发治理 = decide FOR UPDATE 行锁串行化审核跃迁（card_repo 既有）。
--
-- 不加 UNIQUE 约束、不建 FK（donor_caller_id 是 SQLite 侧 caller 体系
-- 的跨库引用标注，PG 情报核无 caller 表——027 归属列口径同源，存在性由
-- 写入面 donor-submit 的 --donor-caller 必填守卫把关）。
--
-- 幂等：refinery 每文件事务 + checksum（intel-db lib.rs:131-140）；已应用
-- 版本跳过。只在开发库副本与测试库执行；生产 PG 实例不接本批写面。
ALTER TABLE knowledge_cards ADD COLUMN donor_caller_id TEXT;
ALTER TABLE knowledge_cards ADD COLUMN donor_channel TEXT CHECK (donor_channel IN ('mcp','cli'));
CREATE INDEX idx_cards_donor ON knowledge_cards(donor_caller_id);
