#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Parsing Worker：文档解析与结构化抽取（M3）。

输入 execute.task：
  {"text": "...", "format": "md"|"txt"|"html"}   内联文本（format 缺省按 txt）
  {"path": "/abs/file.md", "format": "md"}       文件路径（缺省按扩展名推断）
输出 data：
  format / source(sha256) / sections[] / heading_tree[] / stats{}

支持 .md/.txt/.html；html 用标准库 html.parser 剥标签（script/style 剔除）。
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from html.parser import HTMLParser
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "runtime"))

from worker_runtime import (WorkerRuntime, WorkerContext, InputError,
                            UnsupportedFormatError, InvalidParams)

MAX_INPUT_BYTES = 10 * 1024 * 1024  # 输入硬上限 10 MiB
SUPPORTED_FORMATS = ("md", "txt", "html")

_ATX_RE = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")
_CJK_RE = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\u3040-\u30ff]")
_WORD_RE = re.compile(r"[A-Za-z0-9_']+")


def tokenize(text: str) -> list[str]:
    """ASCII 词（小写）+ 逐个 CJK/假名字符，统一作为词元。"""
    return _WORD_RE.findall(text.lower()) + _CJK_RE.findall(text)


class _HTMLTextExtractor(HTMLParser):
    """剥标签并抽取 h1-h6 标题，产出 (heading|text) token 流。"""

    _HEADINGS = {"h1": 1, "h2": 2, "h3": 3, "h4": 4, "h5": 5, "h6": 6}
    _BLOCK = {"p", "div", "br", "li", "tr", "section", "article", "ul", "ol",
              "table", "pre", "blockquote", "header", "footer", "hr"}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.tokens: list[tuple[str, int, str]] = []
        self._skip_depth = 0
        self._head_level = 0
        self._head_buf: list[str] = []
        self._text_buf: list[str] = []

    def handle_starttag(self, tag, attrs):
        tag = tag.lower()
        if tag in ("script", "style"):
            self._skip_depth += 1
        elif tag in self._HEADINGS:
            self._flush_text()
            self._head_level = self._HEADINGS[tag]
            self._head_buf = []
        elif tag in self._BLOCK:
            self._text_buf.append("\n")

    def handle_endtag(self, tag):
        tag = tag.lower()
        if tag in ("script", "style"):
            self._skip_depth = max(0, self._skip_depth - 1)
        elif tag in self._HEADINGS and self._head_level:
            self._flush_text()
            self.tokens.append(("heading", self._head_level,
                                " ".join("".join(self._head_buf).split())))
            self._head_level = 0
            self._head_buf = []
        elif tag in self._BLOCK:
            self._text_buf.append("\n")

    def handle_data(self, data):
        if self._skip_depth:
            return
        if self._head_level:
            self._head_buf.append(data)
        else:
            self._text_buf.append(data)

    def _flush_text(self):
        chunk = "".join(self._text_buf)
        self._text_buf = []
        if chunk.strip():
            self.tokens.append(("text", 0, chunk))


def parse_html_tokens(raw: str) -> list[tuple[str, int, str]]:
    p = _HTMLTextExtractor()
    try:
        p.feed(raw)
        p.close()
    except Exception as exc:
        raise InputError(f"html parse failed: {exc}")
    p._flush_text()
    return p.tokens


def parse_markdown_tokens(raw: str) -> list[tuple[str, int, str]]:
    """ATX 标题 + setext 下划线标题，产出 (heading|text) token 流。"""
    tokens: list[tuple[str, int, str]] = []
    lines = raw.splitlines()
    buf: list[str] = []
    prev_text: str | None = None

    def flush_text():
        nonlocal buf, prev_text
        parts = ([prev_text] if prev_text is not None else []) + buf
        chunk = "\n".join(parts)
        buf = []
        prev_text = None
        if chunk.strip():
            tokens.append(("text", 0, chunk))

    i = 0
    while i < len(lines):
        line = lines[i]
        m = _ATX_RE.match(line)
        if m:
            flush_text()
            tokens.append(("heading", len(m.group(1)), m.group(2)))
            i += 1
            continue
        stripped = line.strip()
        if prev_text is not None and stripped and set(stripped) <= {"="}:
            tokens.append(("heading", 1, prev_text))
            prev_text = None
            i += 1
            continue
        if prev_text is not None and stripped and set(stripped) <= {"-"}:
            tokens.append(("heading", 2, prev_text))
            prev_text = None
            i += 1
            continue
        if stripped:
            if prev_text is not None:
                buf.append(line)
            else:
                prev_text = line
        elif prev_text is not None or buf:
            buf.append(line)
        i += 1
    flush_text()
    return tokens


def parse_text_tokens(raw: str) -> list[tuple[str, int, str]]:
    if raw.strip():
        return [("text", 0, raw)]
    return []


def build_sections(tokens: list[tuple[str, int, str]],
                   with_lines: bool, total_lines: int) -> tuple[list[dict], list[dict]]:
    """token 流 -> 章节列表 + 标题树。md 带行号，html 行号为 null。"""
    sections: list[dict] = []
    current = {"index": 0, "title": None, "level": 0, "start_line": 1,
               "end_line": total_lines, "lines": [], "char_count": 0}
    line_no = 1

    def close_current():
        content = "\n".join(current["lines"]).strip("\n")
        toks = tokenize(content)
        current["word_count"] = len(toks)
        current["char_count"] = len(content)
        current["content"] = content
        current.pop("lines")
        sections.append(current)

    if not with_lines:
        current["start_line"] = None
        current["end_line"] = None

    for kind, level, content in tokens:
        chunk_lines = content.count("\n") + 1
        if kind == "heading":
            close_current()
            current = {"index": len(sections), "title": content, "level": level,
                       "start_line": line_no if with_lines else None,
                       "end_line": None, "lines": [content], "char_count": 0}
        else:
            current["lines"].append(content)
        line_no += chunk_lines
    close_current()
    # 文件直接以标题开头时，丢弃空的前导章节并重排 index
    if len(sections) > 1 and sections[0]["title"] is None \
            and sections[0]["word_count"] == 0:
        sections.pop(0)
    for i, sec in enumerate(sections):
        sec["index"] = i
    # 结束行：下一章节起始行 - 1，末章为总行数
    for i, sec in enumerate(sections):
        if not with_lines:
            continue
        nxt = sections[i + 1]["start_line"] if i + 1 < len(sections) else None
        sec["end_line"] = (nxt - 1) if nxt else total_lines

    # 标题树
    tree: list[dict] = []
    stack: list[dict] = []
    for sec in sections:
        if sec["title"] is None:
            continue
        node = {"title": sec["title"], "level": sec["level"],
                "line": sec["start_line"], "children": []}
        while stack and stack[-1]["level"] >= node["level"]:
            stack.pop()
        if stack:
            stack[-1]["children"].append(node)
        else:
            tree.append(node)
        stack.append(node)
    return sections, tree


class ParsingWorker(WorkerRuntime):
    NAME = "parsing-worker"
    VERSION = "0.1.0"
    CAPABILITIES = ["parse.markdown", "parse.text", "parse.html", "stats.tokens"]

    def execute(self, ctx: WorkerContext, task: dict) -> dict:
        if "op" in task:  # op 前缀保留给运行时（debug sleep/boom 等）
            return super().execute(ctx, task)
        text, fmt, source_info = self._load_input(task)
        ctx.check_cancelled()

        if fmt == "html":
            tokens = parse_html_tokens(text)
        elif fmt == "md":
            tokens = parse_markdown_tokens(text)
        else:
            tokens = parse_text_tokens(text)
        ctx.check_cancelled()

        total_lines = 0
        if text:
            total_lines = text.count("\n") + (0 if text.endswith("\n") else 1)
        sections, tree = build_sections(tokens, with_lines=(fmt != "html"),
                                        total_lines=total_lines)
        toks = tokenize(text)
        stats = {
            "chars": len(text),
            "lines": total_lines,
            "words": len(toks),
            "unique_words": len(set(toks)),
            "cjk_chars": len(_CJK_RE.findall(text)),
            "ascii_words": len(_WORD_RE.findall(text)),
            "tokens_est": len(toks),
            "sections": len(sections),
            "headings": sum(1 for s in sections if s["title"] is not None),
        }
        return {
            "format": fmt,
            "source": source_info,
            "sections": sections,
            "heading_tree": tree,
            "stats": stats,
        }

    def _load_input(self, task: dict) -> tuple[str, str, dict]:
        fmt = task.get("format")
        if fmt is not None and fmt not in SUPPORTED_FORMATS:
            raise UnsupportedFormatError(
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
            resolved_fmt = fmt or "txt"
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
        resolved_fmt = fmt or inferred.get(p.suffix.lower(), "txt")
        info = {"kind": "file", "path": str(p), "bytes": len(raw),
                "sha256": hashlib.sha256(raw).hexdigest()}
        return raw.decode("utf-8", errors="replace"), resolved_fmt, info


if __name__ == "__main__":
    ParsingWorker().serve()
