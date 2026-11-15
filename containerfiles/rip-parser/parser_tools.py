#!/usr/bin/env python3
"""rip.parser.* 解析容器共用入口（RIP3 P2-1）。

帧协议（TOOL-CONTRACT D-TC-03，与 core_tools.py/src_tools.py 同款）：stdin 一行
JSON 请求 {"run_id": ..., "input": {...}, "budget": {...}}；stdout 一行 JSON 结果
{"status": "result", "data": {...}}；错误帧 {"status": "error", "error": {...}}
（stdout 与 stderr 双路镜像）。op 取 argv[1]。

op 分派：
- detect  魔法数格式判定 + sha256 + ZIP 成员清单与安全判定（13-SECURITY 第二节：
          条目数/尺寸/压缩比/路径穿越阈值），OCR 栈零加载（快速探针可用）。
- parse   按格式路由解析引擎，输出 Document IR v1（schemas/document-ir/
          ir-v1.schema.json 实例；发帧前容器内 jsonschema 自检，双通道之一）：
          pdf/docx/pptx/xlsx/html/image -> Docling（模型镜像内烘焙，运行期零网络）；
          epub -> PyMuPDF（get_toc 章节树 + 分页文本 + 图片提取）；
          markdown/txt/code -> native_markdown（std-only 行态机）；
          zip -> 拒绝（-32602：zip 走 detect 成员清单，平台侧逐成员 parse）。
- ocr     Tesseract 兜底 OCR（R2 4.3：Docling 先行，Tesseract 兜底；TSV 输出
          带 bbox 与置信度，extraction_method=tesseract）。

安全边界：读限 /task/in 之下（realtime 归一化前缀校验）；写限 /task/out 之下；
原始文件帽 512MiB；SIGTERM 协作取消（D-TC-04）；network.mode=none（manifest
强制；模型与 tessdata 全部构建期烘焙）。解析产物落 /task/out/ir.json 与
/task/out/assets/（Artifact 收集载体）。

IR 产出纪律：block_id 确定性生成（sha256 输入指纹 + 序），同输入重解析同 IR
（提交服务幂等重放的依据）；页眉/页脚/页码去噪（D2 规范化面，丢弃计数进
self_check detail）；extraction_quality 五检项（04 号冻结物）+ LOW_EXTRACTION_
CONFIDENCE / POSSIBLE_PROMPT_INJECTION 风险标记（R2 5.3 / 13 号冻结物第三节）。
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import time
import zipfile
from datetime import datetime, timezone
from pathlib import Path

IN_ROOT = "/task/in"
OUT_ROOT = "/task/out"
SCHEMA_PATH = "/app/schemas/ir-v1.schema.json"
MAX_INPUT_BYTES = 512 * 1024 * 1024  # 13-SECURITY 第二节：原始文件帽 512MiB
ZIP_MAX_ENTRIES = 10000
ZIP_MAX_MEMBER = 512 * 1024 * 1024
ZIP_MAX_TOTAL = 2 * 1024 * 1024 * 1024
ZIP_MAX_RATIO = 100
ASSET_CAP = 64  # 单文档图片提取上限（超限记 self_check，不静默丢）

DOC_TYPE_BY_FORMAT = {
    "pdf": "pdf",
    "docx": "docx",
    "pptx": "pptx",
    "xlsx": "xlsx",
    "html": "html",
    "epub": "book",
    "markdown": "markdown",
    "txt": "other",
    "code": "code",
    "image": "image",
    "zip": "other",
    "json": "dataset",
    "other": "other",
}

CODE_SUFFIXES = {
    ".py": "python", ".rs": "rust", ".js": "javascript", ".ts": "typescript",
    ".go": "go", ".c": "c", ".h": "c", ".cpp": "cpp", ".java": "java",
    ".sh": "shell", ".rb": "ruby", ".php": "php", ".sql": "sql",
    ".toml": "toml", ".yaml": "yaml", ".yml": "yaml",
}

# POSSIBLE_PROMPT_INJECTION 形态检测（13 号冻结物第三节：指令式祈使句针对
# 模型/工具的形态）。高信号单命中即标记；低信号累计 >=3 命中标记。
INJECTION_HIGH = (
    r"ignore\s+(all\s+|any\s+)?(previous|prior|above|earlier)\s+instructions?",
    r"disregard\s+(all\s+)?(previous|prior|above)\s+instructions?",
    r"忽略(之前|以上|上述|前面)(的)?(所有|全部)?指令",
    r"无视(之前|以上|上述)(的)?(所有|全部)?指令",
    r"(print|reveal|show)\s+(me\s+)?(your\s+)?(system\s+)?(prompt|instructions)",
    r"</?\s*(system|assistant|tool)\s*>",
)
INJECTION_LOW = (
    r"you\s+are\s+now\s+(a|an|the)\s+",
    r"act\s+as\s+(a|an|the)\s+",
    r"(system|developer)\s+(prompt|message|role)\s*[:：]",
    r"请(忽略|无视)",
    r"assistant\s*[:：]",
    r"你现在是(一|一个|一名)",
)

# 13-SECURITY 第二节：ZIP 路径穿越（绝对路径与 .. 拒绝）+ 嵌套包深度 <= 2。
ARCHIVE_SUFFIXES = (".zip", ".tar", ".gz", ".tgz", ".bz2", ".xz", ".7z", ".rar")


def _on_term(signum, _frame):
    """SIGTERM 处理器（D-TC-04 协作式取消的容器内侧，与 core_tools.py 同款）。"""
    sys.exit(128 + signum)


def _emit(payload):
    print(json.dumps(payload, ensure_ascii=False))
    sys.stdout.flush()


def _emit_error(code, message, exit_code=1):
    frame = {"status": "error", "error": {"code": code, "message": message}}
    print(json.dumps(frame, ensure_ascii=False))
    print(json.dumps(frame, ensure_ascii=False), file=sys.stderr)
    sys.stderr.flush()
    return exit_code


def _in_path(raw):
    """归一化并校验输入路径：必须在 /task/in 之下、存在、且不超 512MiB 帽。"""
    if not isinstance(raw, str) or not raw:
        raise ValueError("path is required")
    p = Path(raw)
    if not p.is_absolute():
        p = Path(IN_ROOT) / p
    real = os.path.realpath(str(p))
    root_real = os.path.realpath(IN_ROOT)
    if not (real == root_real or real.startswith(root_real + os.sep)):
        raise ValueError("path escapes /task/in")
    fp = Path(real)
    if not fp.is_file():
        raise ValueError(f"file not found: {real}")
    if fp.stat().st_size > MAX_INPUT_BYTES:
        raise ValueError(f"file exceeds 512MiB cap: {fp.stat().st_size} bytes")
    return fp


def _sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


_MAGIC = (
    (b"%PDF-", "pdf"),
    (b"\x89PNG", "image"),
    (b"\xff\xd8\xff", "image"),
    (b"GIF8", "image"),
    (b"RIFF", "image"),
    (b"\x1f\x8b", "archive-other"),
    (b"PK\x03\x04", "zip-family"),
    (b"{\xf7\xca\x20", "unknown"),  # 7z
)


def _sniff_bytes(head):
    for magic, kind in _MAGIC:
        if head.startswith(magic):
            return kind
    if b"\x00" in head[:512]:
        return "binary-unknown"
    return "text"


def _zip_kind(path):
    """zip 家族细分：epub / OOXML 三种 / 普通压缩包。"""
    try:
        with zipfile.ZipFile(path) as z:
            names = set(z.namelist())
            if "mimetype" in names:
                try:
                    if z.read("mimetype", b"")[:60] == b"application/epub+zip":
                        return "epub"
                except Exception:
                    pass
            if "[Content_Types].xml" in names:
                ct = z.read("[Content_Types].xml")[:65536]
                if b"word/" in ct:
                    return "docx"
                if b"ppt/" in ct:
                    return "pptx"
                if b"xl/" in ct:
                    return "xlsx"
            return "zip"
    except zipfile.BadZipFile:
        return "unknown"


def _suffix_kind(path):
    sfx = path.suffix.lower()
    if sfx in (".md", ".markdown", ".mkd"):
        return "markdown"
    if sfx == ".txt" or sfx == ".log":
        return "txt"
    if sfx in CODE_SUFFIXES:
        return "code"
    if sfx in (".html", ".htm", ".xhtml"):
        return "html"
    if sfx == ".pdf":
        return "pdf"
    if sfx in (".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".tif", ".tiff"):
        return "image"
    if sfx == ".epub":
        return "epub"
    if sfx == ".json":
        return "json"
    if sfx == ".zip":
        return "zip"
    if sfx in (".docx", ".pptx", ".xlsx"):
        return sfx
    return None


def detect_format(path):
    """魔法数 + 扩展名双证据判定（13-SECURITY：不信扩展名；扩展名只做参考）。"""
    size = path.stat().st_size
    with open(path, "rb") as f:
        head = f.read(4096)
    sniffed = _sniff_bytes(head)
    sfx = _suffix_kind(path)
    fmt = None
    if sniffed == "pdf":
        fmt = "pdf"
    elif sniffed == "image":
        fmt = "image"
    elif sniffed == "zip-family":
        fmt = _zip_kind(path)
    elif sniffed == "text":
        if sfx in ("markdown", "txt", "code", "html", "json"):
            fmt = sfx
        elif sfx is None and head.lstrip().startswith(("{", "[")):
            fmt = "json"
        else:
            fmt = "txt" if sfx is None else sfx
    else:
        fmt = sfx if sfx else "unknown"
    mime = _mime_for(fmt, path)
    return fmt, mime, size


def _mime_for(fmt, path):
    table = {
        "pdf": "application/pdf",
        "docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "pptx": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "epub": "application/epub+zip",
        "zip": "application/zip",
        "html": "text/html",
        "markdown": "text/markdown",
        "txt": "text/plain",
        "code": "text/plain",
        "json": "application/json",
    }
    if fmt == "image":
        with open(path, "rb") as f:
            head = f.read(16)
        if head.startswith(b"\x89PNG"):
            return "image/png"
        if head.startswith(b"\xff\xd8\xff"):
            return "image/jpeg"
        if head.startswith(b"GIF8"):
            return "image/gif"
        if head[8:12] == b"WEBP":
            return "image/webp"
        return "image/octet-stream"
    return table.get(fmt, "application/octet-stream")


def zip_inventory(path):
    """ZIP 成员清单 + 安全判定（13-SECURITY 第二节阈值；不做任何解压）。"""
    with zipfile.ZipFile(path) as z:
        infos = z.infolist()
        members = []
        reasons = []
        total = 0
        for info in infos:
            if info.is_dir():
                continue
            compressed = info.compress_size or info.file_size or 1
            ratio = info.file_size / compressed
            traversal = info.filename.startswith("/") or ".." in Path(info.filename).parts
            nested = info.filename.lower().endswith(ARCHIVE_SUFFIXES)
            member = {
                "name": info.filename,
                "size_bytes": info.file_size,
                "compressed_bytes": info.compress_size,
                "nested_archive": nested,
                "traversal": traversal,
                "ratio": round(ratio, 2),
            }
            if traversal:
                member["verdict"] = "rejected"
                reasons.append(f"traversal: {info.filename}")
            elif info.file_size > ZIP_MAX_MEMBER:
                member["verdict"] = "rejected"
                reasons.append(f"member too large: {info.filename}")
            elif ratio > ZIP_MAX_RATIO:
                member["verdict"] = "rejected"
                reasons.append(f"bomb ratio {member['ratio']}: {info.filename}")
            else:
                member["verdict"] = "ok"
            members.append(member)
            total += info.file_size
        if len(infos) > ZIP_MAX_ENTRIES:
            reasons.append(f"entry count {len(infos)} > {ZIP_MAX_ENTRIES}")
        if total > ZIP_MAX_TOTAL:
            reasons.append(f"total {total} > {ZIP_MAX_TOTAL}")
        return {
            "format": "zip",
            "entry_count": len(infos),
            "total_uncompressed": total,
            "members": members,
            "zip_verdict": {"safe": not reasons, "reasons": reasons},
        }


def decode_text(path):
    data = path.read_bytes()
    for enc in ("utf-8", "utf-16"):
        try:
            return data.decode(enc), enc
        except UnicodeDecodeError:
            continue
    return data.decode("latin-1"), "latin-1"


def detect_language(text):
    sample = text[:65536]
    if not sample.strip():
        return None
    cjk = len(re.findall(r"[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]", sample))
    letters = len(re.findall(r"[A-Za-z]", sample))
    if letters == 0 and cjk == 0:
        return None
    return "zh" if cjk > letters else "en"


def scan_injection(text):
    joined = text if len(text) < 4 * 1024 * 1024 else text[: 4 * 1024 * 1024]
    high = sum(1 for p in INJECTION_HIGH if re.search(p, joined, re.IGNORECASE))
    low = sum(1 for p in INJECTION_LOW if re.search(p, joined, re.IGNORECASE))
    return high, low


def _block_id(seed, seq):
    h = hashlib.sha256(f"{seed}|{seq}".encode("utf-8")).hexdigest()[:20]
    return f"blk-{h}"


def _ref_id(seed, seq):
    h = hashlib.sha256(f"ref|{seed}|{seq}".encode("utf-8")).hexdigest()[:16]
    return f"ref-{h}"


def _asset_id(seed, seq):
    h = hashlib.sha256(f"as|{seed}|{seq}".encode("utf-8")).hexdigest()[:16]
    return f"as-{h}"


class BlockBuilder:
    """按标题层级栈维护父子关系（禁止压平），block_id/order_index 确定性生成。"""

    def __init__(self, seed, extraction_method, title_hint=None):
        self.seed = seed
        self.method = extraction_method
        self.title_hint = title_hint
        self.seq = 0
        self.blocks = []
        self.stack = []  # [(level, block_id)]；title 为 level 0 沉底

    def _add(self, **fields):
        btype = fields["block_type"]
        seq = self.seq
        self.seq += 1
        bid = _block_id(
            f"{self.seed}|{btype}|{fields.get('page')}|{fields.get('content', '')[:512]}", seq
        )
        if btype == "document_title":
            parent = None
            self.stack = [(0, bid)]
        elif btype == "heading":
            level = fields.get("heading_level", 6)
            while self.stack and self.stack[-1][0] >= level:
                self.stack.pop()
            parent = self.stack[-1][1] if self.stack else None
            self.stack.append((level, bid))
        else:
            parent = self.stack[-1][1] if self.stack else None
        block = {
            "block_id": bid,
            "parent_id": parent,
            "block_type": btype,
            "order_index": 0,
            "content": fields["content"],
            "extraction_method": self.method,
        }
        if btype == "heading":
            block["heading_level"] = fields.get("heading_level", 6)
        for key in ("table", "code", "image_ref", "page", "location", "extraction_confidence"):
            if fields.get(key) is not None:
                block[key] = fields[key]
        self.blocks.append(block)
        return bid

    def finish(self):
        """补兄弟序 + 章节树根收敛 + 质量检项的原始计数。

        根收敛纪律（单树，平台侧连通性校验 fail-closed）：document_title
        在场时全部顶层块挂其下；无标题且顶层分量 > 1 时造 synthetic
        document_title（content = title_hint，即文件名）作根；单一顶层分量
        时以该块为根，零重挂。
        """
        top_level = [b for b in self.blocks if b["parent_id"] is None]
        title = next((b for b in self.blocks if b["block_type"] == "document_title"), None)
        if title is not None:
            for b in top_level:
                if b is not title:
                    b["parent_id"] = title["block_id"]
            root = title
        elif len(top_level) > 1:
            seq = self.seq
            self.seq += 1
            bid = _block_id(f"{self.seed}|synthetic-title", seq)
            self.blocks.insert(0, {
                "block_id": bid,
                "parent_id": None,
                "block_type": "document_title",
                "order_index": 0,
                "content": self.title_hint or "(untitled)",
                "extraction_method": self.method,
            })
            for b in top_level:
                b["parent_id"] = bid
            root = self.blocks[0]
        else:
            root = None
            for b in self.blocks:
                if b["block_type"] == "heading":
                    root = b
                    break
            if root is None and top_level:
                root = top_level[0]
        counters = {}
        for b in self.blocks:
            key = b["parent_id"]
            b["order_index"] = counters.get(key, 0)
            counters[key] = counters.get(key, 0) + 1
        children = [b["block_id"] for b in self.blocks if root and b["parent_id"] == root["block_id"]]
        structure = {"root": {"block_id": root["block_id"] if root else "blk-none", "children": children}}
        return self.blocks, structure


def quality_checks(blocks, headings, pages_with, total_pages, table_count, fffd_count,
                   skipped_noise, injection, mean_conf):
    checks = []
    checks.append({
        "check": "toc_present",
        "passed": headings >= 2,
        "detail": f"heading/section blocks={headings}",
    })
    if total_pages:
        cov = pages_with / total_pages
        checks.append({
            "check": "page_coverage",
            "passed": cov >= 0.9,
            "detail": f"pages_with_blocks={pages_with}/{total_pages} cov={cov:.2f}",
        })
    else:
        checks.append({
            "check": "page_coverage",
            "passed": True,
            "detail": "non-paginated format",
        })
    total = len(blocks)
    empties = sum(1 for b in blocks if not b.get("content", "").strip())
    ratio = empties / total if total else 0.0
    checks.append({
        "check": "empty_ratio",
        "passed": ratio <= 0.3,
        "detail": f"empty={empties}/{total} ratio={ratio:.2f} noise_skipped={skipped_noise}",
    })
    checks.append({
        "check": "font_glyphs",
        "passed": fffd_count == 0,
        "detail": f"replacement_chars={fffd_count}",
    })
    checks.append({
        "check": "table_integrity",
        "passed": table_count == 0 or all(
            b.get("table", {}).get("rows") for b in blocks if b["block_type"] == "table"
        ),
        "detail": f"tables={table_count}",
    })
    failed = sum(1 for c in checks if not c["passed"])
    overall = "poor" if failed >= 2 else ("degraded" if failed == 1 else "good")
    flags = []
    if overall == "poor" or (mean_conf is not None and mean_conf < 0.6):
        flags.append("LOW_EXTRACTION_CONFIDENCE")
    high, low = injection
    if high >= 1 or low >= 3:
        flags.append("POSSIBLE_PROMPT_INJECTION")
    return {
        "self_check": checks,
        "overall": overall,
        "flags": flags,
    }, failed


def build_ir(fmt, path, mime, content_sha256, parser_id, parser_version, pipeline,
             blocks, structure, assets, references, started, duration_ms, quality):
    title = None
    for b in blocks:
        if b["block_type"] == "document_title":
            title = b["content"]
            break
    if not title:
        title = path.name
    text_all = "\n".join(b["content"] for b in blocks)
    ir = {
        "schema_version": "1",
        "document_metadata": {
            "document_id": "doc-pending",
            "title": title,
            "document_type": DOC_TYPE_BY_FORMAT.get(fmt, "other"),
            "original_filename": path.name,
            "mime_type": mime,
            "language": detect_language(text_all),
            "content_sha256": content_sha256,
        },
        "parser_metadata": {
            "parser": parser_id,
            "parser_version": parser_version,
            "options": {},
            "pipeline": pipeline,
            "started_at": started,
            "duration_ms": duration_ms,
            "device": "cpu",
        },
        "structure": structure,
        "blocks": blocks,
        "assets": assets,
        "references": references,
        "extraction_quality": quality,
        "source_mapping": {
            "block_to_source": [
                {
                    "block_id": b["block_id"],
                    **({"page": b["page"]} if b.get("page") else {}),
                    **({"char_offset": b["location"]["char_offset"]}
                       if (b.get("location") or {}).get("char_offset") else {}),
                    **({"bbox": b["location"]["bbox"]}
                       if (b.get("location") or {}).get("bbox") else {}),
                }
                for b in blocks
            ]
        },
    }
    return ir


def self_check_schema(ir):
    """容器内 jsonschema 自检（双通道之一；Rust 侧 validate_instance 为另一通道）。"""
    import jsonschema

    schema = json.loads(Path(SCHEMA_PATH).read_text(encoding="utf-8"))
    jsonschema.validate(ir, schema)


def _docling_convert(path, ocr=True):
    from docling.datamodel.base_models import InputFormat
    from docling.datamodel.pipeline_options import PdfPipelineOptions
    from docling.document_converter import DocumentConverter, PdfFormatOption

    artifacts = os.environ.get("DOCLING_ARTIFACTS_PATH", "/opt/docling-artifacts")
    pdf_opts = PdfPipelineOptions()
    pdf_opts.artifacts_path = Path(artifacts)
    pdf_opts.do_ocr = ocr
    pdf_opts.do_table_structure = True
    # standard 安装面 OCR 后端 = rapidocr（easyocr 不在 [standard] 依赖内）；
    # 显式钉定，缺类时回落引擎默认。
    try:
        from docling.datamodel.pipeline_options import RapidOcrOptions

        pdf_opts.ocr_options = RapidOcrOptions()
    except ImportError:
        pass
    converter = DocumentConverter(
        format_options={InputFormat.PDF: PdfFormatOption(pipeline_options=pdf_opts)}
    )
    return converter.convert(str(path), raises_on_error=True)


def _label_map():
    from docling_core.types.doc.labels import DocItemLabel

    def _opt(name):
        return getattr(DocItemLabel, name, None)

    mapping = {
        DocItemLabel.TITLE: "document_title",
        DocItemLabel.SECTION_HEADER: "heading",
        DocItemLabel.TEXT: "paragraph",
        DocItemLabel.PARAGRAPH: "paragraph",
        DocItemLabel.LIST_ITEM: "list",
        DocItemLabel.TABLE: "table",
        DocItemLabel.PICTURE: "figure",
        DocItemLabel.CODE: "code",
        DocItemLabel.FORMULA: "formula",
        DocItemLabel.FOOTNOTE: "reference",
    }
    for name in ("QUOTE", "CAPTION"):
        lab = _opt(name)
        if lab is not None:
            mapping[lab] = "quote"
    return mapping


def _noise_labels():
    from docling_core.types.doc.labels import DocItemLabel

    labs = set()
    for name in ("PAGE_HEADER", "PAGE_FOOTER", "PAGE_NUMBER", "EMPTY_PAGE",
                 "KEY_VALUE_REGION", "CHECKBOX_SELECTED", "CHECKBOX_UNSELECTED"):
        lab = getattr(DocItemLabel, name, None)
        if lab is not None:
            labs.add(lab)
    return labs


def _prov_fields(item):
    """ProvenanceItem -> page/bbox/char_offset（缺省如实缺省）。"""
    prov = getattr(item, "prov", None)
    if not prov:
        return {}
    first = prov[0]
    out = {}
    page_no = getattr(first, "page_no", None)
    if page_no:
        out["page"] = int(page_no)
    bbox = getattr(first, "bbox", None)
    if bbox is not None:
        try:
            out["location"] = {"bbox": [
                float(bbox.l), float(bbox.t), float(bbox.r), float(bbox.b)
            ]}
        except Exception:
            pass
    charson = getattr(first, "charson", None)
    if charson:
        try:
            out["location"] = {
                **(out.get("location") or {}),
                "char_offset": [int(charson[0]), int(charson[1])],
            }
        except Exception:
            pass
    return out


def _table_fields(item):
    rows = None
    header = None
    try:
        df = item.export_to_dataframe()
        rows = [[("" if v is None or (isinstance(v, float) and v != v) else str(v)) for v in r]
                for r in df.values.tolist()]
        header = [str(c) for c in df.columns.tolist()]
    except Exception:
        try:
            html = item.export_to_html()
            rows, header = _rows_from_html(html)
        except Exception:
            rows = None
    if not rows:
        rows = [[""]]
    return {"rows": rows, "header": header, "caption": None}


def _rows_from_html(html):
    rows = []
    cur = None
    cell = None
    header = []
    in_cell = False
    from html.parser import HTMLParser

    class _P(HTMLParser):
        def handle_starttag(self, tag, attrs):
            nonlocal cur, cell, in_cell, header
            if tag == "tr":
                cur = []
            elif tag in ("td", "th") and cur is not None:
                cell = []
                in_cell = True
                if tag == "th" and not header:
                    pass

        def handle_endtag(self, tag):
            nonlocal cur, cell, in_cell
            if tag in ("td", "th") and cell is not None:
                cur.append("".join(cell).strip())
                cell = None
                in_cell = False
            elif tag == "tr" and cur is not None:
                if cur:
                    rows.append(cur)
                cur = None

        def handle_data(self, data):
            if cell is not None:
                cell.append(data)

    _P().feed(html)
    return rows or [[""]], header or None


def _save_picture(doc, item, assets_dir, seed, seq):
    """三层兜底取图：save_picture / item.image / 放弃（如实记 self_check）。"""
    before = set(p.name for p in assets_dir.iterdir()) if assets_dir.exists() else set()
    try:
        out = doc.save_picture(item, image_dir=assets_dir, scale=2, resolution=150)
        if out:
            return Path(str(out))
    except Exception:
        pass
    after = set(p.name for p in assets_dir.iterdir()) if assets_dir.exists() else set()
    new = after - before
    if new:
        return assets_dir / sorted(new)[0]
    img = getattr(item, "image", None)
    if img is not None:
        p = assets_dir / f"pic-{seq}.png"
        img.save(str(p))
        return p
    return None


def parse_docling(path, fmt, seed):
    started = datetime.now(timezone.utc).isoformat()
    t0 = time.monotonic()
    result = _docling_convert(path, ocr=(fmt == "image"))
    doc = result.document
    version = _docling_version()
    assets_dir = Path(OUT_ROOT) / "assets"
    assets_dir.mkdir(parents=True, exist_ok=True)

    mapping = _label_map()
    noise = _noise_labels()
    builder = BlockBuilder(seed, "docling", title_hint=path.name)
    assets = []
    references = []
    asset_seq = 0
    ref_seq = 0
    skipped_noise = 0
    pages_with = set()
    fffd_count = 0
    table_count = 0
    confidences = []

    for item, _level in doc.iterate_items():
        label = getattr(item, "label", None)
        if label in noise:
            skipped_noise += 1
            continue
        btype = mapping.get(label)
        if btype is None:
            skipped_noise += 1
            continue
        prov = _prov_fields(item)
        if prov.get("page"):
            pages_with.add(prov["page"])
        if btype == "table":
            table_count += 1
            table = _table_fields(item)
            bid = builder._add(
                block_type="table",
                content="table",
                table=table,
                extraction_confidence=None,
                **prov,
            )
            continue
        if btype == "figure":
            if asset_seq >= ASSET_CAP:
                skipped_noise += 1
                continue
            img_path = _save_picture(doc, item, assets_dir, seed, asset_seq)
            if img_path is None:
                skipped_noise += 1
                continue
            aid = _asset_id(seed, asset_seq)
            asset_seq += 1
            ash = _sha256_file(img_path)
            assets.append({
                "asset_id": aid,
                "kind": "image",
                "content_sha256": ash,
                "storage_uri": f"cas://{ash}",
                "source_block_id": None,  # 回填
                "mime_type": _mime_for("image", img_path),
            })
            caption = ""
            bid = builder._add(
                block_type="figure",
                content=f"figure:{img_path.name}" if not caption else caption,
                image_ref=aid,
                **prov,
            )
            assets[-1]["source_block_id"] = bid
            continue
        text = getattr(item, "text", None)
        if text is None or not str(text).strip():
            skipped_noise += 1
            continue
        text = str(text)
        fffd_count += text.count("\ufffd")
        conf = getattr(item, "confidence", None)
        if isinstance(conf, (int, float)) and 0 <= conf <= 1:
            confidences.append(float(conf))
        fields = {
            "block_type": btype,
            "content": text,
            **prov,
        }
        if btype == "heading":
            lvl = getattr(item, "level", None)
            try:
                lvl = int(lvl)
            except (TypeError, ValueError):
                lvl = 2
            fields["heading_level"] = max(1, min(6, lvl))
        if btype == "code":
            fields["code"] = {
                "language": getattr(item, "code_language", None) or None,
                "source": text,
            }
        if btype == "reference":
            ref_seq += 1
            references.append({
                "ref_id": _ref_id(seed, ref_seq),
                "raw": text,
                "resolved": {},
            })
        builder._add(**fields)

    blocks, structure = builder.finish()
    duration = int((time.monotonic() - t0) * 1000)
    headings = sum(1 for b in blocks if b["block_type"] in ("heading", "document_title", "chapter", "section"))
    total_pages = len(getattr(result, "pages", []) or []) or None
    injection = scan_injection(text_all_of(blocks))
    mean_conf = (sum(confidences) / len(confidences)) if confidences else None
    quality, _failed = quality_checks(
        blocks, headings, len(pages_with), total_pages, table_count,
        fffd_count, skipped_noise, injection, mean_conf,
    )
    pipeline = ["format_detect", "parse"]
    if fmt == "image":
        pipeline.append("ocr")
    pipeline.append("normalize")
    ir = build_ir(fmt, path, _mime_for(fmt, path), seed,
                  "docling", version, pipeline, blocks, structure, assets,
                  references, started, duration, quality)
    return ir


def text_all_of(blocks):
    return "\n".join(b["content"] for b in blocks)


def _docling_version():
    try:
        from importlib.metadata import version

        return version("docling")
    except Exception:
        return "unknown"


def _pymupdf_version():
    import pymupdf

    v = getattr(pymupdf, "__version__", None)
    if v:
        return str(v)
    vt = getattr(pymupdf, "VersionBind", None) or getattr(pymupdf, "version", None)
    if isinstance(vt, (tuple, list)):
        return str(vt[0])
    return str(vt) if vt else "unknown"


def parse_epub(path, seed):
    import pymupdf

    started = datetime.now(timezone.utc).isoformat()
    t0 = time.monotonic()
    version = _pymupdf_version()
    doc = pymupdf.open(str(path))
    builder = BlockBuilder(seed, "pymupdf", title_hint=path.name)
    assets_dir = Path(OUT_ROOT) / "assets"
    assets_dir.mkdir(parents=True, exist_ok=True)
    assets = []
    asset_seq = 0
    pages_with = set()
    fffd_count = 0
    confidences = []

    toc = doc.get_toc(simple=True)  # [[level, title, page], ...]
    toc_titles = set()
    for level, title, page in toc:
        try:
            level = max(1, min(6, int(level)))
        except (TypeError, ValueError):
            level = 2
        toc_titles.add(title.strip())
        builder._add(
            block_type="heading", heading_level=level, content=title,
            page=int(page) if page else None,
        )

    for pno, page in enumerate(doc, start=1):
        text = page.get_text("text")
        if not text.strip():
            continue
        pages_with.add(pno)
        fffd_count += text.count("\ufffd")
        for para in re.split(r"\n\s*\n", text):
            # 页面文本会重排包含标题行（与 toc 重复）；与 toc 标题完全一致的
            # 行去重，避免同一标题双块。
            lines = [ln.strip() for ln in para.splitlines() if ln.strip()]
            lines = [ln for ln in lines if ln not in toc_titles]
            para = "\n".join(lines).strip()
            if not para:
                continue
            builder._add(block_type="paragraph", content=para, page=pno)
        if asset_seq < ASSET_CAP:
            for img in page.get_images(full=True):
                xref = img[0]
                try:
                    pix = pymupdf.Pixmap(doc, xref)
                    if pix.n - pix.alpha > 3:
                        pix = pymupdf.Pixmap(pymupdf.csRGB, pix)
                    p = assets_dir / f"epub-{pno}-{xref}.png"
                    pix.save(str(p))
                    aid = _asset_id(seed, asset_seq)
                    asset_seq += 1
                    bid = builder._add(
                        block_type="image",
                        content=f"image:{p.name}",
                        image_ref=aid,
                        page=pno,
                    )
                    assets.append({
                        "asset_id": aid,
                        "kind": "image",
                        "content_sha256": _sha256_file(p),
                        "storage_uri": f"cas://{_sha256_file(p)}",
                        "source_block_id": bid,
                        "mime_type": "image/png",
                    })
                except Exception:
                    continue

    blocks, structure = builder.finish()
    duration = int((time.monotonic() - t0) * 1000)
    headings = sum(1 for b in blocks if b["block_type"] == "heading")
    total_pages = doc.page_count
    injection = scan_injection(text_all_of(blocks))
    mean_conf = (sum(confidences) / len(confidences)) if confidences else None
    quality, _ = quality_checks(
        blocks, headings, len(pages_with), total_pages, 0, fffd_count,
        0, injection, mean_conf,
    )
    ir = build_ir(
        "epub", path, _mime_for("epub", path), seed,
        "pymupdf", version, ["format_detect", "parse", "normalize"],
        blocks, structure, assets, [], started, duration, quality,
    )
    return ir


def _md_lang_for(suffix):
    return CODE_SUFFIXES.get(suffix.lower())


def parse_native(path, fmt, seed):
    """markdown/txt/code -> native_markdown 解析（std-only 行态机）。"""
    started = datetime.now(timezone.utc).isoformat()
    t0 = time.monotonic()
    text, enc = decode_text(path)
    fffd = text.count("\ufffd")
    builder = BlockBuilder(seed, "native_markdown", title_hint=path.name)

    if fmt == "code":
        lang = _md_lang_for(path.suffix)
        builder._add(
            block_type="code",
            content=text if text.strip() else "(empty)",
            code={"language": lang, "source": text},
        )
    elif fmt == "markdown":
        lines = text.splitlines()
        i = 0
        para = []
        fence = None
        fence_lang = None
        fence_lines = []

        def flush_para():
            nonlocal para
            if para:
                body = "\n".join(para).strip()
                if body:
                    builder._add(block_type="paragraph", content=body)
                para = []

        def flush_fence():
            nonlocal fence, fence_lang, fence_lines
            if fence is not None:
                src = "\n".join(fence_lines)
                builder._add(
                    block_type="code",
                    content=src if src.strip() else "(empty fence)",
                    code={"language": fence_lang, "source": src},
                )
            fence, fence_lang, fence_lines = None, None, []

        while i < len(lines):
            line = lines[i]
            if fence is not None:
                if re.match(r"^```+\s*$", line):
                    flush_fence()
                else:
                    fence_lines.append(line)
                i += 1
                continue
            m = re.match(r"^(#{1,6})\s+(.*?)\s*#*\s*$", line)
            if m:
                flush_para()
                builder._add(
                    block_type="heading",
                    heading_level=len(m.group(1)),
                    content=m.group(2),
                )
                i += 1
                continue
            f = re.match(r"^```\s*([A-Za-z0-9_+-]*)\s*$", line)
            if f:
                flush_para()
                fence = True
                fence_lang = f.group(1) or None
                i += 1
                continue
            if re.match(r"^\s*([-*+]|\d+[.)])\s+", line):
                flush_para()
                builder._add(block_type="list", content=line.strip())
                i += 1
                continue
            if line.lstrip().startswith(">"):
                flush_para()
                builder._add(block_type="quote", content=line.strip())
                i += 1
                continue
            if line.strip() == "":
                flush_para()
                i += 1
                continue
            para.append(line)
            i += 1
        flush_fence()
        flush_para()
    else:  # txt
        for para in re.split(r"\n\s*\n", text):
            para = para.strip()
            if para:
                builder._add(block_type="paragraph", content=para)

    blocks, structure = builder.finish()
    duration = int((time.monotonic() - t0) * 1000)
    headings = sum(1 for b in blocks if b["block_type"] in ("heading", "document_title"))
    injection = scan_injection(text_all_of(blocks))
    quality, _ = quality_checks(blocks, headings, 0, None, 0, fffd, 0, injection, None)
    ir = build_ir(
        fmt, path, _mime_for(fmt, path), seed,
        "native_markdown", "native-parser-1.0.0",
        ["format_detect", "parse", "normalize"],
        blocks, structure, [], [], started, duration, quality,
    )
    return ir


def parse_tesseract(path, seed, language="eng+chi_sim", psm=None):
    """Tesseract 兜底 OCR：TSV 输出 -> 块（bbox + 置信度）。"""
    started = datetime.now(timezone.utc).isoformat()
    t0 = time.monotonic()
    try:
        out = subprocess.run(
            ["tesseract", str(path), "stdout", "-l", language, "tsv"]
            + (["--psm", str(psm)] if psm else []),
            capture_output=True, text=True, timeout=540, check=True,
        )
    except subprocess.CalledProcessError as e:
        raise ValueError(f"tesseract failed: {e.stderr[-400:]}") from e
    ver_out = subprocess.run(["tesseract", "--version"], capture_output=True, text=True)
    version = next(
        (ln.strip() for ln in (ver_out.stdout + ver_out.stderr).splitlines() if ln.strip()),
        "tesseract unknown",
    )
    builder = BlockBuilder(seed, "tesseract", title_hint=path.name)
    lines = out.stdout.splitlines()
    if not lines:
        raise ValueError("tesseract tsv empty")
    header = lines[0].split("\t")
    col = {name: idx for idx, name in enumerate(header)}
    confidences = []
    region = []
    for row in lines[1:]:
        cols = row.split("\t")
        if len(cols) != len(header):
            continue
        text = cols[col["text"]].strip()
        if not text:
            if region:
                _flush_tsv_region(builder, region, confidences, col)
                region = []
            continue
        region.append(cols)
    if region:
        _flush_tsv_region(builder, region, confidences, col)

    blocks, structure = builder.finish()
    duration = int((time.monotonic() - t0) * 1000)
    mean_conf = (sum(confidences) / len(confidences)) if confidences else None
    injection = scan_injection(text_all_of(blocks))
    quality, _ = quality_checks(blocks, 0, 0, None, 0, 0, 0, injection, mean_conf)
    ir = build_ir(
        "image", path, _mime_for("image", path), seed,
        "tesseract", version, ["format_detect", "parse", "ocr", "normalize"],
        blocks, structure, [], [], started, duration, quality,
    )
    return ir


def _flush_tsv_region(builder, region, confidences, col):
    text = " ".join(c[col["text"]] for c in region).strip()
    if not text:
        return
    left = min(int(c[col["left"]]) for c in region)
    top = min(int(c[col["top"]]) for c in region)
    right = max(int(c[col["left"]]) + int(c[col["width"]]) for c in region)
    bottom = max(int(c[col["top"]]) + int(c[col["height"]]) for c in region)
    conf = None
    vals = []
    for c in region:
        try:
            v = float(c[col["conf"]])
            if v >= 0:
                vals.append(v)
        except ValueError:
            pass
    if vals:
        conf = sum(vals) / len(vals) / 100.0
        confidences.append(conf)
    builder._add(
        block_type="paragraph",
        content=text,
        location={"bbox": [float(left), float(top), float(right), float(bottom)]},
        extraction_confidence=conf,
    )


def op_detect(req_input):
    path = _in_path(req_input.get("path"))
    fmt, mime, size = detect_format(path)
    sha = _sha256_file(path)
    data = {
        "format": fmt,
        "mime_type": mime,
        "size_bytes": size,
        "sha256": sha,
        "document_type": DOC_TYPE_BY_FORMAT.get(fmt, "other"),
        "extension_hint": path.suffix.lower() or None,
    }
    if fmt == "zip":
        data.update(zip_inventory(path))
    return data


def op_parse(req_input):
    path = _in_path(req_input.get("path"))
    fmt_hint = req_input.get("format")
    fmt, mime, size = detect_format(path)
    # 扩展名级细化只允许在魔法数判定为文本族时收敛（md/txt/code/html/json），
    # 二进制格式以魔法数为准（13-SECURITY：不信扩展名）。
    if fmt_hint and fmt in ("txt", "markdown", "code", "html", "json") \
            and fmt_hint in DOC_TYPE_BY_FORMAT:
        fmt = fmt_hint
    if fmt == "zip":
        raise ValueError("zip must be iterated via detect members (platform-side orchestration)")
    if fmt in ("unknown", "json", "archive-other", "binary-unknown", "other"):
        raise ValueError(f"format not parseable in first batch: {fmt}")
    seed = _sha256_file(path)
    if fmt in ("pdf", "docx", "pptx", "xlsx", "html", "image"):
        ir = parse_docling(path, fmt, seed)
    elif fmt == "epub":
        ir = parse_epub(path, seed)
    elif fmt in ("markdown", "txt", "code"):
        ir = parse_native(path, fmt, seed)
    else:
        raise ValueError(f"unsupported format: {fmt}")
    self_check_schema(ir)
    out_dir = Path(OUT_ROOT)
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "ir.json").write_text(
        json.dumps(ir, ensure_ascii=False), encoding="utf-8"
    )
    return {
        "ir": ir,
        "format": fmt,
        "parser": ir["parser_metadata"]["parser"],
        "parser_version": ir["parser_metadata"]["parser_version"],
        "blocks": len(ir["blocks"]),
        "assets": len(ir["assets"]),
        "overall": ir["extraction_quality"]["overall"],
        "flags": ir["extraction_quality"]["flags"],
        "ir_sha256": hashlib.sha256(
            json.dumps(ir, ensure_ascii=False, sort_keys=True).encode("utf-8")
        ).hexdigest(),
    }


def op_ocr(req_input):
    path = _in_path(req_input.get("path"))
    language = req_input.get("language") or "eng+chi_sim"
    psm = req_input.get("psm")
    seed = _sha256_file(path)
    ir = parse_tesseract(path, seed, language=language, psm=psm)
    self_check_schema(ir)
    out_dir = Path(OUT_ROOT)
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "ir.json").write_text(
        json.dumps(ir, ensure_ascii=False), encoding="utf-8"
    )
    return {
        "ir": ir,
        "format": "image",
        "parser": "tesseract",
        "parser_version": ir["parser_metadata"]["parser_version"],
        "blocks": len(ir["blocks"]),
        "assets": 0,
        "overall": ir["extraction_quality"]["overall"],
        "flags": ir["extraction_quality"]["flags"],
        "ir_sha256": hashlib.sha256(
            json.dumps(ir, ensure_ascii=False, sort_keys=True).encode("utf-8")
        ).hexdigest(),
    }


OPS = {
    "detect": op_detect,
    "parse": op_parse,
    "ocr": op_ocr,
}


def main(argv):
    signal.signal(signal.SIGTERM, _on_term)
    if len(argv) < 2 or argv[1] not in OPS:
        return _emit_error(-32602, "usage: parser_tools.py <detect|parse|ocr>")
    op = argv[1]
    line = sys.stdin.readline()
    if not line.strip():
        return _emit_error(-32700, "empty request line")
    try:
        request = json.loads(line)
    except json.JSONDecodeError as e:
        return _emit_error(-32700, f"request parse failed: {e}")
    req_input = request.get("input") or {}
    # stdout 守卫：D-TC-03 帧 = stdout 第一行；解析期间库噪声（tqdm 进度条/
    # 第三方 print）全部改道 stderr，保证结果帧是 stdout 首行（Adapter 按
    # lines().next() 取帧，podman_host.rs:504）。
    real_stdout = sys.stdout
    sys.stdout = sys.stderr
    try:
        try:
            data = OPS[op](req_input)
        except ValueError as e:
            return _emit_error(-32602, str(e))
        except ImportError as e:
            return _emit_error(-32006, f"engine import failed: {e}")
        except Exception as e:  # noqa: BLE001 容器侧顶层兜底，错误帧不丢栈摘要
            return _emit_error(-32006, f"execution error: {type(e).__name__}: {e}")
    finally:
        sys.stdout = real_stdout
    _emit({"status": "result", "data": data})
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
