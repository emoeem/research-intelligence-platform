# -*- coding: utf-8 -*-
"""SK-3 catalog.extract 测试：Worker Protocol v1 契约（ContractMixin）+
目录抽取单元断言（scheme 拒绝 / 截断分页 / 去重键 / 分类映射 / 不猜造 URL）。

安全知识源计划 §6.2A：纯解析不访问链接，javascript:/data:/file: 等非
http(s) scheme 拒绝并计数，保留原始 URL，HTML 剥 script/style。
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from _harness import ContractMixin, WORKERS_PY  # noqa: E402

import catalog_worker  # noqa: E402
from catalog_worker import normalize_url, resource_kind_of, url_host  # noqa: E402
from parsing_worker import _link_scheme  # noqa: E402


class CatalogContractTests(ContractMixin, unittest.TestCase):
    """Worker Protocol v1 全契约（握手/预算/取消/幂等/截断/崩溃安全）。"""

    WORKER = "catalog"

    def expected_worker_name(self) -> str:
        return "catalog-extract-worker"


def run_catalog(task: dict) -> dict:
    """直接驱动 worker（不经子进程），便于单元断言。"""
    import worker_runtime

    ctx = worker_runtime.WorkerContext(
        catalog_worker.CatalogWorker(), "unit", task, 30_000, 67_883_264, None)
    return catalog_worker.CatalogWorker().execute(ctx, task)


MD_DOC = """---
title: catalog
ignore: "[x](javascript:alert(1))"
---

# Agent 工具

- [Nucleus](https://github.com/abc/nucleus): fuzzing framework
- [Docs](https://docs.example.com/guide): user guide

```sh
- [Fenced](javascript:alert(1))
- [Fenced2](https://inside.example/fence)
```

| T | [Tbl](http://tbl.example/x) |
| --- | --- |

- [Rel](./other.md)
- [Anchor](#top)
- [Evil](javascript:alert(1))
- [Data](data:text/plain,hi)
- [File](file:///etc/passwd)
"""


class CatalogExtractTests(unittest.TestCase):
    """目录抽取单元断言（与契约测试互补，直接函数级驱动）。"""

    def test_scheme_rejection_counts(self):
        out = run_catalog({"source_id": "s", "text": MD_DOC, "format": "md"})
        urls = {e["canonical_url"] for e in out["entries"]}
        # 围栏内不抽（Fenced2 不在），frontmatter 内不抽（javascript: 不计）
        self.assertEqual(urls, {"https://github.com/abc/nucleus",
                                "https://docs.example.com/guide",
                                "http://tbl.example/x"})
        st = out["stats"]
        # 正文内 javascript:/data:/file: 三条 scheme 拒绝；./other.md 与 #top
        # 两条无 scheme；frontmatter 的 javascript: 不进入解析面
        self.assertEqual(st["scheme_rejected"], 3)
        self.assertEqual(st["no_scheme"], 2)
        self.assertEqual(st["kept"], 3)
        self.assertEqual(st["scheme_rejected_final"], 0)

    def test_entry_shape_and_dedupe_key(self):
        out = run_catalog({"source_id": "s", "text": MD_DOC, "format": "md"})
        entry = out["entries"][0]
        for key in ("name", "description", "canonical_url", "raw_url",
                    "raw_link_text", "category_path", "resource_kind",
                    "source_location", "dedupe_key", "confidence", "meta"):
            self.assertIn(key, entry)
        self.assertEqual(entry["dedupe_key"], entry["canonical_url"])
        self.assertEqual(entry["name"], "Nucleus")
        self.assertEqual(entry["description"], "fuzzing framework")
        self.assertEqual(entry["category_path"], ["Agent 工具"])
        self.assertEqual(entry["source_location"]["line"], 8)
        self.assertFalse(entry["meta"]["in_table"])

    def test_dedupe_first_wins_counts(self):
        doc = "# T\n\n- [First](https://dup.example/x): one\n" \
              "- [Second](https://dup.example/x): two\n" \
              "- [Third](https://dup.example/x/): three\n"
        out = run_catalog({"source_id": "s", "text": doc, "format": "md"})
        self.assertEqual(len(out["entries"]), 1)
        self.assertEqual(out["entries"][0]["name"], "First")
        self.assertEqual(out["stats"]["deduped"], 2)

    def test_truncation_paging_and_next_offset(self):
        doc = "# T\n" + "".join(
            f"- [n{i}](https://p.example/{i}): d\n" for i in range(5))
        out = run_catalog({"source_id": "s", "text": doc, "format": "md",
                           "max_entries": 2})
        self.assertTrue(out["truncated"])
        self.assertEqual(out["next_offset"], 2)
        self.assertEqual([e["name"] for e in out["entries"]],
                         ["n0", "n1"])
        self.assertEqual(out["stats"]["returned"], 2)
        self.assertEqual(out["stats"]["unique"], 5)
        page2 = run_catalog({"source_id": "s", "text": doc, "format": "md",
                             "max_entries": 2, "offset": 2})
        self.assertEqual([e["name"] for e in page2["entries"]], ["n2", "n3"])
        self.assertEqual(page2["next_offset"], 4)
        page3 = run_catalog({"source_id": "s", "text": doc, "format": "md",
                             "max_entries": 2, "offset": 4})
        self.assertEqual([e["name"] for e in page3["entries"]], ["n4"])
        self.assertFalse(page3["truncated"])
        self.assertIsNone(page3["next_offset"])

    def test_category_map_remap(self):
        doc = "# Agent 工具\n\n- [X](https://x.example/a)\n"
        out = run_catalog({"source_id": "s", "text": doc, "format": "md",
                           "category_map": {"Agent 工具": "Agent & MCP"}})
        self.assertEqual(out["entries"][0]["category_path"], ["Agent & MCP"])

    def test_html_script_style_stripped_and_anchors(self):
        html = ("<html><head>\n<style>a{color:red}</style>\n"
                "<script>var a = '<a href=\"https://trap.example/s\">t</a>';</script>\n"
                "</head><body>\n"
                "<a href='https://ok.example/one'>One</a> desc\n"
                "<p><a href=\"javascript:bad\">JS</a></p>\n"
                "<a href='#frag'>Frag</a>\n"
                "</body></html>")
        out = run_catalog({"source_id": "s", "text": html, "format": "html"})
        urls = [e["canonical_url"] for e in out["entries"]]
        self.assertEqual(urls, ["https://ok.example/one"])
        self.assertEqual(out["stats"]["scheme_rejected"], 1)
        self.assertEqual(out["stats"]["no_scheme"], 1)
        self.assertEqual(out["entries"][0]["source_location"]["line"], 5)

    def test_no_url_fabrication_and_raw_preserved(self):
        doc = "- [Rel](./other.md)\n- [Trailing](https://t.example/path/)\n"
        out = run_catalog({"source_id": "s", "text": doc, "format": "md"})
        self.assertEqual(len(out["entries"]), 1)
        entry = out["entries"][0]
        self.assertEqual(entry["canonical_url"], "https://t.example/path")
        # parsing_worker.extract_markdown_links 在抽取层即剥尾斜杠（既有行为，
        # SK-1 登记）：raw_url 与 canonical 一致，worker 不再二次猜造
        self.assertEqual(entry["raw_url"], "https://t.example/path")
        self.assertEqual(out["stats"]["no_scheme"], 1)

    def test_resource_kind_host_and_confidence(self):
        self.assertEqual(
            resource_kind_of("https://github.com/abc/nucleus"), "repo")
        self.assertEqual(resource_kind_of("https://docs.example.com/guide"),
                         "link")
        self.assertEqual(url_host("https://GitHub.com/A/B"), "github.com")
        doc = "# T\n\n- [A](https://github.com/abc/nucleus): desc here\n" \
              "- [](https://bare.example/x)\n"
        out = run_catalog({"source_id": "s", "text": doc, "format": "md"})
        self.assertEqual(out["entries"][0]["resource_kind"], "repo")
        self.assertEqual(out["entries"][0]["confidence"], 1.0)
        self.assertEqual(out["entries"][1]["name"], "https://bare.example/x")
        # 空文字链接：源侧无名称证据（0.6）+ 有分类路径（0.1）
        self.assertEqual(out["entries"][1]["confidence"], 0.7)
        # 空文字链接回退 canonical URL，不丢条目
        self.assertEqual(out["entries"][1]["resource_kind"], "link")

    def test_normalize_url_semantics(self):
        self.assertEqual(normalize_url(" https://a.example/x/ "),
                         "https://a.example/x")
        self.assertEqual(normalize_url("ftp://a.example/x/"),
                         "ftp://a.example/x/")  # 非 http/https 不剥尾斜杠
        self.assertTrue(_link_scheme("https://a.example") == "https")

    def test_txt_zero_entries(self):
        out = run_catalog({"source_id": "s", "text": "plain text no links",
                           "format": "txt"})
        self.assertEqual(out["entries"], [])
        self.assertEqual(out["stats"]["total"], 0)
        self.assertNotIn("frontmatter", out)

    def test_frontmatter_included_for_md(self):
        out = run_catalog({"source_id": "s", "text": MD_DOC, "format": "md"})
        self.assertTrue(out["frontmatter"]["present"])
        self.assertEqual(out["frontmatter"]["pairs"].get("title"), "catalog")

    def test_input_validation_rejections(self):
        cases = [
            {"text": "x"},                                     # 缺 source_id
            {"source_id": "s"},                                # 缺 text/path
            {"source_id": "s", "text": "a", "path": "b"},      # 双输入
            {"source_id": "", "text": "a"},                    # 空 source_id
            {"source_id": "s", "text": "a", "format": "xml"},  # 未知 format
            {"source_id": "s", "text": "a", "max_entries": 0},
            {"source_id": "s", "text": "a", "offset": -1},
            {"source_id": "s", "text": "a", "category_map": {"a": 1}},
        ]
        for task in cases:
            with self.assertRaises(Exception) as cm:
                run_catalog(task)
            self.assertEqual(getattr(cm.exception, "code", None), -32602,
                             f"task={task!r}")


if __name__ == "__main__":
    unittest.main()
