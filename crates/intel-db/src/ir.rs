//! Document IR v1 校验与提交侧结构（04-DOCUMENT-IR-SCHEMA 实施面，P2-1）。
//!
//! 双通道之一：Rust 侧以 `tool_registry::jsonschema_lite::validate_instance`
//! （jsonschema crate 0.61，draft 2020-12 全语义，Tool Contract 同校验器）
//! 对 IR 实例做 schema 校验；schema 单一来源 `schemas/document-ir/ir-v1.schema.json`
//! 经 include_str! 内嵌（research-engine contract.rs:43-98 先例）。容器内
//! jsonschema（Python）自检为另一通道（parser_tools.py self_check_schema）。
//!
//! schema 无法表达的约束在此兜底（04 号冻结物 fail-closed 细则）：parent 引用
//! 存在性、环检测、structure.root 一致性、全树连通。

use crate::{IntelError, IntelResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// IR v1 schema 文本（编译期内嵌；落盘单点 schemas/document-ir/ir-v1.schema.json）。
pub const IR_V1_SCHEMA: &str = include_str!("../../../schemas/document-ir/ir-v1.schema.json");

/// IR v1 顶层（九节，04 号冻结物第二节）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentIr {
    pub schema_version: String,
    pub document_metadata: DocumentMetadata,
    pub parser_metadata: ParserMetadata,
    pub structure: Structure,
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub references: Vec<Reference>,
    pub extraction_quality: ExtractionQuality,
    pub source_mapping: SourceMapping,
}

/// 解析元数据（词表与 PG document_blocks.extraction_method / IR parser 一致）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub document_id: String,
    pub title: String,
    pub document_type: String,
    pub original_filename: String,
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub content_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserMetadata {
    /// docling|pymupdf|tesseract|native_markdown|manual（04 号词表）。
    pub parser: String,
    pub parser_version: String,
    #[serde(default)]
    pub options: Value,
    #[serde(default)]
    pub pipeline: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Structure {
    pub root: StructureNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureNode {
    pub block_id: String,
    #[serde(default)]
    pub children: Vec<String>,
}

/// 结构块（04 号 $defs/block；父子树禁止压平）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub block_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// 14 值词表（03-DOMAIN-MODEL 1.3；单一词表来源 = 04 号文件）。
    pub block_type: String,
    pub order_index: i64,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Value>,
    /// docling|pymupdf|tesseract|native_markdown|manual。
    pub extraction_method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub asset_id: String,
    pub kind: String,
    pub content_sha256: String,
    pub storage_uri: String,
    pub source_block_id: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub ref_id: String,
    pub raw: String,
    #[serde(default)]
    pub resolved: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionQuality {
    #[serde(default)]
    pub self_check: Vec<QualityCheck>,
    pub overall: String,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityCheck {
    pub check: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMapping {
    #[serde(default)]
    pub block_to_source: Vec<BlockSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSource {
    pub block_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub char_offset: Option<Vec<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<Vec<f64>>,
}

/// 解析侧风险标记词表（R2 5.3 七类；提交服务据此判 REVIEW_PENDING 面）。
pub const QUARANTINE_FLAGS: [&str; 1] = ["POSSIBLE_PROMPT_INJECTION"];

/// 全量校验：schema 门（jsonschema_lite）+ 平台侧树门（环/引用/连通/root 一致）。
pub fn validate_ir(ir: &DocumentIr, raw: &Value) -> IntelResult<()> {
    let schema: Value = serde_json::from_str(IR_V1_SCHEMA)
        .map_err(|e| IntelError::Invalid(format!("ir schema unparsable: {e}")))?;
    tool_registry::jsonschema_lite::validate_instance(&schema, raw)
        .map_err(|e| IntelError::Invalid(format!("ir schema violation: {e}")))?;
    validate_tree(ir)
}

/// 平台侧树门（04 号冻结物：环由平台侧校验兜底）。
///
/// 规则：parent_id 引用存在且无环；structure.root.block_id 在 blocks 内；
/// 全部块自 root 连通（root.children 为顶层块集合，parent 链构成单树）。
pub fn validate_tree(ir: &DocumentIr) -> IntelResult<()> {
    use std::collections::{HashMap, HashSet};

    let mut ids: HashMap<&str, &Block> = HashMap::new();
    for b in &ir.blocks {
        if ids.insert(b.block_id.as_str(), b).is_some() {
            return Err(IntelError::Invalid(format!("duplicate block_id {}", b.block_id)));
        }
    }
    // parent 引用存在 + 环检测（沿父链走，重复到达即环）。
    for b in &ir.blocks {
        let mut seen = HashSet::new();
        let mut cur: Option<&str> = b.parent_id.as_deref();
        while let Some(pid) = cur {
            if !seen.insert(pid) {
                return Err(IntelError::Invalid(format!(
                    "parent cycle detected at block {}",
                    b.block_id
                )));
            }
            let parent = ids
                .get(pid)
                .ok_or_else(|| IntelError::Invalid(format!(
                    "block {} references missing parent {}",
                    b.block_id, pid
                )))?;
            cur = parent.parent_id.as_deref();
        }
    }
    // root 一致性：root 块存在、root 无父、structure.children 与「父为 root
    // 的块」一致（root 的直接子集；深层块经 parent 链挂在 children 子树上）。
    let root_id = ir.structure.root.block_id.as_str();
    let root = ids.get(root_id)
        .ok_or_else(|| IntelError::Invalid(format!("structure root {root_id} not in blocks")))?;
    if root.parent_id.is_some() {
        return Err(IntelError::Invalid("structure root must have no parent".to_string()));
    }
    let root_children: HashSet<&str> = ir
        .blocks
        .iter()
        .filter(|b| b.parent_id.as_deref() == Some(root_id))
        .map(|b| b.block_id.as_str())
        .collect();
    let declared: HashSet<&str> = ir.structure.root.children.iter().map(|s| s.as_str()).collect();
    let actual = root_children;
    if declared != actual {
        return Err(IntelError::Invalid(
            "structure.root.children must equal root children (parent_id == root)".to_string(),
        ));
    }
    // 连通性：每个块沿父链最终到达 root。
    for b in &ir.blocks {
        let mut cur = Some(b.block_id.as_str());
        let mut depth = 0usize;
        while let Some(cid) = cur {
            if cid == root_id {
                break;
            }
            cur = ids.get(cid).and_then(|blk| blk.parent_id.as_deref());
            depth += 1;
            if depth > ir.blocks.len() {
                return Err(IntelError::Invalid(format!(
                    "block {} unreachable from root (parent chain overflow)",
                    b.block_id
                )));
            }
        }
        if cur != Some(root_id) {
            return Err(IntelError::Invalid(format!(
                "block {} unreachable from structure root",
                b.block_id
            )));
        }
    }
    Ok(())
}

/// 提交侧 block_id 重映射（revision 域隔离）：IR 内 id 为内容派生（同内容
/// 新版本会产生同 id，与 document_blocks 全局 PK 冲突）；提交时以
/// `blk-sha256(revision_id|ir_id)` 前 20 位重写，parent/structure/mapping/
/// assets.source_block_id 全量同步映射（确定性：重放幂等）。
pub fn remap_block_ids(ir: &DocumentIr, revision_id: &str) -> IntelResult<DocumentIr> {
    use sha2::{Digest, Sha256};

    let map = |ir_id: &str| -> String {
        let mut h = Sha256::new();
        h.update(revision_id.as_bytes());
        h.update(b"|");
        h.update(ir_id.as_bytes());
        let d = format!("{:x}", h.finalize());
        format!("blk-{}", &d[..20])
    };
    let mut out = ir.clone();
    for b in &mut out.blocks {
        b.block_id = map(&b.block_id);
        if let Some(p) = b.parent_id.as_mut() {
            *p = map(p);
        }
    }
    out.structure.root.block_id = map(&out.structure.root.block_id);
    for c in &mut out.structure.root.children {
        *c = map(c);
    }
    for m in &mut out.source_mapping.block_to_source {
        m.block_id = map(&m.block_id);
    }
    for a in &mut out.assets {
        a.source_block_id = map(&a.source_block_id);
        // asset_id 同法 revision 域隔离（as- 前缀保持）。
        let mut h = Sha256::new();
        h.update(revision_id.as_bytes());
        h.update(b"|");
        h.update(a.asset_id.as_bytes());
        let d = format!("{:x}", h.finalize());
        a.asset_id = format!("as-{}", &d[..20]);
    }
    for r in &mut out.references {
        let mut h = Sha256::new();
        h.update(revision_id.as_bytes());
        h.update(b"|");
        h.update(r.ref_id.as_bytes());
        let d = format!("{:x}", h.finalize());
        r.ref_id = format!("ref-{}", &d[..16]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_ir() -> (DocumentIr, Value) {
        let raw = json!({
            "schema_version": "1",
            "document_metadata": {
                "document_id": "doc-pending", "title": "t", "document_type": "pdf",
                "original_filename": "a.pdf", "mime_type": "application/pdf",
                "content_sha256": format!("{:064}", 0)
            },
            "parser_metadata": {"parser": "docling", "parser_version": "2.150.1"},
            "structure": {"root": {"block_id": "blk-a", "children": ["blk-b"]}},
            "blocks": [
                {"block_id": "blk-a", "block_type": "document_title", "order_index": 0,
                 "content": "Title", "extraction_method": "docling"},
                {"block_id": "blk-b", "parent_id": "blk-a", "block_type": "heading",
                 "heading_level": 1, "order_index": 0, "content": "H1",
                 "extraction_method": "docling"},
                {"block_id": "blk-c", "parent_id": "blk-b", "block_type": "paragraph",
                 "order_index": 0, "content": "P", "extraction_method": "docling"}
            ],
            "assets": [], "references": [],
            "extraction_quality": {"self_check": [], "overall": "good", "flags": []},
            "source_mapping": {"block_to_source": [{"block_id": "blk-c", "page": 1}]}
        });
        let ir: DocumentIr = serde_json::from_value(raw.clone()).unwrap();
        (ir, raw)
    }

    #[test]
    fn valid_ir_passes_schema_and_tree() {
        let (ir, raw) = valid_ir();
        validate_ir(&ir, &raw).unwrap();
    }

    #[test]
    fn schema_rejects_unknown_field() {
        let (ir, mut raw) = valid_ir();
        raw["blocks"][0]["bogus"] = json!(1);
        assert!(validate_ir(&ir, &raw).is_err());
    }

    #[test]
    fn schema_rejects_unknown_block_type() {
        let (ir, mut raw) = valid_ir();
        raw["blocks"][2]["block_type"] = json!("banner");
        assert!(validate_ir(&ir, &raw).is_err());
    }

    #[test]
    fn schema_rejects_missing_content() {
        let (ir, mut raw) = valid_ir();
        raw["blocks"][2].as_object_mut().unwrap().remove("content");
        assert!(validate_ir(&ir, &raw).is_err());
    }

    #[test]
    fn schema_rejects_heading_without_level() {
        let (ir, mut raw) = valid_ir();
        raw["blocks"][1].as_object_mut().unwrap().remove("heading_level");
        assert!(validate_ir(&ir, &raw).is_err());
    }

    #[test]
    fn schema_rejects_table_without_rows() {
        let (ir, mut raw) = valid_ir();
        raw["blocks"][2]["block_type"] = json!("table");
        raw["blocks"][2]["table"] = json!({"caption": null});
        assert!(validate_ir(&ir, &raw).is_err());
    }

    #[test]
    fn schema_rejects_bad_sha256() {
        let (ir, mut raw) = valid_ir();
        raw["document_metadata"]["content_sha256"] = json!("zz");
        assert!(validate_ir(&ir, &raw).is_err());
    }

    #[test]
    fn schema_rejects_bad_version() {
        let (ir, mut raw) = valid_ir();
        raw["schema_version"] = json!("9");
        assert!(validate_ir(&ir, &raw).is_err());
    }

    #[test]
    fn tree_rejects_missing_parent() {
        let (mut ir, _raw) = valid_ir();
        ir.blocks[2].parent_id = Some("blk-missing".to_string());
        assert!(validate_tree(&ir).is_err());
    }

    #[test]
    fn tree_rejects_parent_cycle() {
        let (mut ir, _raw) = valid_ir();
        ir.blocks[0].parent_id = Some("blk-c".to_string()); // title -> paragraph -> heading -> title 环
        assert!(validate_tree(&ir).is_err());
    }

    #[test]
    fn tree_rejects_root_mismatch() {
        let (mut ir, _raw) = valid_ir();
        ir.structure.root.children = vec![]; // root 直接子集声明丢失
        assert!(validate_tree(&ir).is_err());
    }

    #[test]
    fn tree_rejects_unreachable_block() {
        let (mut ir, _raw) = valid_ir();
        ir.blocks[2].parent_id = None; // 悬空顶层（自 root 不可达）
        assert!(validate_tree(&ir).is_err());
    }

    #[test]
    fn remap_is_deterministic_and_consistent() {
        let (ir, _) = valid_ir();
        let a = remap_block_ids(&ir, "drev-x").unwrap();
        let b = remap_block_ids(&ir, "drev-x").unwrap();
        assert_eq!(a.blocks[0].block_id, b.blocks[0].block_id);
        let c = remap_block_ids(&ir, "drev-y").unwrap();
        assert_ne!(a.blocks[0].block_id, c.blocks[0].block_id);
        // 映射一致性：父引用同步重写（blk-c 的 parent 是 blk-b）。
        let new_b = &a.blocks[1].block_id;
        let c_block = a.blocks.iter().find(|x| x.block_type == "paragraph").unwrap();
        assert_eq!(c_block.parent_id.as_deref(), Some(new_b.as_str()));
        assert_eq!(c_block.block_id, a.source_mapping.block_to_source[0].block_id);
        // 重映射后树门仍过。
        validate_tree(&a).unwrap();
    }
}
