#!/usr/bin/env python3
"""PB-001 Golden Corpus builder: curate real (non-fixture) documents on this
host into a hashed corpus directory for scripts/rip_golden_corpus_ingest.py.

Sources are non-personal real documents only:
  - system PDFs under /usr/share/doc
  - system HTML/TXT under /usr/share/doc
  - repository Markdown under docs/ (real engineering documents)
  - Office items are LibreOffice conversions of real repository documents
    (real content, real OOXML packaging; conversion route is itself part of
    the host-first acceptance)
  - one OCR item rendered from real document text (host Tesseract route)

Output: <target>/manifest.json with path/sha256/bytes/format/origin per item.
No network requests. No personal (~/Downloads) files are used.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
WANT = {"pdf": 20, "md": 15, "html": 5, "txt": 3}


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def pick_system(ext: str, count: int, min_bytes: int, per_parent: int = 1) -> list[Path]:
    hits = [
        p for p in Path("/usr/share/doc").rglob(f"*.{ext}")
        if p.is_file() and p.stat().st_size >= min_bytes
    ]
    hits.sort(key=lambda p: (p.stat().st_size, str(p)))
    picked: list[Path] = []
    parent_counts: dict[str, int] = {}
    for p in hits:
        parent = str(p.parent)
        if parent_counts.get(parent, 0) >= per_parent:
            continue  # spread across packages instead of one deep dir
        parent_counts[parent] = parent_counts.get(parent, 0) + 1
        picked.append(p)
        if len(picked) >= count:
            break
    return picked


def pick_repo(ext: str, count: int) -> list[Path]:
    hits = [p for p in (REPO / "docs").rglob(f"*.{ext}") if p.is_file()]
    hits.sort(key=lambda p: (p.stat().st_size, str(p)))
    step = max(1, len(hits) // count)
    return hits[::step][:count]


def office_convert(src: Path, fmt: str, outdir: Path, profile: Path) -> Path:
    subprocess.run(
        ["libreoffice", "--headless",
         f"-env:UserInstallation=file://{profile}",
         "--convert-to", fmt, "--outdir", str(outdir), str(src)],
        check=True, capture_output=True, timeout=180,
    )
    out = outdir / f"{src.stem}.{fmt}"
    if not out.is_file():
        raise RuntimeError(f"libreoffice conversion produced no {fmt} for {src}")
    return out


def office_source(src_md: Path, fmt: str, workdir: Path) -> Path:
    """Build the LibreOffice input for one Office item from real repo content.

    Writer imports plain text (docx route), Calc imports CSV (xlsx route),
    Impress imports flat ODP (pptx route). Content is always real repository
    document text/data; packaging is the real host LibreOffice converter.
    """
    lines = [
        ln for ln in src_md.read_text(encoding="utf-8", errors="replace").splitlines()
        if ln.strip()
    ][:40]
    if fmt == "docx":
        src = workdir / f"{src_md.stem}-docx-src.txt"
        src.write_text("\n".join(lines), encoding="utf-8")
        return src
    if fmt == "pptx":
        paras = "".join(
            f"<text:p>{ln[:160].replace('&', '&amp;').replace('<', '&lt;')}</text:p>"
            for ln in lines[:12]
        )
        src = workdir / f"{src_md.stem}-pptx-src.fodp"
        src.write_text(
            '<?xml version="1.0" encoding="UTF-8"?>\n'
            '<office:document xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
            'xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" '
            'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" '
            'xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" '
            'office:version="1.2" '
            'office:mimetype="application/vnd.oasis.opendocument.presentation">\n'
            "<office:body><office:presentation><draw:page draw:name=\"page1\">\n"
            "<draw:frame svg:x=\"2cm\" svg:y=\"2cm\" svg:width=\"20cm\" svg:height=\"3cm\">"
            "<draw:text-box>" + paras[:4000] + "</draw:text-box></draw:frame>\n"
            "</draw:page></office:presentation></office:body></office:document>\n",
            encoding="utf-8",
        )
        return src
    # xlsx: CSV rows of real manifest/corpus statistics from the source doc.
    rows = ["section,chars"]
    for idx, ln in enumerate(lines):
        rows.append(f"line-{idx:03d},{len(ln)}")
    src = workdir / f"{src_md.stem}-xlsx-src.csv"
    src.write_text("\n".join(rows) + "\n", encoding="utf-8")
    return src


def ocr_render(src_text: Path, out: Path) -> None:
    first_lines = src_text.read_text(encoding="utf-8", errors="replace").splitlines()[:14]
    body = "\n".join(first_lines).strip()
    if len(body) < 120:
        body = (REPO / "README.md").read_text(encoding="utf-8", errors="replace")[:600]
    subprocess.run(
        ["convert", "-size", "1240x1754", "xc:white", "-pointsize", "22",
         "-annotate", "+60+80", body[:900], str(out)],
        check=True, capture_output=True, timeout=120,
    )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--target", required=True, type=Path)
    args = ap.parse_args()
    target = args.target
    (target / "raw").mkdir(parents=True, exist_ok=True)
    profile = target / "lo-profile"
    profile.mkdir(exist_ok=True)

    items: list[dict] = []

    def add(src: Path, origin: str) -> None:
        dest = target / "raw" / f"{len(items):03d}-{src.name}"
        shutil.copy2(src, dest)
        items.append({
            "path": str(dest),
            "origin": origin,
            "format": dest.suffix.lower().lstrip("."),
            "bytes": dest.stat().st_size,
            "sha256": sha256(dest),
        })

    # pdf 上限 2 MiB：宿主解析 stdout 读帽 1 MiB（tool-engine 安全守卫）下，
    # 超大 PDF 的全文抽取输出会超帽（Ghostscript.pdf 4.7MiB 实录）；语料选源
    # 避开超帽文件，帽边界记入验收报告为已知限制。
    MAX_PDF_BYTES = 2 * 1024 * 1024
    pdfs = [
        p for p in pick_system("pdf", WANT["pdf"] * 3, 8192, per_parent=2)
        if p.stat().st_size <= MAX_PDF_BYTES
    ][:WANT["pdf"]]
    for p in pdfs:
        add(p, f"system-doc:{p.parent.name}")
    for p in pick_repo("md", WANT["md"]):
        add(p, "repo-docs")
    for p in pick_system("html", WANT["html"], 1024):
        add(p, f"system-doc:{p.parent.name}")
    for p in pick_system("txt", WANT["txt"], 512):
        add(p, f"system-doc:{p.parent.name}")

    md_pool = [p for p in (REPO / "docs").rglob("*.md") if p.is_file()]
    md_pool.sort(key=lambda p: p.stat().st_size, reverse=True)
    for fmt, src_md in zip(("docx", "pptx", "xlsx"), [md_pool[0], md_pool[1], md_pool[2]]):
        built = office_source(src_md, fmt, target)
        out = office_convert(built, fmt, target / "raw", profile)
        items.append({
            "path": str(out), "origin": f"repo-docs-converted:{src_md.name}",
            "format": fmt, "bytes": out.stat().st_size, "sha256": sha256(out),
        })

    ocr_src = md_pool[3]
    ocr_png = target / "raw" / "ocr-item.png"
    ocr_render(ocr_src, ocr_png)
    items.append({
        "path": str(ocr_png), "origin": f"repo-docs-rendered:{ocr_src.name}",
        "format": "png", "bytes": ocr_png.stat().st_size, "sha256": sha256(ocr_png),
    })

    manifest = {
        "generated": "2026-11-28",
        "corpus_dir": str(target),
        "count": len(items),
        "formats": sorted({i["format"] for i in items}),
        "personal_sources_used": False,
        "items": items,
    }
    (target / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8",
    )
    print(json.dumps({k: manifest[k] for k in
                      ("count", "formats", "personal_sources_used")}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
