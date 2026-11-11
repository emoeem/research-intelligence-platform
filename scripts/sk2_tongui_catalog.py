#!/usr/bin/env python3
"""SK-2 tongui 课程派生目录生成器（P6-10，安全知识源计划 §3.3/§4.2）。

输入两个一手来源，输出 catalog/articles.md（knowledge.tongui-course-notes
插件的 catalog_entry 解析入口）：

1. Git 原稿仓库固定 Commit 的工作树（Cyber-Security-Course-Notes），
   逐篇读 渗透测试全套/ 13 章 74 篇的 frontmatter（编号/课程/分类/状态）；
2. 精选站点首页快照 HTML（tongui66.github.io/course-notes/，Hexo 生成页），
   提取 31 条精选文章链接，按链接内编号与 frontmatter 编号配对合并。

合并语义（计划 §4.2）：精选站点与 Git 仓库同文章合并为同一逻辑资源，保留
双 source_ref：canonical_url 用站点 URL（精选 31 篇）或 GitHub blob URL
（未精选 43 篇），Git 路径（正文权威来源，全文需先取得作者许可）与 blob
URL 落在条目描述。只登记元数据：本工具不读取、不输出任何课程正文。

用法：
  python3 scripts/sk2_tongui_catalog.py \
    --repo /path/to/Cyber-Security-Course-Notes \
    --site-index /path/to/course-notes-index.html \
    --out tests/fixtures/sk2/tongui-course-notes/catalog/articles.md

退出码 0 成功；配对不闭环（站点链接无法按编号配到原稿、或仓库文章数不是
74）时非零退出，不产出半成品目录。
"""

from __future__ import annotations

import argparse
import re
import sys
import urllib.parse
from pathlib import Path

REPO_PINNED_COMMIT = "45a12a759e82adec13c3d171a34d1cbeea1a15b3"
BLOB_BASE = (
    "https://github.com/tongui66/Cyber-Security-Course-Notes/blob/master/"
)
SITE_BASE = "https://tongui66.github.io"
EXPECTED_ARTICLES = 74


def parse_frontmatter(text: str) -> dict[str, str]:
    """读文章 frontmatter 键值对（编号/课程/分类/状态/日期/标签）。"""
    m = re.match(r"^---\s*\n(.*?)\n---", text[:1200], re.S)
    pairs: dict[str, str] = {}
    if not m:
        return pairs
    for line in m.group(1).splitlines():
        if ":" in line:
            key, value = line.split(":", 1)
            pairs[key.strip()] = value.strip()
    return pairs


def blob_url(rel_path: str) -> str:
    """GitHub blob URL：CJK 保留原字（本仓 74 篇路径实测只含 CJK/字母数字/
    连字符），仅对 URL 结构敏感字符做最小转义，与 extract_markdown_links
    的首个 `)` 收口语义兼容（括号必须转义，否则链接被提前截断）。"""
    return (
        BLOB_BASE
        + rel_path.replace("%", "%25")
        .replace("(", "%28")
        .replace(")", "%29")
        .replace(" ", "%20")
        .replace("?", "%3F")
        .replace("#", "%23")
    )


def load_articles(repo: Path) -> dict[str, dict[str, str]]:
    """仓库侧：74 篇原稿的 frontmatter 与相对路径（键 = 编号）。"""
    root = repo / "渗透测试全套"
    if not root.is_dir():
        raise SystemExit(f"原稿章节根目录不存在：{root}")
    articles: dict[str, dict[str, str]] = {}
    for path in sorted(root.rglob("*.md")):
        if path.name == "00-课程索引.md":
            continue
        rel = path.relative_to(repo).as_posix()
        fm = parse_frontmatter(path.read_text(encoding="utf-8", errors="replace"))
        number = fm.get("编号", "")
        if not number:
            raise SystemExit(f"文章缺 frontmatter 编号：{rel}")
        if number in articles:
            raise SystemExit(f"frontmatter 编号重复：{number}（{rel}）")
        articles[number] = {
            "title": fm.get("课程", path.stem),
            "chapter": fm.get("分类", ""),
            "status": fm.get("状态", ""),
            "rel": rel,
        }
    return articles


def load_featured(site_index: Path) -> list[tuple[str, str]]:
    """站点侧：精选文章链接（原样保留 percent-encoding）+ 解码文本。"""
    page = site_index.read_text(encoding="utf-8", errors="replace")
    hrefs = sorted(set(re.findall(r'href="(/course-notes/[^"]+/)"', page)))
    hrefs = [h for h in hrefs if h.rstrip("/") != "/course-notes"]
    featured: list[tuple[str, str]] = []
    for href in hrefs:
        decoded = urllib.parse.unquote(href)
        m = re.match(r"/course-notes/[^/]+/(\d+)-", decoded)
        if not m:
            raise SystemExit(f"精选链接无编号前缀，无法配对：{decoded}")
        featured.append((m.group(1), SITE_BASE + href))
    return featured


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--repo", required=True, type=Path, help="原稿仓库工作树根")
    ap.add_argument("--site-index", required=True, type=Path, help="精选站点首页快照 HTML")
    ap.add_argument("--out", required=True, type=Path, help="输出 catalog/articles.md 路径")
    args = ap.parse_args()

    articles = load_articles(args.repo)
    if len(articles) != EXPECTED_ARTICLES:
        raise SystemExit(f"原稿文章数 {len(articles)} != 预期 {EXPECTED_ARTICLES}，拒绝产出")
    featured = load_featured(args.site_index)

    matched: dict[str, str] = {}
    for number, url in featured:
        if number not in articles:
            raise SystemExit(f"精选链接编号 {number} 在原稿 74 篇中不存在：{url}")
        if number in matched:
            raise SystemExit(f"精选链接编号 {number} 重复：{url}")
        matched[number] = url
    if len(matched) != len(featured):
        raise SystemExit("精选配对不闭环，拒绝产出")

    chapters: dict[str, list[str]] = {}
    for number, art in articles.items():
        chapters.setdefault(art["chapter"], []).append(number)

    lines: list[str] = []
    lines.append("# tongui 安全课程派生目录（SK-2）")
    lines.append("")
    lines.append(
        "> 由 scripts/sk2_tongui_catalog.py 生成：输入 = Cyber-Security-Course-Notes "
        f"固定 Commit {REPO_PINNED_COMMIT} 原稿树 + 精选站点首页快照。"
    )
    lines.append("> 只登记章节/标题/URL/路径元数据；课程正文无 LICENSE（UNLICENSED），"
                 "本目录不含任何正文（安全知识源计划 §7 第 2 条）。")
    lines.append("> 精选站点与 Git 仓库同文章合并为同一逻辑资源：精选 31 篇 "
                 "canonical_url 用站点 URL，Git 路径（正文权威来源）与 GitHub blob "
                 "URL 保留在条目描述（双 source_ref）；未精选 43 篇 canonical_url "
                 "用 blob URL。")
    lines.append("")
    lines.append("## 来源主页")
    lines.append("")
    lines.append(
        "- [tongui 安全课程精选站点](https://tongui66.github.io/course-notes/): "
        f"来源主页 · 精选导航 · Git 仓库 tongui66/Cyber-Security-Course-Notes @ "
        f"{REPO_PINNED_COMMIT}"
    )
    lines.append("")
    lines.append(f"## 渗透测试全套（{EXPECTED_ARTICLES} 篇，13 章）")
    lines.append("")

    emitted_featured = 0
    for chapter in sorted(chapters):
        for number in sorted(chapters[chapter], key=int):
            art = articles[number]
            if number in matched:
                url = matched[number]
                featured_flag = "是"
                blob = blob_url(art["rel"])
                desc = (
                    f"章节={art['chapter']} | Git路径={art['rel']} | "
                    f"状态={art['status']} | 精选=是 | Blob={blob}"
                )
                emitted_featured += 1
            else:
                url = blob_url(art["rel"])
                featured_flag = "否"
                desc = (
                    f"章节={art['chapter']} | Git路径={art['rel']} | "
                    f"状态={art['status']} | 精选=否"
                )
            lines.append(f"- [{number} {art['title']}]({url}): {desc}")
    lines.append("")

    if emitted_featured != 31:
        raise SystemExit(f"精选合并数 {emitted_featured} != 31，拒绝产出")

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(
        f"OK: {len(articles)} 篇 + 1 来源主页 = {len(articles) + 1} 条；"
        f"精选合并 {emitted_featured}/31；输出 {args.out}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
