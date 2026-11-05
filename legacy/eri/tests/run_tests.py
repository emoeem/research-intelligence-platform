#!/usr/bin/env python3
"""ERI P0 测试套件：解析 / 规范化 / 去重 / 幂等 / 增量 / 软删除 / 检索。

运行：python3 -I tests/run_tests.py   （-I 隔离模式：本文件显式注入路径）
每个测试组用独立临时数据库，互不污染；全程离线，不访问网络。
"""
import itertools
import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.join(HERE, "fixtures")
sys.path.insert(0, os.path.join(HERE, ".."))

TMP = tempfile.mkdtemp(prefix="eri-test-")
os.environ.setdefault("ERI_DB", os.path.join(TMP, "catalog.db"))
import eri  # noqa: E402  (env 必须先设)

_db_counter = itertools.count()
FAILURES = []


def check(name, cond, detail=""):
    if cond:
        print(f"  ok   {name}")
    else:
        print(f"  FAIL {name}  {detail}")
        FAILURES.append((name, detail))


def fresh_db():
    for suffix in ("", "-wal", "-shm"):
        if os.path.exists(eri.DB_PATH):
            os.remove(eri.DB_PATH)
    eri.DB_PATH = os.path.join(TMP, f"db-{next(_db_counter)}", "catalog.db")
    os.makedirs(os.path.dirname(eri.DB_PATH), exist_ok=True)


# ── 1. URL 规范化 ────────────────────────────────────────────────────────────
def test_normalizer():
    print("[1] URL normalization (D2/D3)")
    cases = [
        ("https://github.com/example/tool.git", "https://github.com/example/tool",
         ("example", "tool")),
        ("https://GitHub.com/Example/Tool/", "https://github.com/Example/Tool",
         ("Example", "Tool")),
        ("https://github.com/example/tool/tree/main/docs", "https://github.com/example/tool",
         ("example", "tool")),
        ("https://github.com/example/tool/releases/tag/v1.0", "https://github.com/example/tool",
         ("example", "tool")),
        ("http://example.com:80/a?b=1", "http://example.com/a?b=1", None),
        ("https://example.com:443/a", "https://example.com/a", None),
        ("https://example.com/path/?utm_source=x&utm_medium=y&id=1",
         "https://example.com/path?id=1", None),
        ("https://example.com/path/#frag", "https://example.com/path", None),
        ("https://www.github.com/e/r", "https://github.com/e/r", ("e", "r")),
        ("//example.com/a", "https://example.com/a", None),
        ("ftp://example.com/a", None, None),       # 应跳过
        ("contributing.md", None, None),           # 应跳过
        ("https://twitter.com/user", None, None),  # 应跳过
    ]
    for raw, want_canon, want_gh in cases:
        canon, gh, reason = eri.normalize_url(raw)
        check(f"normalize {raw[:55]}", canon == want_canon and gh == want_gh,
              f"got canon={canon} gh={gh} reason={reason}")
    canon, _, _ = eri.normalize_url("https://example.com/?fbclid=z")
    check("fbclid stripped", canon == "https://example.com", canon)
    canon, _, _ = eri.normalize_url("https://github.com/ziadoz/awesome-php#security")
    check("anchor dropped in canonical", canon == "https://github.com/ziadoz/awesome-php", canon)


# ── 2. 解析 ──────────────────────────────────────────────────────────────────
def test_parser():
    print("[2] Markdown parsing")
    text = open(os.path.join(FIXTURES, "readme-basic.md"), encoding="utf-8").read()
    out = eri.parse_readme(text, {"source_id": "test", "source_repository": "test/repo",
                                  "source_commit": "aaa"})
    recs, skipped = out["records"], out["skipped"]
    check("record count = 8", len(recs) == 8, f"got {len(recs)}")
    check("skipped = 4 (social/relative/self x2)", len(skipped) == 4,
          json.dumps(skipped, ensure_ascii=False))
    names = [r["resource_name"] for r in recs]
    check("bullet row parsed", "Delta Tool" in names, names)
    check("section from heading", all(r["source_section"] in ("Tools", "More Tools")
                                      for r in recs))
    check("positions are line numbers", all(r["source_position"] > 0 for r in recs))
    gamma = next(r for r in recs if r["resource_name"] == "Gamma Site")
    check("gamma canonical", gamma["resource_url"] == "https://example.com/path?id=1",
          gamma["resource_url"])
    check("gamma type website", gamma["resource_type"] == "website", gamma["resource_type"])
    alpha = next(r for r in recs if r["resource_name"] == "Alpha Tool")
    check("alpha type github_repo", alpha["resource_type"] == "github_repo")
    check("raw url preserved", alpha["resource_url_raw"]
          == "https://github.com/example/alpha-tool")


# ── 3. 批内去重 ──────────────────────────────────────────────────────────────
def test_dedup_batch():
    print("[3] in-batch dedup (D1-D4)")
    text = open(os.path.join(FIXTURES, "readme-basic.md"), encoding="utf-8").read()
    out = eri.parse_readme(text, {"source_id": "test", "source_repository": "t/r",
                                  "source_commit": "aaa"})
    dd = eri.dedup_batch(out["records"])
    check("unique resources = 5", len(dd["order"]) == 5, f"got {len(dd['order'])}")
    alpha = dd["merged"]["gh:example/alpha-tool"]
    check("alpha merged 3 occurrences", len(alpha["occurrences"]) == 3,
          len(alpha["occurrences"]))
    check("cross-section dupes detected", dd["cross_section"] == 2, dd["cross_section"])
    check("name variants recorded",
          dd["name_variants"].get("gh:example/alpha-tool")
          == ["Alpha Tool", "Alpha Tool Alias"], dd["name_variants"])
    beta = dd["merged"]["gh:example/beta-tool"]
    check(".git suffix same identity", len(beta["occurrences"]) == 2, len(beta["occurrences"]))


# ── 4/5/6. 入库幂等、增量、软删除 ─────────────────────────────────────────────
def test_ingest_lifecycle():
    print("[4] ingest first run")
    fresh_db()
    r1 = eri.ingest(os.path.join(FIXTURES, "readme-basic.md"), None, "aaaa1111",
                    source_id="test")
    check("added = 5", r1["added"] == 5, r1["added"])
    check("parsed = 8", r1["parsed"] == 8, r1["parsed"])

    print("[5] ingest idempotency (same commit + same hash => noop)")
    r2 = eri.ingest(os.path.join(FIXTURES, "readme-basic.md"), None, "aaaa1111",
                    source_id="test")
    check("second run noop", r2["status"] == "noop", r2["status"])
    st = eri.stats()
    n_total = sum(x["n"] for x in st["totals"])
    check("no duplicate rows after noop", n_total == 5, n_total)

    print("[6] ingest delta (v2: +1 added, 1 desc updated, 2 removed)")
    r3 = eri.ingest(os.path.join(FIXTURES, "readme-modified.md"), None, "bbbb2222",
                    source_id="test")
    check("added = 1 (echo)", r3["added"] == 1, r3["added"])
    check("updated = 1 (beta desc)", r3["updated"] == 1, r3["updated"])
    check("removed = 2 (anchor, delta)", len(r3["removed"]) == 2, r3["removed"])
    beta = eri.get("gh:example/beta-tool")
    check("beta description updated", "UPDATED" in (beta["description"] or ""),
          beta["description"])
    # v1 的 beta-tool.git 与 v2 的 beta-tool 是不同 source_url，各自保留来源行
    check("beta sources kept across versions", len(beta["sources"]) == 3,
          len(beta["sources"]))
    anchor = eri.get("gh:example/anchor")
    check("anchor soft-deleted UNLISTED", anchor["status"] == "UNLISTED", anchor["status"])
    check("anchor history preserved (sources kept)", len(anchor["sources"]) == 1)

    print("[6b] removed re-listed on rollback (re-ingest v1 as new commit)")
    r4 = eri.ingest(os.path.join(FIXTURES, "readme-basic.md"), None, "cccc3333",
                    source_id="test")
    check("anchor re-INDEXED", eri.get("gh:example/anchor")["status"] == "INDEXED",
          r4["added"])


# ── 7. 检索与预算 ────────────────────────────────────────────────────────────
def test_search():
    print("[7] metadata-layer search")
    hits = eri.search(["alpha"])
    check("alpha found first", len(hits) >= 1 and hits[0]["resource_id"]
          == "gh:example/alpha-tool", [h["resource_id"] for h in hits])
    hits = eri.search(["tool"], limit=2)
    check("limit respected", len(hits) <= 2, len(hits))
    check("empty result ok", eri.search(["nothing-matches-xyz"]) == [])
    full = eri.get("gh:example/alpha-tool")
    check("get returns sources+categories",
          full and len(full["sources"]) >= 2 and len(full["categories"]) >= 1)


# ── 8. 报告 ──────────────────────────────────────────────────────────────────
def test_report():
    print("[8] report/dedup-report smoke")
    md = eri.report()
    check("report has sections", "## More Tools" in md and "## Tools" in md)
    dr = eri.dedup_report()
    check("dedup-report runs", isinstance(dr, dict) and "same_name_different_repo" in dr)


def main():
    tests = [test_normalizer, test_parser, test_dedup_batch,
             test_ingest_lifecycle, test_search, test_report]
    for t in tests:
        try:
            t()
        except Exception as e:  # noqa: BLE001
            import traceback
            traceback.print_exc()
            FAILURES.append((t.__name__, repr(e)))
    print()
    if FAILURES:
        print(f"FAILED: {len(FAILURES)}")
        for name, d in FAILURES:
            print(f"  - {name}: {d}")
        sys.exit(1)
    print("ALL PASS")
    shutil.rmtree(TMP, ignore_errors=True)


if __name__ == "__main__":
    main()
