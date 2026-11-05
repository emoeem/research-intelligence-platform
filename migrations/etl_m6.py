#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
M6 ETL：Tool & Workflow Platform 数据面（隔离环境 dry-run，不切流）
==================================================================

版本：1.0.0（沿用 M1_IMPLEMENTATION_PLAN 约定：脚本带版本号）

职责：
  Extract（全部只读）：
    - /home/emo/.dsh/redteam/rtis/data/registry/tools.json（20 工具声明）
    - /home/emo/.dsh/redteam/rtis/data/registry/workflows.yaml（受限解析器解析）
    - /home/emo/.dsh/redteam/skills/*.md（125 个文件，只读抽取 front matter）
  Transform：
    - 工具健康实测：对 20 个工具逐个执行白名单内的只读探测命令
      （--version / -h / node --check / sqlite quick_check，timeout 5s，
        绝不执行扫描、不触网扫描、不写任何源），得到 install_state /
      health_state / 实测 version / 探测证据；与 registry 声明对比生成 drift 清单。
    - 落实 SKILLS_AND_TOOLS_REVIEW 已知缺陷修正：
        fscan version="flag needs an argument: -h"（坏探测值）→ version 置 NULL；
        suo5  version="NAME:"（坏探测值）→ 置 NULL，实测补 v2.2.0；
        rtis.py version="1"（硬编码）→ 置 NULL；
        gogo  installed=false 与 toolkit 二进制实存不符 → 按 ls 实存重判；
        frpc  版本漂移以实测为准（registry 0.71.1 → 实测 0.72.0）。
      健康与安装分离：install_state（定义/安装）与 health_state（健康）、
      permission_level（授权）、schema（定义，M6 后期）四态独立落列。
    - workflow：受限 YAML 子集解析（缩进映射/块列表/流式列表/流式映射/标量），
      解析失败条目入隔离清单（不猜测、不导入）。
    - skill：front matter 抽取（name/roles/description/when_to_use/enabled/
      generator/body_ref），缺失字段标 NULL；不修改任何 skill 文件。
  Load：只写隔离目标库 platform-dryrun.db：
    tool / tool_version / workflow / workflow_registry_meta / skill / entity_alias
    / etl_run_log（库内运行日志）；tool_execution 仅空结构（备 M5）。
  Replay：内容行全部使用源侧确定性时间戳（registry generated_at / 文件 mtime），
    已存在且内容不变的行跳过写入；重复执行 no-op（内容摘要一致）。
  Rollback：
    DELETE FROM entity_alias WHERE source_system IN
      ('rtis-registry','rtis-workflows','rtis-skills');
    DELETE FROM tool_version; DELETE FROM tool; DELETE FROM workflow;
    DELETE FROM workflow_registry_meta;
    DELETE FROM skill WHERE source_system='rtis-skills';
    或删除 platform-dryrun.db 整体回滚。

硬约束：
  - registry 源文件与 skill 文件只读；绝不写 knowledge.db / catalog.db / assets.db；
  - stdlib-only；
  - 探测命令白名单见 PROBE_SPECS，全部为版本/帮助/语法/完整性类只读探测；
  - 运行时刻信息（探测时刻、运行时刻）只进 etl_run_log 与 stdout，不进内容行。
"""

import hashlib
import json
import re
import shutil
import sqlite3
import subprocess
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

ETL_VERSION = "1.0.0"

MIG_DIR = Path(__file__).resolve().parent
TARGET_DB = MIG_DIR / "platform-dryrun.db"
SCHEMA_003_SQL = MIG_DIR / "003_tools_workflow.sql"

SRC_TOOLS_JSON = Path("/home/emo/.dsh/redteam/rtis/data/registry/tools.json")
SRC_WORKFLOWS_YAML = Path("/home/emo/.dsh/redteam/rtis/data/registry/workflows.yaml")
SRC_SKILLS_DIR = Path("/home/emo/.dsh/redteam/skills")

TOOLKIT_DIR = Path("/home/emo/.dsh/redteam/toolkit")
RTIS_PY = Path("/home/emo/.dsh/redteam/rtis/rtis.py")
RTIS_KB_MJS = Path("/home/emo/.dsh/redteam/rtis/rtis_kb.mjs")
RTIS_MCP_MJS = Path("/home/emo/.dsh/redteam/scripts/rtis_mcp.mjs")
KNOWLEDGE_DB = Path("/home/emo/.dsh/redteam/knowledge.db")
PD_HTTPX = Path("/home/emo/.local/bin/pd-httpx")

SCHEMA_VERSION = 1
PROBE_TIMEOUT_S = 5
PROBE_HEAD_CHARS = 240

SOURCE_SYSTEM_TOOLS = "rtis-registry"
SOURCE_SYSTEM_WORKFLOWS = "rtis-workflows"
SOURCE_SYSTEM_SKILLS = "rtis-skills"
ALIAS_NS_TOOL_NAME = "rtis.data.registry.tools.json.name"
ALIAS_NS_WORKFLOW_ID = "rtis.data.registry.workflows.yaml.id"
ALIAS_NS_SKILL_NAME = "rtis.skills.frontmatter.name"

# 本脚本职责域内容表（content digest 覆盖范围）
CONTENT_TABLES = {
    "tool": "tool_id",
    "tool_version": "tv_id",
    "skill": "skill_id",
    "workflow": "workflow_id",
    "workflow_registry_meta": "id",
}
M6_ALIAS_DOMAINS = (SOURCE_SYSTEM_TOOLS, SOURCE_SYSTEM_WORKFLOWS, SOURCE_SYSTEM_SKILLS)

ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")

# ---------------------------------------------------------------------------
# 探测规范（白名单：只读探测，绝不执行扫描）
#   type=cmd    ：子进程执行 argv，version 从 ver_re 或首个非空行提取
#                 （first_line_version=False 的工具禁止首行提取：其首行是
#                  用法/报错文本，不是版本）
#   type=syntax ：node --check 语法校验（MCP 桥 / mjs 入口）
#   type=sqlite ：只读连接 + PRAGMA quick_check 完整性探测
# ---------------------------------------------------------------------------
PROBE_SPECS = {
    "nmap": {"type": "cmd", "argv": ["nmap", "--version"],
             "ver_re": r"Nmap version\s+(\S+)"},
    "nuclei": {"type": "cmd", "argv": ["nuclei", "--version"],
               "ver_re": r"Engine Version:\s*(\S+)"},
    "fscan": {"type": "cmd", "argv": [str(TOOLKIT_DIR / "fscan" / "fscan"), "-h"],
              "ver_re": None, "first_line_version": False,
              "note": "无版本 flag（-h 返回 usage、-V 未定义）；registry version 字段为坏探测值，置 NULL"},
    "gogo": {"type": "cmd", "argv": [str(TOOLKIT_DIR / "gogo" / "gogo"), "--version"],
             "ver_re": r"version\s+(\S+)", "first_line_version": False,
             "note": "二进制实存（toolkit/gogo/gogo）；--version exit=0 但输出为空："
                     "源码本地构建未注入版本号（评审 P0 备忘关联）"},
    "suo5": {"type": "cmd", "argv": [str(TOOLKIT_DIR / "suo5" / "suo5-linux-amd64"), "--version"],
             "ver_re": r"version\s+(\S+)"},
    "frpc": {"type": "cmd", "argv": [str(TOOLKIT_DIR / "frp" / "frpc"), "-v"],
             "ver_re": None, "first_line_version": True},
    "chisel": {"type": "cmd", "argv": [str(TOOLKIT_DIR / "chisel" / "chisel"), "--version"],
               "ver_re": None, "first_line_version": True},
    "ffuf": {"type": "cmd", "argv": ["ffuf", "-V"],
             "ver_re": r"ffuf version:\s*(\S+)"},
    "sqlmap": {"type": "cmd", "argv": ["sqlmap", "--version"],
               "ver_re": None, "first_line_version": True},
    "hashcat": {"type": "cmd", "argv": ["hashcat", "--version"],
                "ver_re": None, "first_line_version": True},
    "subfinder": {"type": "cmd", "argv": [str(TOOLKIT_DIR / "subfinder" / "subfinder"), "-version"],
                  "ver_re": r"Current Version:\s*(\S+)"},
    "pd-httpx": {"type": "cmd", "argv": [str(PD_HTTPX), "-version"],
                 "ver_re": None, "first_line_version": False,
                 "note": "声明路径 ~/.local/bin/pd-httpx 不存在；registry installed=false 与实况一致。"
                         "~/.local/bin/httpx（Go ELF）与 httpx2 存在但名称与 registry 声明不符，未擅自等同"},
    "frida": {"type": "cmd", "argv": ["frida", "--version"],
              "ver_re": None, "first_line_version": True},
    "jadx": {"type": "cmd", "argv": ["jadx", "--version"],
             "ver_re": None, "first_line_version": True},
    "node": {"type": "cmd", "argv": ["node", "--version"],
             "ver_re": None, "first_line_version": True},
    "python3": {"type": "cmd", "argv": ["python3", "--version"],
                "ver_re": r"Python\s+(\S+)"},
    "rtis_kb（node 桥）": {"type": "syntax", "file": str(RTIS_KB_MJS),
                       "note": "node --check 语法校验通过；MCP 桥无版本声明，version 置 NULL"},
    "rtis.py": {"type": "cmd", "argv": ["python3", str(RTIS_PY), "--version"],
                "ver_re": None, "first_line_version": False,
                "note": "无版本 flag（--version 走 argparse 报错退出）；registry version='1' 为硬编码，置 NULL"},
    "knowledge.db": {"type": "sqlite", "file": str(KNOWLEDGE_DB),
                     "note": "PRAGMA quick_check（只读连接）；库本体无版本声明，version 置 NULL"},
    "rtis_mcp.mjs": {"type": "syntax", "file": str(RTIS_MCP_MJS),
                     "note": "node --check 语法校验通过；MCP 入口无版本声明，version 置 NULL"},
}

# registry 坏 version 值的签名（缺陷在场才判 corrupt，值被修复后按正常值处理）
CORRUPT_DECLARED_SIGNATURES = {
    "fscan": (lambda v: "flag needs" in v, "registry version 存的是探测报错首行（'flag needs an argument: -h'）"),
    "suo5": (lambda v: v.startswith("NAME:"), "registry version 存的是帮助输出首行（'NAME:'），探测命令选择错误"),
    "rtis.py": (lambda v: v == "1", "registry version 为硬编码 '1'，无探测依据"),
}


class YamlSubsetError(Exception):
    """受限 YAML 子集解析失败。"""


# ---------------------------------------------------------------------------
# 通用工具
# ---------------------------------------------------------------------------
def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def mtime_iso(path: Path) -> str:
    return datetime.fromtimestamp(path.stat().st_mtime, tz=timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%SZ")


def iso_to_utc_z(s: str) -> str:
    """registry generated_at（含时区）规范为 UTC Z 后缀；解析失败原样返回。"""
    try:
        return datetime.fromisoformat(s).astimezone(timezone.utc).strftime(
            "%Y-%m-%dT%H:%M:%SZ")
    except ValueError:
        return s


def strip_ansi(s: str) -> str:
    return ANSI_RE.sub("", s or "")


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def table_digest(conn: sqlite3.Connection, table: str, pk: str,
                 where: str = "", params: tuple = ()) -> str:
    """逻辑内容摘要：全行按稳定主键排序后规范化哈希（与 etl_tasks.py 同算法）。"""
    sql = f"SELECT * FROM {table}{(' WHERE ' + where) if where else ''} ORDER BY {pk}"
    h = hashlib.sha256()
    cur = conn.execute(sql, params)  # noqa: S608（表名为内部常量）
    cols = [d[0] for d in cur.description]
    for row in cur:
        vals = ["" if v is None else repr(v) for v in row]
        h.update(("\x1f".join(cols) + "\x1e" + "\x1f".join(vals) + "\x1e").encode("utf-8"))
    return h.hexdigest()


def alias_key(entity_type: str, source_system: str, namespace: str, alias_value: str) -> str:
    raw = f"{entity_type}|{source_system}|{namespace}|{alias_value}"
    return hashlib.md5(raw.encode("utf-8")).hexdigest()  # noqa: S324（非安全用途，仅确定性键）


def register_alias(cur: sqlite3.Cursor, entity_type: str, entity_id: str,
                   source_system: str, namespace: str, alias_value: str,
                   stats: Counter) -> None:
    """登记别名；同键不同实体 = 冲突，红线：不覆盖、不静默，直接失败。"""
    key = alias_key(entity_type, source_system, namespace, alias_value)
    existing = cur.execute(
        "SELECT entity_id FROM entity_alias WHERE id = ?", (key,)).fetchone()
    if existing is not None and existing["entity_id"] != entity_id:
        raise SystemExit(
            f"FATAL: Alias 冲突：{source_system}/{namespace}/{alias_value} "
            f"已映射到 {existing['entity_id']}，又尝试映射到 {entity_id}")
    if existing is not None:
        stats["alias_skipped_existing"] += 1
        return
    cur.execute(
        "INSERT INTO entity_alias (id, entity_type, entity_id, source_system, namespace,"
        " alias_value, valid_from, valid_until, created_at, updated_at, schema_version)"
        " VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        (key, entity_type, entity_id, source_system, namespace, alias_value,
         None, None, now_iso(), now_iso(), SCHEMA_VERSION),
    )
    stats["alias_inserted"] += 1


def open_target() -> sqlite3.Connection:
    conn = sqlite3.connect(str(TARGET_DB))
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON;")
    return conn


# ---------------------------------------------------------------------------
# 守卫 DDL（SQLite 无条件 DDL，只能脚本侧判定后执行；003_tools_workflow.sql 为权威记录）
# ---------------------------------------------------------------------------
def apply_003_schema(conn: sqlite3.Connection) -> None:
    conn.executescript(SCHEMA_003_SQL.read_text(encoding="utf-8"))
    conn.execute("PRAGMA foreign_keys = ON;")


def ensure_tool_columns(conn: sqlite3.Connection) -> list:
    """tool 表追加 purpose / provenance_ref / health_note 列（幂等守卫）。"""
    added = []
    cols = {r[1] for r in conn.execute("PRAGMA table_info(tool)")}
    for col in ("purpose", "provenance_ref", "health_note"):
        if col not in cols:
            conn.execute(f"ALTER TABLE tool ADD COLUMN {col} TEXT")  # noqa: S608（内部常量列名）
            added.append(col)
    return added


def ensure_tool_version_schema(conn: sqlite3.Connection) -> bool:
    """tool_version 重建：version 放宽 NULL-able + health 列（幂等守卫）。

    重建时若表有数据则全量拷贝（本批导入前为空表，无损失）。
    """
    sql = conn.execute(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='tool_version'"
    ).fetchone()
    if sql is None:
        raise SystemExit("FATAL: tool_version 表不存在（001_init.sql 未应用？）")
    create_sql = sql[0] or ""
    cols = {r[1] for r in conn.execute("PRAGMA table_info(tool_version)")}
    needs = (re.search(r"version\s+TEXT\s+NOT\s+NULL", create_sql) is not None
             or "health" not in cols)
    if not needs:
        return False
    conn.executescript("""
      DROP TABLE IF EXISTS tool_version_v2;
      CREATE TABLE tool_version_v2 (
        tv_id          INTEGER PRIMARY KEY AUTOINCREMENT,
        tool_id        TEXT NOT NULL REFERENCES tool(tool_id),
        version        TEXT,
        health         TEXT CHECK (health IN ('unknown','ok','degraded','broken')
                                  OR health IS NULL),
        provenance_json TEXT,
        content_hash   TEXT,
        validated_at   TEXT,
        status         TEXT CHECK (status IN ('current','superseded') OR status IS NULL),
        created_at     TEXT NOT NULL,
        updated_at     TEXT NOT NULL,
        schema_version INTEGER NOT NULL DEFAULT 1,
        UNIQUE (tool_id, version)
      );
      INSERT INTO tool_version_v2
        (tv_id, tool_id, version, health, provenance_json, content_hash,
         validated_at, status, created_at, updated_at, schema_version)
        SELECT tv_id, tool_id, version, NULL, provenance_json, content_hash,
               validated_at, status, created_at, updated_at, schema_version
          FROM tool_version;
      DROP TABLE tool_version;
      ALTER TABLE tool_version_v2 RENAME TO tool_version;
      CREATE INDEX IF NOT EXISTS ix_tool_version_tool ON tool_version(tool_id);
    """)
    conn.execute("PRAGMA foreign_keys = ON;")
    return True


def ensure_entity_alias_workflow_domain(conn: sqlite3.Connection) -> bool:
    """entity_alias.entity_type CHECK 域扩展：加入 'workflow'（同 002 重建模式）。"""
    sql = conn.execute(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='entity_alias'"
    ).fetchone()[0]
    if "'workflow'" in sql:
        return False
    conn.executescript("""
      DROP TABLE IF EXISTS entity_alias_v3;
      CREATE TABLE entity_alias_v3 (
        id            TEXT PRIMARY KEY,
        entity_type   TEXT NOT NULL CHECK (entity_type IN
                        ('resource','resource_source','knowledge','evidence',
                         'tool','tool_version','skill','review','research_task',
                         'workflow')),
        entity_id     TEXT NOT NULL,
        source_system TEXT NOT NULL,
        namespace     TEXT NOT NULL,
        alias_value   TEXT NOT NULL,
        valid_from    TEXT,
        valid_until   TEXT,
        created_at    TEXT NOT NULL,
        updated_at    TEXT NOT NULL,
        schema_version INTEGER NOT NULL DEFAULT 1,
        UNIQUE (source_system, namespace, alias_value, entity_type)
      );
      INSERT INTO entity_alias_v3
        SELECT id, entity_type, entity_id, source_system, namespace, alias_value,
               valid_from, valid_until, created_at, updated_at, schema_version
          FROM entity_alias;
      DROP TABLE entity_alias;
      ALTER TABLE entity_alias_v3 RENAME TO entity_alias;
      CREATE INDEX IF NOT EXISTS ix_alias_entity ON entity_alias(entity_type, entity_id);
    """)
    conn.execute("PRAGMA foreign_keys = ON;")
    return True


# ---------------------------------------------------------------------------
# 受限 YAML 子集解析器（仅覆盖 workflows.yaml 用到的语法面）
# ---------------------------------------------------------------------------
def _yaml_scalar(s: str):
    s = s.strip()
    if s == "":
        return None
    if s == "[]":
        return []
    if s == "{}":
        return {}
    if len(s) >= 2 and s[0] == s[-1] and s[0] in "\"'":
        return s[1:-1]
    low = s.lower()
    if low == "true":
        return True
    if low == "false":
        return False
    if low in ("null", "~"):
        return None
    if re.fullmatch(r"-?\d+", s):
        return int(s)
    if s.startswith("[") and s.endswith("]"):
        return _yaml_flow_seq(s[1:-1])
    if s.startswith("{") and s.endswith("}"):
        return _yaml_flow_map(s[1:-1])
    return s


def _yaml_split_flow(s: str) -> list:
    parts, depth, cur, quote = [], 0, [], None
    for ch in s:
        if quote:
            cur.append(ch)
            if ch == quote:
                quote = None
            continue
        if ch in "\"'":
            quote = ch
            cur.append(ch)
            continue
        if ch in "[{":
            depth += 1
        if ch in "]}":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    tail = "".join(cur)
    if tail.strip():
        parts.append(tail)
    elif parts and tail.strip() == "" and s.strip().endswith(","):
        raise YamlSubsetError(f"流式集合存在尾逗号，不支持：{s!r}")
    return parts


def _yaml_flow_seq(inner: str) -> list:
    return [_yaml_scalar(p) for p in _yaml_split_flow(inner) if p.strip() != ""]


def _yaml_flow_map(inner: str) -> dict:
    result = {}
    for part in _yaml_split_flow(inner):
        if ":" not in part:
            raise YamlSubsetError(f"流式映射项缺少冒号：{part!r}")
        k, _, v = part.partition(":")
        key = _yaml_scalar(k)
        if not isinstance(key, str):
            raise YamlSubsetError(f"流式映射键必须是标量：{part!r}")
        result[key] = _yaml_scalar(v)
    return result


def _yaml_indent(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def _yaml_split_key_val(s: str):
    """映射行拆 key/value：首个不在引号/括号内的冒号。"""
    depth, quote = 0, None
    for i, ch in enumerate(s):
        if quote:
            if ch == quote:
                quote = None
            continue
        if ch in "\"'":
            quote = ch
            continue
        if ch in "[{":
            depth += 1
        if ch in "]}":
            depth -= 1
        if ch == ":" and depth == 0:
            return s[:i].strip(), s[i + 1:].strip()
    raise YamlSubsetError(f"映射行缺少冒号：{s!r}")


def _yaml_parse_block(lines: list, i: int, indent: int):
    if i >= len(lines):
        return None, i
    s = lines[i].lstrip()
    if s.startswith("- ") or s == "-":
        return _yaml_parse_list(lines, i, indent)
    return _yaml_parse_map(lines, i, indent)


def _yaml_next_significant(lines: list, i: int):
    while i < len(lines) and lines[i].strip() == "":
        i += 1
    return i


def _yaml_parse_map(lines: list, i: int, indent: int):
    result = {}
    while i < len(lines):
        if lines[i].strip() == "":
            i += 1
            continue
        ind = _yaml_indent(lines[i])
        if ind < indent:
            break
        if ind > indent:
            raise YamlSubsetError(f"意外的缩进（期望 {indent}，得到 {ind}）：{lines[i]!r}")
        s = lines[i].strip()
        if s.startswith("- ") or s == "-":
            break
        key, val = _yaml_split_key_val(s)
        if key in result:
            raise YamlSubsetError(f"重复键：{key!r}")
        if val == "":
            j = _yaml_next_significant(lines, i + 1)
            if j < len(lines) and _yaml_indent(lines[j]) > indent:
                child, i = _yaml_parse_block(lines, j, _yaml_indent(lines[j]))
                result[key] = child
            elif (j < len(lines) and _yaml_indent(lines[j]) == indent
                  and (lines[j].lstrip().startswith("- ") or lines[j].strip() == "-")):
                child, i = _yaml_parse_list(lines, j, indent)
                result[key] = child
            else:
                result[key] = None
                i += 1
        else:
            result[key] = _yaml_scalar(val)
            i += 1
    return result, i


def _yaml_parse_list(lines: list, i: int, indent: int):
    items = []
    while i < len(lines):
        if lines[i].strip() == "":
            i += 1
            continue
        ind = _yaml_indent(lines[i])
        if ind < indent:
            break
        if ind > indent:
            raise YamlSubsetError(f"意外的缩进（期望 {indent}，得到 {ind}）：{lines[i]!r}")
        s = lines[i].strip()
        if not (s.startswith("- ") or s == "-"):
            break
        if s == "-":
            j = _yaml_next_significant(lines, i + 1)
            if j < len(lines) and _yaml_indent(lines[j]) > indent:
                child, i = _yaml_parse_block(lines, j, _yaml_indent(lines[j]))
                items.append(child)
            else:
                items.append(None)
                i += 1
            continue
        content = s[2:].strip()
        if content.startswith("{") and content.endswith("}"):
            items.append(_yaml_flow_map(content[1:-1]))
            i += 1
        elif content.startswith("[") and content.endswith("]"):
            items.append(_yaml_flow_seq(content[1:-1]))
            i += 1
        elif ":" in content:
            # "- id: WF-00" 形式：把 '- ' 前缀改写为空格后按映射解析
            dash = lines[i].index("-", ind)
            k = dash + 1
            while k < len(lines[i]) and lines[i][k] == " ":
                k += 1
            content_col = k
            lines[i] = " " * content_col + lines[i][k:]
            child, i = _yaml_parse_map(lines, i, content_col)
            items.append(child)
        else:
            items.append(_yaml_scalar(content))
            i += 1
    return items, i


def parse_yaml_subset(text: str):
    """受限 YAML 解析入口。仅支持：整行注释、顶层/嵌套映射、块列表、
    流式列表/映射、标量。多行标量/锚点/多文档不支持（出现即抛错）。"""
    lines = []
    for raw in text.splitlines():
        if raw.lstrip().startswith("#"):
            continue
        if "\t" in raw:
            raise YamlSubsetError(f"含制表符缩进，不支持：{raw!r}")
        lines.append(raw.rstrip())
    obj, i = _yaml_parse_map(lines, 0, 0)
    i = _yaml_next_significant(lines, i)
    if i < len(lines):
        raise YamlSubsetError(f"存在未消费的顶层内容：{lines[i]!r}")
    return obj


# ---------------------------------------------------------------------------
# 工具健康实测
# ---------------------------------------------------------------------------
def probe_tool(name: str, spec: dict) -> dict:
    """对单个工具执行只读探测，返回确定性证据字典（不含探测时刻）。"""
    ev = {"tool": name, "probe_type": spec["type"]}
    note = spec.get("note")

    if spec["type"] in ("syntax", "sqlite"):
        path = Path(spec["file"])
        ev["checked_path"] = str(path)
        if not path.exists():
            ev.update(install_state="missing", health="broken", version=None,
                      exit_code=None, output_head=None,
                      note=(note or "") + "；文件缺失" if note else "文件缺失")
            return ev
        ev["install_state"] = "installed"
        if spec["type"] == "syntax":
            node = shutil.which("node")
            if node is None:
                ev.update(health="unknown", version=None, exit_code=None,
                          output_head=None, note=(note or "") + "；node 不在 PATH，无法语法校验")
                return ev
            argv = [node, "--check", str(path)]
            ev["argv"] = argv
            try:
                proc = subprocess.run(argv, capture_output=True, text=True,
                                      timeout=PROBE_TIMEOUT_S)  # noqa: S603
            except subprocess.TimeoutExpired:
                ev.update(health="broken", version=None, exit_code=None,
                          output_head=None,
                          note=(note or "") + f"；探测超时（>{PROBE_TIMEOUT_S}s）")
                return ev
            head = strip_ansi(((proc.stdout or "") + (proc.stderr or "")).strip())
            ev.update(exit_code=proc.returncode,
                      output_head=head[:PROBE_HEAD_CHARS] or None)
            if proc.returncode == 0:
                ev.update(health="ok", version=None, note=note)
            else:
                ev.update(health="broken", version=None,
                          note=(note or "") + "；node --check 语法校验失败")
            return ev
        # sqlite quick_check（只读连接）
        ev["argv"] = ["sqlite3(只读连接)", f"PRAGMA quick_check @ {path}"]
        try:
            conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
            try:
                rc = conn.execute("PRAGMA quick_check").fetchone()[0]
            finally:
                conn.close()
        except sqlite3.Error as e:
            ev.update(health="broken", version=None, exit_code=None,
                      output_head=repr(e)[:PROBE_HEAD_CHARS],
                      note=(note or "") + "；只读连接/quick_check 失败")
            return ev
        ev.update(exit_code=0, output_head=rc)
        if rc == "ok":
            ev.update(health="ok", version=None,
                      note=(note or "") + f"；quick_check=ok（sqlite_lib={sqlite3.sqlite_version}）")
        else:
            ev.update(health="broken", version=None,
                      note=(note or "") + f"；quick_check={rc}")
        return ev

    # type=cmd
    argv = list(spec["argv"])
    ev["argv"] = argv
    binary = argv[0]
    if "/" in binary:
        bin_path = Path(binary)
        bin_exists = bin_path.exists()
        bin_ref = str(bin_path)
    else:
        resolved = shutil.which(binary)
        bin_exists = resolved is not None
        bin_ref = resolved or binary
    ev["binary_ref"] = bin_ref
    if not bin_exists:
        ev.update(install_state="missing", health="unknown", version=None,
                  exit_code=None, output_head=None,
                  note=(note or "") + "；二进制/文件缺失，无法探测")
        return ev
    ev["install_state"] = "installed"
    try:
        proc = subprocess.run(argv, capture_output=True, text=True,
                              timeout=PROBE_TIMEOUT_S)  # noqa: S603
    except subprocess.TimeoutExpired:
        ev.update(health="broken", version=None, exit_code=None, output_head=None,
                  note=(note or "") + f"；探测超时（>{PROBE_TIMEOUT_S}s）")
        return ev
    except OSError as e:
        ev.update(health="broken", version=None, exit_code=None,
                  output_head=repr(e)[:PROBE_HEAD_CHARS],
                  note=(note or "") + "；探测执行失败（OSError）")
        return ev

    combined = strip_ansi((proc.stdout or "") + "\n" + (proc.stderr or ""))
    head = combined.strip()
    ev.update(exit_code=proc.returncode,
              output_head=head[:PROBE_HEAD_CHARS] or None)

    version = None
    ver_re = spec.get("ver_re")
    if ver_re:
        m = re.search(ver_re, combined)
        if m:
            version = m.group(1).strip()
    elif spec.get("first_line_version"):
        for ln in head.splitlines():
            ln = ln.strip()
            if ln:
                version = ln
                break

    if version:
        ev.update(health="ok", version=version, note=note)
    elif proc.returncode == 0 and not head:
        ev.update(health="degraded", version=None,
                  note=(note or "") + "；--version exit=0 但输出为空，版本不可得")
    elif head:
        ev.update(health="degraded", version=None,
                  note=(note or "") + "；二进制可执行但版本不可解析（输出为用法/报错文本）")
    else:
        ev.update(health="broken", version=None,
                  note=(note or "") + f"；探测失败（exit={proc.returncode}，无输出）")
    return ev


def classify_declared_version(name: str, raw) -> tuple:
    """registry 声明 version 清洗：(clean, status, reason)。
    status: 'ok' | 'corrupt'（坏探测值，置 NULL）| 'empty'（未声明）。"""
    if raw is None:
        return None, "empty", None
    clean = strip_ansi(str(raw)).strip()
    if clean == "":
        return None, "empty", None
    sig = CORRUPT_DECLARED_SIGNATURES.get(name)
    if sig and sig[0](clean):
        return None, "corrupt", sig[1]
    # 通用启发式：报错文本/超长值不可信
    if "flag needs an argument" in clean or "Traceback" in clean or len(clean) > 120:
        return None, "corrupt", "registry version 含报错文本或超长，判为坏探测值"
    return clean, "ok", None


def versions_equivalent(measured, declared_clean) -> bool:
    """版本等价判定：相等或声明串包含实测 token（如 nmap 'Nmap 7.993 (…)' ⊇ '7.993'）。"""
    if not measured or not declared_clean:
        return False
    return measured == declared_clean or measured in declared_clean


# ---------------------------------------------------------------------------
# Tools ETL
# ---------------------------------------------------------------------------
def etl_tools(conn: sqlite3.Connection, stats: Counter) -> dict:
    if not SRC_TOOLS_JSON.exists():
        raise SystemExit(f"FATAL: 源不存在：{SRC_TOOLS_JSON}")
    doc = json.loads(SRC_TOOLS_JSON.read_text(encoding="utf-8"))
    items = doc.get("items")
    if not isinstance(items, list):
        raise SystemExit("FATAL: tools.json 结构异常（无 items 数组）")
    generated_at = iso_to_utc_z(doc.get("generated_at") or "")
    if not generated_at:
        raise SystemExit("FATAL: tools.json 无 generated_at（无法取确定性时间戳）")

    quarantine, drift, probes = [], [], {}
    health_dist = Counter()
    imported = 0

    for idx, item in enumerate(items):
        name = (item.get("name") or "").strip()
        if not name:
            quarantine.append({"index": idx, "name": None,
                               "reason": "registry 条目缺 name"})
            stats["tool_quarantined"] += 1
            continue
        spec = PROBE_SPECS.get(name)
        if spec is None:
            quarantine.append({"index": idx, "name": name,
                               "reason": "registry 工具无探测规范（PROBE_SPECS 缺失），不猜测"})
            stats["tool_quarantined"] += 1
            continue

        ev = probe_tool(name, spec)
        probes[name] = ev

        declared_raw = item.get("version")
        declared_clean, decl_status, decl_reason = classify_declared_version(name, declared_raw)
        measured = ev.get("version")
        install_state = ev["install_state"]
        health = ev["health"]
        health_dist[health] += 1

        # ---- drift 判定 ----
        if declared_raw is not None and ANSI_RE.search(str(declared_raw)):
            drift.append({"tool": name, "field": "version_format", "kind": "format",
                          "declared": str(declared_raw), "measured": measured,
                          "disposition": "声明值含 ANSI 转义污染；清洗后"
                          + ("与实测一致" if versions_equivalent(measured, declared_clean)
                             else "与实测比对见 version 行")})
        if decl_status == "corrupt":
            drift.append({"tool": name, "field": "version", "kind": "value-corrupt",
                          "declared": str(declared_raw), "measured": measured,
                          "disposition": decl_reason + "；已置 NULL，以实测为准"})
        elif (decl_status == "empty" and measured is None
              and item.get("kind") == "cli" and install_state == "installed"):
            drift.append({"tool": name, "field": "version", "kind": "unavailable",
                          "declared": "", "measured": None,
                          "disposition": "registry 未声明版本且实测不可得（如实登记，不猜测）；"
                                         "mcp/datastore 类无版本概念，不计入"})
        elif decl_status == "ok" and not versions_equivalent(measured, declared_clean):
            drift.append({"tool": name, "field": "version", "kind": "value-drift",
                          "declared": declared_clean, "measured": measured,
                          "disposition": "registry 声明与实测不一致，以实测为准落库"})
        declared_installed = item.get("installed")
        actual_installed = install_state == "installed"
        if isinstance(declared_installed, bool) and declared_installed != actual_installed:
            drift.append({"tool": name, "field": "installed", "kind": "install-flag",
                          "declared": declared_installed, "measured": actual_installed,
                          "disposition": "registry installed 布尔与实存探测不符，按实测重判"})

        # ---- tool 行（内容确定性；已存在且不变则跳过）----
        tool_id = name  # 现网 name 直接沿用为平台主键（任务书 M6 指定）
        permission_level = "T0" if item.get("readonly") else "T1"
        binary_ref = ev.get("binary_ref") or ev.get("checked_path")
        provenance_ref = f"rtis/data/registry/tools.json#items[name={name}]"
        health_note = ev.get("note") or None
        if decl_status == "corrupt":
            health_note = ((health_note + "；") if health_note else "") + decl_reason
        row = {
            "tool_id": tool_id, "name": name, "kind": item.get("kind"),
            "purpose": item.get("purpose"), "lifecycle": "REGISTERED",
            "governance": "ACTIVE", "install_state": install_state,
            "health_state": health, "health_note": health_note,
            "permission_level": permission_level,
            "input_schema": None, "output_schema": None,  # M6 后期补 JSON Schema
            "timeout_s": None, "binary_ref": binary_ref,
            "provenance_ref": provenance_ref,
            "source_system": SOURCE_SYSTEM_TOOLS, "source_pk": name,
            "updated_at": generated_at, "schema_version": SCHEMA_VERSION,
        }
        existing = conn.execute(
            "SELECT * FROM tool WHERE tool_id=?", (tool_id,)).fetchone()
        if existing is None:
            conn.execute(
                "INSERT INTO tool (tool_id, name, kind, purpose, lifecycle, governance,"
                " install_state, health_state, health_note, permission_level,"
                " input_schema, output_schema, timeout_s, binary_ref, provenance_ref,"
                " source_system, source_pk, created_at, updated_at, schema_version)"
                " VALUES (:tool_id,:name,:kind,:purpose,:lifecycle,:governance,"
                " :install_state,:health_state,:health_note,:permission_level,"
                " :input_schema,:output_schema,:timeout_s,:binary_ref,:provenance_ref,"
                " :source_system,:source_pk,:updated_at,:updated_at,:schema_version)",
                row)
            stats["tool_inserted"] += 1
        else:
            same = all(existing[k] == v for k, v in row.items())
            if same:
                stats["tool_skipped_existing"] += 1
            else:
                conn.execute(
                    "UPDATE tool SET name=:name, kind=:kind, purpose=:purpose,"
                    " lifecycle=:lifecycle, governance=:governance,"
                    " install_state=:install_state, health_state=:health_state,"
                    " health_note=:health_note, permission_level=:permission_level,"
                    " input_schema=:input_schema, output_schema=:output_schema,"
                    " timeout_s=:timeout_s, binary_ref=:binary_ref,"
                    " provenance_ref=:provenance_ref, source_system=:source_system,"
                    " source_pk=:source_pk, updated_at=:updated_at,"
                    " schema_version=:schema_version WHERE tool_id=:tool_id", row)
                stats["tool_updated"] += 1
        register_alias(conn.cursor(), "tool", tool_id, SOURCE_SYSTEM_TOOLS,
                       ALIAS_NS_TOOL_NAME, name, stats)
        imported += 1

        # ---- tool_version 行（1:N current；版本/健康变化才写，保证 no-op）----
        prov = {
            "probe": {k: ev.get(k) for k in
                      ("probe_type", "argv", "checked_path", "binary_ref",
                       "exit_code", "output_head")},
            "probe_timeout_s": PROBE_TIMEOUT_S,
            "declared_registry_version_raw": (str(declared_raw)
                                              if declared_raw is not None else None),
            "declared_registry_version_clean": declared_clean,
            "declared_status": decl_status,
            "declared_corrupt_reason": decl_reason,
            "registry_generated_at": generated_at,
        }
        prov_json = json.dumps(prov, ensure_ascii=False, sort_keys=True)
        content_hash = hashlib.sha256(prov_json.encode("utf-8")).hexdigest()
        cur_row = conn.execute(
            "SELECT * FROM tool_version WHERE tool_id=? AND status='current'",
            (tool_id,)).fetchone()
        if cur_row is not None and cur_row["version"] == measured \
                and cur_row["health"] == health \
                and cur_row["content_hash"] == content_hash:
            stats["tool_version_skipped_existing"] += 1
        else:
            if cur_row is not None:
                conn.execute(
                    "UPDATE tool_version SET status='superseded', updated_at=?,"
                    " schema_version=? WHERE tv_id=?",
                    (now_iso(), SCHEMA_VERSION, cur_row["tv_id"]))
                stats["tool_version_superseded"] += 1
            conn.execute(
                "INSERT INTO tool_version (tool_id, version, health, provenance_json,"
                " content_hash, validated_at, status, created_at, updated_at,"
                " schema_version) VALUES (?,?,?,?,?,NULL,'current',?,?,?)",
                (tool_id, measured, health, prov_json, content_hash,
                 generated_at, generated_at, SCHEMA_VERSION))
            stats["tool_version_inserted"] += 1

    stats["tool_imported"] = imported
    return {
        "registry_count": len(items),
        "registry_generated_at": generated_at,
        "registry_sha256": sha256_file(SRC_TOOLS_JSON),
        "imported": imported,
        "quarantine": quarantine,
        "drift": drift,
        "probes": probes,
        "health_distribution": dict(sorted(health_dist.items())),
    }


# ---------------------------------------------------------------------------
# Workflows ETL
# ---------------------------------------------------------------------------
WF_ID_RE = re.compile(r"^WF-\d{2}$")


def etl_workflows(conn: sqlite3.Connection, stats: Counter) -> dict:
    if not SRC_WORKFLOWS_YAML.exists():
        raise SystemExit(f"FATAL: 源不存在：{SRC_WORKFLOWS_YAML}")
    text = SRC_WORKFLOWS_YAML.read_text(encoding="utf-8")
    try:
        doc = parse_yaml_subset(text)
    except YamlSubsetError as e:
        raise SystemExit(f"FATAL: workflows.yaml 受限解析失败（全量隔离，不猜测）：{e}")

    wfs = doc.get("workflows")
    if not isinstance(wfs, list) or not wfs:
        raise SystemExit("FATAL: workflows.yaml 无 workflows 列表")

    src_file = str(SRC_WORKFLOWS_YAML)
    ts = mtime_iso(SRC_WORKFLOWS_YAML)
    quarantine, imported_ids = [], []
    seen = set()
    imported = 0

    for idx, wf in enumerate(wfs):
        def fail(reason, _idx=idx, _wf=wf):
            quarantine.append({"index": _idx,
                               "id": _wf.get("id") if isinstance(_wf, dict) else None,
                               "reason": reason})
            stats["workflow_quarantined"] += 1
        if not isinstance(wf, dict):
            fail(f"条目不是映射（得到 {type(wf).__name__}）")
            continue
        wid = wf.get("id")
        if not isinstance(wid, str) or not WF_ID_RE.fullmatch(wid):
            fail(f"id 缺失或不符合 WF-\\d{{2}}：{wid!r}")
            continue
        if wid in seen:
            fail(f"id 重复：{wid}")
            continue
        name = wf.get("name")
        if not isinstance(name, str) or not name.strip():
            fail("name 缺失或为空")
            continue
        phases = wf.get("phases")
        if phases is not None:
            if not isinstance(phases, list) or any(not isinstance(p, dict) for p in phases):
                fail(f"phases 不是字典列表（条目 {idx}）")
                continue
        trigger = wf.get("trigger")
        if trigger is not None and not isinstance(trigger, dict):
            fail(f"trigger 不是映射（得到 {type(trigger).__name__}）")
            continue

        def j(v):
            return None if v is None else json.dumps(v, ensure_ascii=False, sort_keys=True)

        row = {
            "workflow_id": wid, "name": name,
            "description": wf.get("description"),
            "trigger_json": j(trigger),
            "requires_json": j(wf.get("requires")),
            "related_skills": j(wf.get("related_skills")),
            "knowledge_queries": j(wf.get("knowledge_queries")),
            "phases": j(phases),
            "stop_conditions": j(wf.get("stop_conditions")),
            "excluded_activities": j(wf.get("excluded_activities")),
            "source_system": SOURCE_SYSTEM_WORKFLOWS, "source_file": src_file,
            "updated_at": ts, "schema_version": SCHEMA_VERSION,
        }
        existing = conn.execute(
            "SELECT * FROM workflow WHERE workflow_id=?", (wid,)).fetchone()
        if existing is None:
            conn.execute(
                "INSERT INTO workflow (workflow_id, name, description, trigger_json,"
                " requires_json, related_skills, knowledge_queries, phases,"
                " stop_conditions, excluded_activities, source_system, source_file,"
                " created_at, updated_at, schema_version)"
                " VALUES (:workflow_id,:name,:description,:trigger_json,:requires_json,"
                " :related_skills,:knowledge_queries,:phases,:stop_conditions,"
                " :excluded_activities,:source_system,:source_file,:updated_at,"
                " :updated_at,:schema_version)", row)
            stats["workflow_inserted"] += 1
        else:
            same = all(existing[k] == v for k, v in row.items())
            if same:
                stats["workflow_skipped_existing"] += 1
            else:
                conn.execute(
                    "UPDATE workflow SET name=:name, description=:description,"
                    " trigger_json=:trigger_json, requires_json=:requires_json,"
                    " related_skills=:related_skills,"
                    " knowledge_queries=:knowledge_queries, phases=:phases,"
                    " stop_conditions=:stop_conditions,"
                    " excluded_activities=:excluded_activities,"
                    " source_system=:source_system, source_file=:source_file,"
                    " updated_at=:updated_at, schema_version=:schema_version"
                    " WHERE workflow_id=:workflow_id", row)
                stats["workflow_updated"] += 1
        register_alias(conn.cursor(), "workflow", wid, SOURCE_SYSTEM_WORKFLOWS,
                       ALIAS_NS_WORKFLOW_ID, wid, stats)
        seen.add(wid)
        imported_ids.append(wid)
        imported += 1

    # registry 级元数据（单行）
    meta = {
        "id": 1,
        "registry_version": str(doc.get("version")) if doc.get("version") is not None else None,
        "last_reviewed": doc.get("last_reviewed"),
        "default_workflow": doc.get("default_workflow"),
        "source_file": src_file, "updated_at": ts,
        "schema_version": SCHEMA_VERSION,
    }
    existing_meta = conn.execute(
        "SELECT * FROM workflow_registry_meta WHERE id=1").fetchone()
    if existing_meta is None:
        conn.execute(
            "INSERT INTO workflow_registry_meta (id, registry_version, last_reviewed,"
            " default_workflow, source_file, created_at, updated_at, schema_version)"
            " VALUES (:id,:registry_version,:last_reviewed,:default_workflow,"
            " :source_file,:updated_at,:updated_at,:schema_version)", meta)
        stats["workflow_meta_inserted"] += 1
    elif not all(existing_meta[k] == v for k, v in meta.items()):
        conn.execute(
            "UPDATE workflow_registry_meta SET registry_version=:registry_version,"
            " last_reviewed=:last_reviewed, default_workflow=:default_workflow,"
            " source_file=:source_file, updated_at=:updated_at,"
            " schema_version=:schema_version WHERE id=:id", meta)
        stats["workflow_meta_updated"] += 1
    else:
        stats["workflow_meta_skipped_existing"] += 1

    stats["workflow_imported"] = imported
    return {
        "source_file": src_file,
        "source_sha256": sha256_file(SRC_WORKFLOWS_YAML),
        "source_workflow_count": len(wfs),
        "imported": imported,
        "imported_ids": imported_ids,
        "quarantine": quarantine,
        "registry_meta": {"version": meta["registry_version"],
                          "last_reviewed": meta["last_reviewed"],
                          "default_workflow": meta["default_workflow"]},
    }


# ---------------------------------------------------------------------------
# Skills ETL（只读抽取 front matter；不改任何 skill 文件）
# ---------------------------------------------------------------------------
def _parse_front_matter(text: str, fname: str) -> dict:
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        raise YamlSubsetError("文件不以 '---' 开头（无 front matter）")
    close = None
    for i in range(1, len(lines)):
        if lines[i].strip() == "---":
            close = i
            break
    if close is None:
        raise YamlSubsetError("front matter 未闭合（缺结束 '---'）")
    fm = {}
    for raw in lines[1:close]:
        if not raw.strip():
            continue
        if raw.startswith((" ", "\t")):
            raise YamlSubsetError(f"front matter 含缩进行（不支持嵌套）：{raw!r}")
        if ":" not in raw:
            raise YamlSubsetError(f"front matter 行缺少冒号：{raw!r}")
        k, _, v = raw.partition(":")
        k = k.strip()
        if k in fm:
            raise YamlSubsetError(f"front matter 重复键：{k!r}")
        fm[k] = v.strip()
    return fm


def _skill_scalar(v):
    if v is None:
        return None
    s = v.strip()
    if s == "":
        return None
    if len(s) >= 2 and s[0] == s[-1] and s[0] in "\"'":
        return s[1:-1]
    low = s.lower()
    if low == "true":
        return True
    if low == "false":
        return False
    return s


def etl_skills(conn: sqlite3.Connection, stats: Counter) -> dict:
    files = sorted(SRC_SKILLS_DIR.glob("*.md"))
    if not files:
        raise SystemExit(f"FATAL: 源目录无 skill 文件：{SRC_SKILLS_DIR}")
    quarantine, imported_names = [], []
    missing_role, missing_enabled, missing_when_to_use, missing_generator = [], [], [], []
    enabled_dist = Counter()
    seen_names = set()
    imported = 0

    for f in files:
        try:
            text = f.read_text(encoding="utf-8")
            fm = _parse_front_matter(text, f.name)
        except (OSError, UnicodeDecodeError, YamlSubsetError) as e:
            quarantine.append({"file": f.name, "reason": repr(e)})
            stats["skill_quarantined"] += 1
            continue

        name = _skill_scalar(fm.get("name"))
        if not isinstance(name, str) or not name.strip():
            quarantine.append({"file": f.name, "reason": "front matter 缺 name（无法定主键）"})
            stats["skill_quarantined"] += 1
            continue
        name = name.strip()
        if name in seen_names:
            quarantine.append({"file": f.name, "reason": f"name 重复：{name}"})
            stats["skill_quarantined"] += 1
            continue
        seen_names.add(name)

        role = _skill_scalar(fm.get("role"))
        if role is None:
            missing_role.append(f.name)
        roles = None if role is None else role  # 源为单维 role；roles 列逗号分隔格式预留
        description = _skill_scalar(fm.get("description"))
        when_to_use = _skill_scalar(fm.get("whenToUse"))
        if when_to_use is None:
            missing_when_to_use.append(f.name)
        enabled_raw = _skill_scalar(fm.get("enabled"))
        if fm.get("enabled") is None or enabled_raw is None:
            enabled = None
            missing_enabled.append(f.name)
        elif enabled_raw is True:
            enabled = 1
        elif enabled_raw is False:
            enabled = 0
        else:
            enabled = None
        enabled_dist[repr(enabled)] += 1
        generator = _skill_scalar(fm.get("generator"))
        if generator is None:
            missing_generator.append(f.name)

        ts = mtime_iso(f)
        row = {
            "skill_id": name, "name": name, "version": None, "roles": roles,
            "description": description, "when_to_use": when_to_use,
            "required_tools": None, "knowledge_refs": None,
            "body_ref": str(f), "enabled": enabled, "generator": generator,
            "lifecycle": None, "governance": "ACTIVE",
            "source_system": SOURCE_SYSTEM_SKILLS, "source_pk": f.name,
            "updated_at": ts, "schema_version": SCHEMA_VERSION,
        }
        existing = conn.execute(
            "SELECT * FROM skill WHERE skill_id=?", (name,)).fetchone()
        if existing is None:
            conn.execute(
                "INSERT INTO skill (skill_id, name, version, roles, description,"
                " when_to_use, required_tools, knowledge_refs, body_ref, enabled,"
                " generator, lifecycle, governance, source_system, source_pk,"
                " created_at, updated_at, schema_version)"
                " VALUES (:skill_id,:name,:version,:roles,:description,:when_to_use,"
                " :required_tools,:knowledge_refs,:body_ref,:enabled,:generator,"
                " :lifecycle,:governance,:source_system,:source_pk,:updated_at,"
                " :updated_at,:schema_version)", row)
            stats["skill_inserted"] += 1
        else:
            same = all(existing[k] == v for k, v in row.items())
            if same:
                stats["skill_skipped_existing"] += 1
            else:
                conn.execute(
                    "UPDATE skill SET name=:name, version=:version, roles=:roles,"
                    " description=:description, when_to_use=:when_to_use,"
                    " required_tools=:required_tools, knowledge_refs=:knowledge_refs,"
                    " body_ref=:body_ref, enabled=:enabled, generator=:generator,"
                    " lifecycle=:lifecycle, governance=:governance,"
                    " source_system=:source_system, source_pk=:source_pk,"
                    " updated_at=:updated_at, schema_version=:schema_version"
                    " WHERE skill_id=:skill_id", row)
                stats["skill_updated"] += 1
        register_alias(conn.cursor(), "skill", name, SOURCE_SYSTEM_SKILLS,
                       ALIAS_NS_SKILL_NAME, name, stats)
        imported_names.append(name)
        imported += 1

    stats["skill_imported"] = imported
    return {
        "source_dir": str(SRC_SKILLS_DIR),
        "source_file_count": len(files),
        "imported": imported,
        "quarantine": quarantine,
        "missing_role": missing_role,
        "missing_enabled": missing_enabled,
        "missing_when_to_use": missing_when_to_use,
        "missing_generator_count": len(missing_generator),
        "enabled_distribution": dict(sorted(enabled_dist.items())),
    }


# ---------------------------------------------------------------------------
def main() -> int:
    started = now_iso()
    print(f"[etl_m6] v{ETL_VERSION} started_at={started}")
    print(f"[etl_m6] 目标（隔离）：{TARGET_DB}")

    conn = open_target()
    apply_003_schema(conn)
    added_cols = ensure_tool_columns(conn)
    rebuilt_tv = ensure_tool_version_schema(conn)
    extended_alias = ensure_entity_alias_workflow_domain(conn)
    if added_cols or rebuilt_tv or extended_alias:
        conn.commit()
        print(f"[etl_m6] 守卫 DDL：tool 追加列={added_cols} "
              f"tool_version 重建={rebuilt_tv} entity_alias 域扩展(workflow)={extended_alias}")

    # 上一状态摘要（用于变化报告；不覆盖任何内容）
    prev = conn.execute(
        "SELECT content_digests FROM etl_run_log ORDER BY run_seq DESC LIMIT 1"
    ).fetchone()
    prev_digests = json.loads(prev["content_digests"]) if prev else {}

    stats: Counter = Counter()
    stats["alias_inserted"] = 0
    stats["alias_skipped_existing"] = 0

    tools_meta = etl_tools(conn, stats)
    wf_meta = etl_workflows(conn, stats)
    skills_meta = etl_skills(conn, stats)
    conn.commit()

    digests = {t: table_digest(conn, t, pk) for t, pk in CONTENT_TABLES.items()}
    digests["entity_alias@m6"] = table_digest(
        conn, "entity_alias", "id",
        where="source_system IN (?,?,?)",
        params=(SOURCE_SYSTEM_TOOLS, SOURCE_SYSTEM_WORKFLOWS, SOURCE_SYSTEM_SKILLS))
    row_counts = {t: conn.execute(
        f"SELECT COUNT(*) FROM {t}").fetchone()[0] for t in CONTENT_TABLES}  # noqa: S608
    row_counts["entity_alias@m6"] = conn.execute(
        "SELECT COUNT(*) FROM entity_alias WHERE source_system IN (?,?,?)",
        (SOURCE_SYSTEM_TOOLS, SOURCE_SYSTEM_WORKFLOWS, SOURCE_SYSTEM_SKILLS)).fetchone()[0]
    row_counts["tool_execution"] = conn.execute(
        "SELECT COUNT(*) FROM tool_execution").fetchone()[0]
    fk_violations = conn.execute("PRAGMA foreign_key_check;").fetchall()
    conn.close()

    changed = sorted(t for t, d in digests.items() if prev_digests.get(t) != d)

    # 版本不可得（NULL）的工具集合，来自本轮探测证据
    null_version_tools = sorted(
        name for name, ev in tools_meta["probes"].items() if ev.get("version") is None)

    result = {
        "etl_script": "etl_m6.py",
        "etl_version": ETL_VERSION,
        "started_at": started,
        "finished_at": now_iso(),
        "target_db": str(TARGET_DB),
        "guard_ddl": {"tool_columns_added": added_cols,
                      "tool_version_rebuilt": rebuilt_tv,
                      "entity_alias_domain_extended": extended_alias},
        "row_counts": row_counts,
        "tables_changed_vs_previous_run": changed,
        "content_digests": digests,
        "fk_violations": len(fk_violations),
        "stats": dict(stats),
        "null_version_tools": null_version_tools,
        "tools": {k: v for k, v in tools_meta.items() if k != "probes"},
        "workflows": wf_meta,
        "skills": skills_meta,
        "probe_evidence": tools_meta["probes"],
    }
    # detail 全量（探测证据/drift/隔离清单）入库内运行日志
    conn = open_target()
    conn.execute(
        "INSERT INTO etl_run_log (etl_script, etl_version, started_at, finished_at,"
        " content_digests, row_counts, stats, detail) VALUES (?,?,?,?,?,?,?,?)",
        ("etl_m6.py", ETL_VERSION, started, result["finished_at"],
         json.dumps(digests, ensure_ascii=False, sort_keys=True),
         json.dumps(row_counts, ensure_ascii=False, sort_keys=True),
         json.dumps(result["stats"], ensure_ascii=False, sort_keys=True),
         json.dumps(result, ensure_ascii=False, sort_keys=True)))
    conn.commit()
    conn.close()

    print(f"[etl_m6] 行数：{json.dumps(row_counts, ensure_ascii=False)}")
    print(f"[etl_m6] 工具健康分布：{json.dumps(tools_meta['health_distribution'], ensure_ascii=False)}")
    print(f"[etl_m6] drift 项数：{len(tools_meta['drift'])}")
    print(f"[etl_m6] workflow：导入 {wf_meta['imported']}/{wf_meta['source_workflow_count']}，"
          f"隔离 {len(wf_meta['quarantine'])}")
    print(f"[etl_m6] skill：导入 {skills_meta['imported']}/{skills_meta['source_file_count']}，"
          f"隔离 {len(skills_meta['quarantine'])}，缺 role {len(skills_meta['missing_role'])}")
    print(f"[etl_m6] 与上一运行相比变化的表：{changed or '（无，no-op）'}")
    print(f"[etl_m6] FK 违例：{len(fk_violations)}")
    print(f"[etl_m6] done at {result['finished_at']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
