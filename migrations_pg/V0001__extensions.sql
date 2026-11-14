-- RIP3 情报核迁移链 V0001：扩展装载（ADR-0001；载体镜像
-- localhost/rip-pg17（containerfiles/rip-pg17/，pgvector/pg_search 预装于
-- paradedb 基座，pg_jieba 由镜像构建层安装））。
-- 本文件在 refinery 每文件事务内执行（12-STORAGE 第三节纪律）。
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_search;
CREATE EXTENSION IF NOT EXISTS pg_jieba;
