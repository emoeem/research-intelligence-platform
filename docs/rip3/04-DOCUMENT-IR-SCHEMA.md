# RIP3 Document IR Schema v1（04-DOCUMENT-IR-SCHEMA，冻结物 2）

> 状态：Phase 0 冻结（2026-11-14）；本文给出 IR v1 的完整 Schema 设计文本（实施时
> 逐字落盘为 schemas/document-ir/ir-v1.schema.json 并编译期内嵌，沿 Tool Contract
> 的 include_str! 惯例，见 crates/research-engine/src/contract.rs:43-98 先例）。
> 规范模板：沿用 Tool Contract 与 P7-6 的 Schema 信封模式（R2 4.4 硬性）：
> $defs 内联自包含、fail-closed 校验、schema_version 显式、additionalProperties:false
> 全层禁宽松默认、禁用宽松默认值。

## 一、IR 在数据流中的位置

解析器（Docling/PyMuPDF/Tesseract/native）→ Document IR（JSON，本 Schema 校验）→
落库（document_blocks/asset 清单/source_mapping）→ Chunking → 索引。IR 是解析器与
平台之间的唯一契约：所有解析器统一输出 IR，不允许各自为政（R2 4.2 硬性）。
旧版本 IR 以 schema_version 路由到对应读取器，修改 JSON 结构不得导致旧解析结果
不可读（R2 4.4 迁移机制）。

## 二、IR v1 顶层结构（九节，R2 4.4 原文全覆盖）

~~~json
{
  "schema_version": "1",
  "document_metadata": {
    "document_id": "doc-...",            // 回填引用（解析时可为占位，入库时校验存在）
    "title": "...",                       // 非空
    "document_type": "book|pdf|docx|pptx|xlsx|html|markdown|code|image|dataset|other",
    "original_filename": "...",
    "mime_type": "...",                   // 魔法数检测值
    "language": "zh|en|...",              // 可空
    "content_sha256": "..."               // 原始文件哈希（CAS 键，必填）
  },
  "parser_metadata": {
    "parser": "docling|pymupdf|tesseract|native_markdown|manual",
    "parser_version": "2.x.y",
    "options": {},                        // 解析参数（可空对象，键不约束）
    "pipeline": ["format_detect", "parse", "ocr", "normalize"],
    "started_at / duration_ms / device": {...}
  },
  "structure": {                          // 章节树（节点引用 block_id）
    "root": {"block_id": "blk-...", "children": ["blk-...", "..."]}
  },
  "blocks": [ { "$ref": "#/$defs/block" } ],
  "assets": [ {                           // 图片/表格等提取物清单
    "asset_id": "as-...", "kind": "image|table|chart",
    "content_sha256": "...",              // 提取物本体进 CAS
    "storage_uri": "cas://...", "source_block_id": "blk-...",
    "mime_type": "..."
  } ],
  "references": [ { "ref_id": "ref-...", "raw": "...", "resolved": {} } ],
  "extraction_quality": {
    "self_check": [ { "check": "toc_present|page_coverage|empty_ratio|font_glyphs|table_integrity",
                       "passed": true, "detail": "..." } ],
    "overall": "good|degraded|poor",
    "flags": ["LOW_EXTRACTION_CONFIDENCE", "..."]   // 对齐 R2 5.3 风险标记词
  },
  "source_mapping": {                     // 双向映射（R2 4.4：到原始位置）
    "block_to_source": [ { "block_id": "blk-...", "page": 12,
                            "char_offset": [1024, 2048], "bbox": [x0,y0,x1,y1] } ],
    "source_to_block": 由读取器反向构建（不冗余存储）
  }
}
~~~

## 三、$defs/block（核心节点定义）

~~~json
{
  "block_id":   "blk-...（IR 内唯一，入库时映射为 PG document_blocks.block_id）",
  "parent_id":  "可空；构成树（父章节），禁止压平",
  "block_type": "document_title|heading|paragraph|list|table|code|image|figure|formula|quote|reference|page|chapter|section",
  "order_index": "整数，兄弟序",
  "heading_level": "1-6（block_type=heading 时必填，否则禁出现）",
  "content": "规范化文本（非空；table 用结构化 rows/cols 子对象替代纯文本）",
  "table":    "block_type=table 时必填：{rows: [[cell]], header: [cell], caption}",
  "code":     "block_type=code 时必填：{language, source}",
  "image_ref":"block_type=image|figure 时必填：asset_id 引用",
  "page":     "可空整数（书/文档页码）",
  "location": "{char_offset:[a,b], bbox:[x0,y0,x1,y1]}（可空；有则入 source_mapping）",
  "extraction_method": "docling|pymupdf|tesseract|native_markdown|manual",
  "extraction_confidence": "0-1 可空"
}
~~~

fail-closed 细则：conditional required（block_type 与子对象联动）用 if/then 表达；
未知 block_type 拒绝；order_index 重复拒绝；parent 环由平台侧树校验兜底（JSON Schema
无法表达环，落库时以 03 号文档 DocumentBlock 约束复核）。

## 四、版本迁移机制

1. schema_version 为字符串常量 "1"；读取器按版本路由（ir-v1 读取器唯一实现）。
2. 演进规则：新增可选字段 = 同版本小迭代（minOr 尾版本号，校验层容忍）；破坏性变更 =
   bump 主版本并新增读取器，旧 IR 文本永不改写。
3. IR 落库后不再依赖原 JSON 文件：blocks/assets/mapping 已关系化；JSON 文本进 CAS
   作为解析产物原始凭证（可重放）。

## 五、与既有契约面的对齐

- 信封校验器：复用 tool_registry::jsonschema_lite（draft 2020-12）+ Python 侧
  jsonschema 双通道（worker 产 IR 时先自检），与 Tool Contract 同口径。
- 与 Document IR 对应的 Python worker 输出协议：解析 worker（rip-parser 容器族）按
  D-TC-03 帧协议返回 {status:"result", data:{ir: {...}}}，ir 即本文档 Schema 实例。
- block 词表与 03-DOMAIN-MODEL 1.3 的 block_type 14 值严格一致（单一词表来源 = 本文件）。

## 六、验收方式（Phase 2 实施时逐条）

1. Schema 门：schemas/document-ir/ir-v1.schema.json + 双通道校验测试（合法 IR 过、
   八类负例拒：未知字段/未知 block_type/缺失 content/heading 级缺失/table 缺 rows/
   坏 sha256 形/环由平台侧拒/坏版本）。
2. roundtrip：MD/PDF/DOCX 样本解析产 IR → 落库 → 读回 blocks 树与 source_mapping
   与原文对账（页码/偏移）。
3. 旧版本可读：构造 v1 IR，升级校验层后 v1 读取器仍通过（版本路由测试）。
