#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Catalog Worker：链接目录条目抽取（SK-3，P6-11 第 1 步，rip.knowledge.catalog.extract）。

T0 纪律（安全知识源计划 §6.2A）：纯解析，不访问链接，不执行内容；
scheme 白名单 http/https（javascript:/data:/file: 等过滤并计数）；保留原始
URL，不从文字猜造；HTML 剥 script/style；超量条目分页截断报告。

复用优先（用户硬性规则）：Markdown 路径直接 import parsing_worker 既有
extract_markdown_links / _link_scheme / extract_frontmatter（SK-1 落地函数，
零重写）；HTML 锚点抽取按 parsing_worker._HTMLTextExtractor 同款
script/style 剔除语义实现（parsing_worker 的 HTML 面只产文本 token，
不抽 href，无可复用函数，属补齐而非重写）。

输入 execute.task：
  {"source_id": "...", "text": "...", "format": "md"|"html"|"txt",
   "category_map": {"原分类": "规范分类", ...},      可选
   "max_entries": 200, "offset": 0}                 可选（分页截断）
  或 {"source_id": "...", "path": "/abs/catalog.md", ...}
输出 data：
  source_id / format / entries[] / stats{} / truncated / next_offset /
  paging{} / source{kind,path,bytes,sha256} / frontmatter（md 专属）

entries[] 每项：name / description / canonical_url / raw_url / raw_link_text /
category_path / resource_kind / source_location{line,file} / dedupe_key /
confidence / meta{in_table,host}。
"""

from __future__ import annotations

import hashlib
import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parent / "runtime"))

from parsing_worker import _link_scheme, extract_frontmatter, extract_markdown_links  # noqa: E402
from worker_runtime import (WorkerRuntime, WorkerContext, InputError,  # noqa: E402
                            InvalidParams)

MAX_INPUT_BYTES = 10 * 1024 * 1024  # 输入硬上限 10 MiB（与 rip.doc.parse 同款）
SUPPORTED_FORMATS = ("md", "html", "txt")
MAX_ENTRIES_LIMIT = 100_000  # max_entries 参数硬上限
REPO_HOSTS = {"github.com", "gitlab.com", "gitee.com"}


def normalize_url(url: str) -> str:
    """规范化（与 plugin_runtime::knowledge_parse::normalize_url 同语义）：
    去首尾空白；http/https 去尾部 `/`；其余原样保留。"""
    u = url.strip()
    lower_scheme = u.split("://", 1)[0].lower() if "://" in u else ""
    if lower_scheme in ("http", "https"):
        while u.endswith("/"):
            u = u[:-1]
    return u


def url_host(url: str) -> str:
    """URL host（小写、去端口、去 userinfo）；解析失败返回空串。"""
    try:
        parts = urlsplit(url)
        host = (parts.hostname or "").lower()
        return host
    except ValueError:
        return ""


def resource_kind_of(url: str) -> str:
    """resource_kind 启发式（只看 URL 形状，不猜内容类型）：github/gitlab/
    gitee 的 owner/repo 两段路径形 → repo；其余一律 link（与 ingest 落库
    meta.resource_kind=link 基线一致，host 信息在 meta 单列）。"""
    try:
        parts = urlsplit(url)
        host = (parts.hostname or "").lower()
        segments = [s for s in parts.path.split("/") if s]
        if host in REPO_HOSTS and len(segments) >= 2:
            return "repo"
    except ValueError:
        pass
    return "link"


def confidence_of(name: str, description: str, category_path: list) -> float:
    """置信度（确定性公式，不做任何网络核验）：基线 0.6；有名称 +0.2；
    有后缀描述 +0.1；有分类路径 +0.1；区间 [0.6, 1.0]。"""
    score = 0.6
    if name.strip():
        score += 0.2
    if description.strip():
        score += 0.1
    if category_path:
        score += 0.1
    return round(min(score, 1.0), 2)


class _HTMLAnchorExtractor(HTMLParser):
    """HTML `<a href>` 锚点抽取：script/style 剔除语义与 parsing_worker
    _HTMLTextExtractor 同款（skip_depth 计数）；行号来自 getpos()。"""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.anchors: list[dict] = []
        self._skip_depth = 0
        self._a_href: str | None = None
        self._a_text: list[str] = []
        self._a_line: int = 0

    def handle_starttag(self, tag, attrs):
        tag = tag.lower()
        if tag in ("script", "style"):
            self._skip_depth += 1
            return
        if tag == "a" and self._skip_depth == 0 and self._a_href is None:
            href = dict(attrs).get("href")
            if href is not None:
                self._a_href = href.strip()
                self._a_text = []
                self._a_line = self.getpos()[0]

    def handle_endtag(self, tag):
        tag = tag.lower()
        if tag in ("script", "style"):
            self._skip_depth = max(0, self._skip_depth - 1)
            return
        if tag == "a" and self._a_href is not None:
            text = " ".join("".join(self._a_text).split())
            self.anchors.append({
                "line": self._a_line,
                "text": text,
                "url": self._a_href,
            })
            self._a_href = None
            self._a_text = []

    def handle_data(self, data):
        if self._skip_depth:
            return
        if self._a_href is not None:
            self._a_text.append(data)

    def close(self) -> None:
        super().close()
        if self._a_href is not None:  # 未闭合 <a>：按已收集文本收口，不丢 href
            self.anchors.append({
                "line": self._a_line,
                "text": " ".join("".join(self._a_text).split()),
                "url": self._a_href,
            })
            self._a_href = None
            self._a_text = []


def extract_html_anchors(raw: str) -> tuple[list[dict], dict]:
    """HTML 锚点抽取（script/style 剔除；scheme 白名单过滤计数）。"""
    parser = _HTMLAnchorExtractor()
    parser.feed(raw)
    parser.close()
    stats = {"total": 0, "kept": 0, "scheme_rejected": 0, "no_scheme": 0}
    kept = []
    for anchor in parser.anchors:
        stats["total"] += 1
        scheme = _link_scheme(anchor["url"])
        if scheme is None:
            stats["no_scheme"] += 1
            continue
        if scheme not in ("http", "https"):
            stats["scheme_rejected"] += 1
            continue
        stats["kept"] += 1
        anchor["in_table"] = False
        kept.append(anchor)
    return kept, stats


class CatalogWorker(WorkerRuntime):
    NAME = "catalog-extract-worker"
    VERSION = "0.1.0"
    CAPABILITIES = ["knowledge.catalog.extract.md", "knowledge.catalog.extract.html"]

    def execute(self, ctx: WorkerContext, task: dict) -> dict:
        if "op" in task:  # op 前缀保留给运行时（debug sleep/boom 等）
            return super().execute(ctx, task)
        source_id = task.get("source_id")
        if not isinstance(source_id, str) or not source_id.strip() or len(source_id) > 512:
            raise InvalidParams("task.source_id must be a non-empty string <=512 chars")
        text, fmt, source_info = self._load_input(task)
        ctx.check_cancelled()

        if fmt == "md":
            raw_links, link_stats = extract_markdown_links(text)
            frontmatter = extract_frontmatter(text)
        elif fmt == "html":
            raw_links, link_stats = extract_html_anchors(text)
            frontmatter = {"present": False, "pairs": {}}
        else:  # txt：无链接目录语义，诚实零条目
            raw_links, link_stats = [], {"total": 0, "kept": 0,
                                         "scheme_rejected": 0, "no_scheme": 0}
            frontmatter = {"present": False, "pairs": {}}
        ctx.check_cancelled()

        category_map = task.get("category_map") or {}
        if not isinstance(category_map, dict) or \
                any(not isinstance(k, str) or not isinstance(v, str)
                    for k, v in category_map.items()):
            raise InvalidParams("task.category_map must map strings to strings")

        # 双重防御：extract_* 已按 scheme 白名单过滤，产出前再以 _link_scheme
        # 复核一次 canonical URL（拦截任何未来解析器回归）。
        unique: dict[str, dict] = {}
        stats = {
            "total": link_stats["total"],
            "kept": link_stats["kept"],
            "scheme_rejected": link_stats["scheme_rejected"],
            "no_scheme": link_stats["no_scheme"],
            "deduped": 0,
            "scheme_rejected_final": 0,
            "returned": 0,
        }
        for link in raw_links:
            canonical = normalize_url(link["url"])
            if not canonical:
                continue
            if _link_scheme(canonical) not in ("http", "https"):
                stats["scheme_rejected_final"] += 1
                continue
            if canonical in unique:
                stats["deduped"] += 1  # 首见胜出，后续重复只计数
                continue
            name = link["text"].strip() or canonical
            description = link.get("description", "") or ""
            category_path = [category_map.get(h, h) for h in link.get("heading_path", [])]
            file_ref = source_info.get("path")
            # 置信度看源侧证据（原链接文字是否非空），不看 fallback 后的 name
            unique[canonical] = {
                "name": name,
                "description": description,
                "canonical_url": canonical,
                "raw_url": link["url"],
                "raw_link_text": link["text"],
                "category_path": category_path,
                "resource_kind": resource_kind_of(canonical),
                "source_location": {
                    "line": link["line"],
                    "file": file_ref,
                },
                "dedupe_key": canonical,
                "confidence": confidence_of(link["text"], description, category_path),
                "meta": {
                    "in_table": bool(link.get("in_table", False)),
                    "host": url_host(canonical),
                },
            }

        all_entries = list(unique.values())
        total_unique = len(all_entries)
        offset = task.get("offset", 0)
        max_entries = task.get("max_entries")
        if max_entries is not None:
            if not isinstance(max_entries, int) or isinstance(max_entries, bool) \
                    or not (1 <= max_entries <= MAX_ENTRIES_LIMIT):
                raise InvalidParams(
                    f"task.max_entries must be integer in [1, {MAX_ENTRIES_LIMIT}]")
        if not isinstance(offset, int) or isinstance(offset, bool) or offset < 0:
            raise InvalidParams("task.offset must be a non-negative integer")

        entries = all_entries[offset:] if offset else all_entries
        truncated = False
        next_offset = None
        if max_entries is not None and len(entries) > max_entries:
            entries = entries[:max_entries]
            truncated = True
            remaining_after = (offset or 0) + len(entries)
            if remaining_after < total_unique:
                next_offset = remaining_after
        stats["returned"] = len(entries)
        stats["unique"] = total_unique

        out = {
            "source_id": source_id.strip(),
            "format": fmt,
            "entries": entries,
            "stats": stats,
            "truncated": truncated,
            "next_offset": next_offset,
            "paging": {"offset": offset, "max_entries": max_entries},
            "source": source_info,
        }
        if fmt == "md":
            out["frontmatter"] = frontmatter
        return out

    def _load_input(self, task: dict) -> tuple[str, str, dict]:
        fmt = task.get("format")
        if fmt is not None and fmt not in SUPPORTED_FORMATS:
            raise InvalidParams(
                f"unsupported format: {fmt!r}, supported: {SUPPORTED_FORMATS}")
        text = task.get("text")
        path = task.get("path")
        if (text is None) == (path is None):
            raise InvalidParams("task requires exactly one of text|path")
        if text is not None:
            if not isinstance(text, str):
                raise InvalidParams("task.text must be a string")
            if len(text.encode("utf-8")) > MAX_INPUT_BYTES:
                raise InputError(f"inline text exceeds {MAX_INPUT_BYTES} bytes")
            resolved_fmt = fmt or "md"
            info = {"kind": "inline", "path": None,
                    "bytes": len(text.encode("utf-8")),
                    "sha256": hashlib.sha256(text.encode("utf-8")).hexdigest()}
            return text, resolved_fmt, info

        p = Path(str(path))
        try:
            if not p.is_file():
                raise InputError(f"path is not a readable file: {str(p)!r}")
            size = p.stat().st_size
            if size > MAX_INPUT_BYTES:
                raise InputError(f"file exceeds {MAX_INPUT_BYTES} bytes: {size}")
            raw = p.read_bytes()
        except InputError:
            raise
        except OSError as exc:
            raise InputError(f"cannot read path: {exc}")
        inferred = {".md": "md", ".markdown": "md", ".html": "html",
                    ".htm": "html", ".txt": "txt", ".text": "txt"}
        resolved_fmt = fmt or inferred.get(p.suffix.lower(), "md")
        info = {"kind": "file", "path": str(p), "bytes": len(raw),
                "sha256": hashlib.sha256(raw).hexdigest()}
        return raw.decode("utf-8", errors="replace"), resolved_fmt, info


if __name__ == "__main__":
    CatalogWorker().serve()
