#!/usr/bin/env python3
"""MQ 重放/内部端点定性器：查询型（可能 BOLA）还是触发/写入型（无读取面）？

用法: python3 triage.py map.json docs.json /member/tp-xxx/handler
  map.json  = GET /api/<svc>/actuator/mappings
  docs.json = GET /api/wg-<svc>/v3/api-docs   （可缺，缺则只看签名）

判据（按可靠性排序）：
  A. 返回类型是 RBoolean / R<Void> / boolean  -> 无数据通道 -> BOLA 结构上不成立
  B. 返回类型是 R<Page*>/R<List*>/R<...VO|DTO|ResultDTO> -> 读取面候选，才值得测越权
  C. 入参含 CurrentUser/HttpServletRequest -> 身份来自会话
     入参只有 Message/DTO                    -> 参数完全由请求体控制，不校验归属（写入类即为伪造面）
"""
import json
import sys
import re
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '用法: python3 triage.py map.json docs.json /member/tp-xxx/handler'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----



def load_handlers(map_path):
    d = json.load(open(map_path))
    out = []
    for cname, c in (d.get('contexts') or {}).items():
        ds = (c.get('mappings') or {}).get('dispatcherServlets') or {}
        for _, entries in ds.items():
            if not isinstance(entries, list):
                continue
            for e in entries:
                pred = e.get('predicate') or ''
                hm = (e.get('details') or {}).get('handlerMethod') or {}
                for m in re.finditer(r'\{(GET|POST|PUT|DELETE|PATCH)\s*\[([^\]]+)\]', pred):
                    for p in m.group(2).split('||'):
                        out.append(dict(verb=m.group(1), path=p.strip(),
                                        cls=hm.get('className', ''),
                                        name=hm.get('name', ''),
                                        desc=hm.get('descriptor', '')))
    return out


def ret_type(desc):
    m = re.search(r'\)(.+)$', desc or '')
    return (m.group(1) if m else '').strip()


def classify(desc):
    r = ret_type(desc)
    params = re.findall(r'L([\w/$]+);', (desc or '').split(')')[0])
    identity_from_session = any(('CurrentUser' in p) or ('HttpServletRequest' in p) for p in params)
    if re.search(r'Boolean|Void', r) and 'R' not in r.split('/')[-1]:
        pass
    if r.endswith('RBoolean;') or r.endswith('Boolean;') or r.endswith('V;') or r.endswith('Void;'):
        kind = 'TRIGGER/WRITE (no data channel)'
    elif re.search(r'(Page|List|ResultDTO|VO|ResponseDTO)', r):
        kind = 'READ candidate (test BOLA)'
    else:
        kind = 'UNKNOWN (need api-docs responses)'
    return kind, params, identity_from_session


def docs_lookup(docs_path, path):
    try:
        d = json.load(open(docs_path))
    except Exception as e:
        return None, f'<no docs: {e}>'
    ops = d.get('paths', {}).get(path, {})
    for verb, o in ops.items():
        rb = (((o.get('requestBody') or {}).get('content') or {}).get('application/json') or {}).get('schema') or {}
        resp = ((((o.get('responses') or {}).get('200') or {}).get('content') or {}).get('*/*') or {}).get('schema') or {}
        return (rb.get('$ref', '').split('/')[-1], resp.get('$ref', '').split('/')[-1]), None
    return None, '<path not documented>'


def main():
    map_path, docs_path, want = sys.argv[1], sys.argv[2], sys.argv[3]
    hs = [h for h in load_handlers(map_path) if h['path'].endswith(want) or want in h['path']]
    if not hs:
        print('no handler matched', want)
        return
    for h in hs:
        kind, params, sess = classify(h['desc'])
        print(f"[{h['verb']}] {h['path']}")
        print(f"  handler : {h['cls'].split('.')[-1]}#{h['name']}")
        print(f"  desc    : {h['desc']}")
        print(f"  ret     : {ret_type(h['desc'])}")
        print(f"  params  : {params or ['(none)']}")
        print(f"  identity: {'from session' if sess else 'from request body ONLY (no ownership check possible)'}")
        print(f"  verdict : {kind}")
        req, err = docs_lookup(docs_path, h['path'].lstrip('/').split('/', 1)[-1] and h['path'])
        # 文档里的 path 通常不带服务名前缀差异，这里两种都试
        for cand in {h['path'], '/' + '/'.join(h['path'].split('/')[1:])}:
            req, err = docs_lookup(docs_path, cand)
            if req:
                break
        print(f"  docs    : requestBody={req[0] if req else None} responses={req[1] if req else err}")
        print()


if __name__ == '__main__':
    main()

