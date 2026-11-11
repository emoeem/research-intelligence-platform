# -*- coding: utf-8 -*-
"""parsing-worker 功能测试：md/html/txt 解析、章节切分、标题树、词元统计。"""

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

import _harness
from _harness import WorkerProc, assert_execute_result, assert_protocol_response


MD_SAMPLE = (
    "intro paragraph\n"
    "\n"
    "# Alpha\n"
    "alpha body\n"
    "\n"
    "## Beta\n"
    "beta body line one\n"
    "beta body line two\n"
    "\n"
    "### Gamma\n"
    "gamma body\n"
)


class ParsingFunctionalTest(unittest.TestCase):
    def setUp(self):
        self.w = WorkerProc("parsing", allow_debug=False)
        self.w.send({"jsonrpc": "2.0", "id": "init", "method": "initialize",
                     "params": {"protocol_version": "1"}})
        self.w.recv_for("init")

    def tearDown(self):
        self.w.close()

    def parse(self, task: dict, request_id="x") -> dict:
        self.w.send({"jsonrpc": "2.0", "id": request_id, "method": "execute",
                     "params": {"task": task}})
        msg = self.w.recv_for(request_id)
        assert_protocol_response(msg, request_id)
        return assert_execute_result(msg["result"])

    def test_markdown_sections_and_tree(self):
        result = self.parse({"text": MD_SAMPLE, "format": "md"})
        data = result["data"]
        assert data["format"] == "md"
        titles = [(s["title"], s["level"]) for s in data["sections"]]
        assert titles == [(None, 0), ("Alpha", 1), ("Beta", 2), ("Gamma", 3)], titles
        # 标题树嵌套：Gamma 挂在 Beta 下
        tree = data["heading_tree"]
        assert tree[0]["title"] == "Alpha"
        beta = tree[0]["children"][0]
        assert beta["title"] == "Beta"
        assert beta["children"][0]["title"] == "Gamma"

    def test_markdown_preamble_kept_when_nonempty(self):
        result = self.parse({"text": MD_SAMPLE, "format": "md"})
        # MD_SAMPLE 以 intro 开头，前导章节有内容，必须保留且无标题
        sections = result["data"]["sections"]
        assert sections[0]["title"] is None
        assert sections[0]["word_count"] == 2  # intro + paragraph
        assert len(sections) == 4

    def test_markdown_line_numbers(self):
        result = self.parse({"text": MD_SAMPLE, "format": "md"})
        secs = result["data"]["sections"]
        # 11 行文本；Alpha 标题在第 3 行
        assert secs[1]["start_line"] == 3
        assert secs[1]["end_line"] == 5  # Beta 标题在第 6 行
        assert secs[2]["start_line"] == 6
        assert secs[3]["end_line"] == 11

    def test_html_strips_tags_and_script(self):
        html = ("<html><head><style>.x{color:red}</style>"
                "<script>var evil = 1;</script></head>"
                "<body><h1>Report Title</h1>"
                "<p>first para with CVE-2024-1234</p>"
                "<h2>Details</h2><p>second para</p></body></html>")
        result = self.parse({"text": html, "format": "html"})
        data = result["data"]
        titles = [s["title"] for s in data["sections"]]
        assert titles == ["Report Title", "Details"], titles
        all_text = json.dumps(data, ensure_ascii=False)
        assert "evil" not in all_text and ".x" not in all_text
        assert "CVE-2024-1234" in all_text
        assert data["sections"][0]["start_line"] is None  # html 无源行号

    def test_txt_single_section(self):
        result = self.parse({"text": "plain text doc\nsecond line\n",
                             "format": "txt"})
        data = result["data"]
        assert len(data["sections"]) == 1
        assert data["sections"][0]["title"] is None
        assert data["heading_tree"] == []
        assert data["stats"]["lines"] == 2

    def test_file_path_mode_with_sha256(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "doc.md"
            raw = "# From File\ncontent body\n".encode("utf-8")
            p.write_bytes(raw)
            result = self.parse({"path": str(p)})
            data = result["data"]
            assert data["format"] == "md"  # 扩展名推断
            src = data["source"]
            assert src["kind"] == "file" and src["path"] == str(p)
            assert src["bytes"] == len(raw)
            assert src["sha256"] == hashlib.sha256(raw).hexdigest()
            assert data["sections"][0]["title"] == "From File"

    def test_invalid_path_input_error(self):
        result = self.parse({"path": "/nonexistent/nope.md"})
        assert result["status"] == "error"
        assert result["error"]["code"] == -32008

    def test_unsupported_format(self):
        result = self.parse({"text": "hello", "format": "pdf"})
        assert result["status"] == "error"
        assert result["error"]["code"] == -32009

    def test_text_and_path_mutually_exclusive(self):
        result = self.parse({"text": "a", "path": "/tmp/x.txt"})
        assert result["status"] == "error"
        assert result["error"]["code"] == -32602

    def test_stats_cjk_tokens(self):
        result = self.parse({"text": "安全研究 cyber scan\n安全", "format": "txt"})
        stats = result["data"]["stats"]
        assert stats["cjk_chars"] == 6
        assert stats["ascii_words"] == 2
        assert stats["words"] == 8  # 2 ascii 词 + 6 cjk 字
        assert stats["tokens_est"] == 8

    def test_sha256_inline(self):
        text = "hash me"
        result = self.parse({"text": text, "format": "txt"})
        expected = hashlib.sha256(text.encode("utf-8")).hexdigest()
        assert result["data"]["source"]["sha256"] == expected
        assert result["data"]["source"]["kind"] == "inline"

    def test_extract_links_markdown_skips_fence_and_filters_scheme(self):
        # SK-1（P6-9 第 4 步）：可选 extract_links 输出；围栏代码块不抽，
        # javascript:/相对路径/纯锚点过滤并计数。
        doc = (
            "---\n"
            "title: T\n"
            "description: D\n"
            "---\n"
            "\n"
            "# 分类\n"
            "\n"
            "- [Name](https://a.example/x/): 描述一\n"
            "\n"
            "| 列 | 链接 |\n"
            "|---|---|\n"
            "| a | [Tbl](http://b.example/y) |\n"
            "\n"
            "```sh\n"
            "[bad](javascript:alert(1))\n"
            "```\n"
            "\n"
            "[rel](./other.md) 与 [evil](javascript:x) 与 [anchor](#top)\n"
        )
        result = self.parse({"text": doc, "format": "md", "extract_links": True})
        data = result["data"]
        links = data["links"]
        assert [(l["text"], l["url"]) for l in links] == [
            ("Name", "https://a.example/x"), ("Tbl", "http://b.example/y")]
        assert links[0]["line"] == 8 and links[0]["heading_path"] == ["分类"]
        assert links[0]["description"] == "描述一"
        assert links[1]["in_table"] is True
        assert data["link_stats"] == {
            "total": 5, "kept": 2, "scheme_rejected": 1, "no_scheme": 2}
        assert data["frontmatter"] == {
            "present": True, "pairs": {"title": "T", "description": "D"}}

    def test_extract_links_absent_by_default(self):
        # 缺省（无 extract_links）不产新键：既有输出契约零回归。
        result = self.parse({"text": "# A\n[link](https://x.example)\n", "format": "md"})
        data = result["data"]
        assert "links" not in data and "frontmatter" not in data
        assert "link_stats" not in data

    def test_extract_links_txt_html_empty_and_no_frontmatter(self):
        result = self.parse({"text": "纯文本", "format": "txt", "extract_links": True})
        data = result["data"]
        assert data["links"] == []
        assert data["frontmatter"]["present"] is False


if __name__ == "__main__":
    unittest.main()
