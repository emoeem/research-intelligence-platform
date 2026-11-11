# SK-1 摄取接线 ADR（指针文件）

> 状态：已裁决（全文唯一正本在 docs/adr/SK1-INGEST-ADR.md，按批次约束「ADR 落 docs/adr/」落位）  
> 日期：2026-11-12

P6-9 通用 Knowledge Catalog/Document 摄取接线的裁决正本：

- 路径：[docs/adr/SK1-INGEST-ADR.md](../adr/SK1-INGEST-ADR.md)
- 裁决范围：Domain 四实体（Source Snapshot / Catalog Entry / Document / ContentRef）与三个词表、SQLite 兼容性（新建 knowledge_source_entry 通用表，010 的 CHECK 与 15,155 行对账基线不动）、去重语义（canonical URL / 包内路径 + content_sha256）、Search 第三源与 Context PluginSource 准入闸（APPROVED + 插件可见）、Evidence 固化（context_citation_evidence）。

本文件只是指针，内容以 docs/adr/ 正本为准（避免双正本漂移）。
