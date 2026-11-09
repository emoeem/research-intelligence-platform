#!/usr/bin/env python3
"""SSTI 引擎指纹阶梯探测（模板，未针对任何真实目标验证）。

判据：对同一注入参数发送七级 probe，做响应差分（原样回显 vs 求值结果）：
  L1 {{7*7}}        → 49 = Jinja2/Twig(Jinja兼容)
  L2 ${7*7}         → 49 = Freemarker/Velocity(${})/Pebble/Mako(${})
  L3 <%= 7*7 %>     → 49 = ERB/EJS
  L4 {7*7}          → 49 = Smarty（{math} 之外的算术需 {7*7} 走 Smarty3 表达式）
  L5 {{7*'7'}}      → 7777777 = Jinja2（乘法语义）；49 = Twig
  L6 ${7*7} + ${T(java.lang.Runtime)} 类 Java 表达式报错 → Java 模板（Freemarker/Velocity/Thymeleaf）
  L7 #{7*7}         → 49 = Thymeleaf/PEB 变体
响应包含 49/7777777 且与基线不同 → 求值成立；再按 L5 区分 Jinja2 vs Twig。
引擎→RCE payload 阶梯见技能 hunt-ssti。

用法：
  python3 fingerprint.py -u 'https://target/hello?name={PROBE}'            # GET 参数位
  python3 fingerprint.py -u 'https://target/api/hello' -H 'X-Name: {PROBE}' # header 位
  python3 fingerprint.py -r req.txt -p body:username                        # 请求体字段位（req.txt 含 {PROBE} 占位说明）

纪律：
  - 仅对授权目标使用；probe 全部为纯算术表达式，无副作用
  - 判定成立后立刻停止阶梯升级，RCE 验证需另行授权并记录回填 knowledge.db
"""
import argparse
import re
import urllib.parse
import urllib.request
import urllib.error

PROBES = [
    ("L1-jinja-twig", "{{7*7}}"),
    ("L2-dollar", "${7*7}"),
    ("L3-erb", "<%= 7*7 %>"),
    ("L4-smarty", "{7*7}"),
    ("L5-jinja-vs-twig", "{{7*'7'}}"),
    ("L6-java-class", "${T(java.lang.Runtime)}"),
    ("L7-thymeleaf", "#{7*7}"),
]
EVAL_RE = re.compile(r"\b(49|7777777)\b")


def send(url_template, header, body_field, probe, timeout=10):
    if header:
        req = urllib.request.Request(url_template, headers={header: probe})
        return _do(req)
    if body_field:
        data = body_field.replace("{PROBE}", probe).encode()
        req = urllib.request.Request(url_template, data=data)
        return _do(req)
    return _do(urllib.request.Request(url_template.replace("{PROBE}", urllib.parse.quote(probe))))


def _do(req, timeout=10):
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        return e.read().decode("utf-8", "replace")
    except Exception as e:
        return f"__ERR__{type(e).__name__}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("-u", "--url", required=True, help="含 {PROBE} 占位符")
    ap.add_argument("-H", "--header", help="header 名（值内含 {PROBE}）")
    ap.add_argument("-p", "--body-field", help="请求体模板，含 {PROBE}")
    a = ap.parse_args()

    base = send(a.url, a.header, a.body_field, "PROBEPLACEHOLDER999")
    verdicts = []
    for name, probe in PROBES:
        resp = send(a.url, a.header, a.body_field, probe)
        if resp.startswith("__ERR__"):
            verdicts.append((name, resp))
            continue
        m = EVAL_RE.search(resp.replace(base[:40], "")) if len(resp) < 20000 else None
        hits = [x for x in ("49", "7777777") if x in resp and x not in base]
        verdicts.append((name, f"eval-hit={hits}" if hits else "no-eval"))
    print(f"基线长度 {len(base)}")
    for name, v in verdicts:
        print(f"  {name:<18} {v}")
    j777 = any("7777777" in v for _, v in verdicts)
    j49 = any("49" in v for _, v in verdicts)
    if j777:
        print("判定: Jinja2（或 Python 系模板）——引擎确认")
    elif j49:
        print("判定: 存在求值（Twig/Freemarker/Velocity/ERB 家族），按 L5/L6/L7 细分——引擎确认")
    else:
        print("判定: 未观察到求值（可能被编码/过滤/不回显），转盲注见 hunt-ssti 技能")


if __name__ == "__main__":
    main()
