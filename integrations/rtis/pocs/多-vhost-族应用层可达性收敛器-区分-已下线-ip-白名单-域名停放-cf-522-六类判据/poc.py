#!/usr/bin/env python3
"""adminapi/admin family reachability matrix.
One request per host on :443 (SNI=host) and :80 (Host=host). Records app-layer facts only.
TUN env: never trust port scans; only body/status/exception type.
"""
import ssl, socket, sys, json, hashlib, concurrent.futures as cf
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


HOSTS = [l.split('\t')[0].strip() for l in open('runs/vs-adminapi/host-ip.tsv') if l.strip()]
IPMAP = {l.split('\t')[0].strip(): l.split('\t')[1].strip().strip('[]') for l in open('runs/vs-adminapi/host-ip.tsv') if '\t' in l}

ctx = ssl.create_default_context()
ctx.check_hostname = False
ctx.verify_mode = ssl.CERT_NONE

def req(host, ip, port, timeout=8):
    """Return dict with class of outcome."""
    out = {'host': host, 'ip': ip, 'port': port}
    try:
        s = socket.create_connection((ip, port), timeout=timeout)
    except socket.timeout:
        out['r'] = 'CONN_TIMEOUT'; return out
    except Exception as e:
        out['r'] = 'CONN_' + type(e).__name__; return out
    try:
        s.settimeout(timeout)
        if port == 443:
            try:
                s = ctx.wrap_socket(s, server_hostname=host)
            except ssl.SSLError as e:
                out['r'] = 'TLS_' + (e.reason or str(e))[:40]; return out
            except Exception as e:
                out['r'] = 'TLS_' + type(e).__name__; return out
        body = f'GET / HTTP/1.1\r\nHost: {host}\r\nUser-Agent: Mozilla/5.0\r\nAccept: */*\r\nConnection: close\r\n\r\n'
        s.sendall(body.encode())
        chunks = []
        while True:
            d = s.recv(8192)
            if not d: break
            chunks.append(d)
            if sum(len(c) for c in chunks) > 6000: break
    except socket.timeout:
        out['r'] = 'READ_TIMEOUT'; return out
    except Exception as e:
        out['r'] = 'IO_' + type(e).__name__; return out
    finally:
        try: s.close()
        except Exception: pass
    raw = b''.join(chunks)
    out['bytes'] = len(raw)
    hdr_end = raw.find(b'\r\n\r\n')
    head = raw[:hdr_end if hdr_end > 0 else 600].decode('latin1', 'replace')
    out['head'] = head[:400].replace('\r\n', ' | ')[:300]
    m = head.split('\n')[0].strip() if head else ''
    out['status'] = m
    bodypart = raw[hdr_end+4:hdr_end+4+300] if hdr_end > 0 else b''
    out['body'] = bodypart.decode('utf-8', 'replace').replace('\n', ' ')[:200]
    b2 = raw[hdr_end+4:] if hdr_end > 0 else raw
    out['sha1'] = hashlib.sha1(b2).hexdigest()[:10]
    out['r'] = 'HTTP'
    return out

jobs = []
for h in HOSTS:
    ip = IPMAP.get(h)
    if not ip: continue
    for p in (443, 80):
        jobs.append((h, ip, p))

res = []
with cf.ThreadPoolExecutor(max_workers=30) as ex:
    futs = {ex.submit(req, *j): j for j in jobs}
    for f in cf.as_completed(futs):
        try: res.append(f.result())
        except Exception as e:
            res.append({'host': futs[f][0], 'r': 'EXC_' + type(e).__name__})

json.dump(res, open('runs/vs-adminapi/matrix.json', 'w'), indent=0)
print('done', len(res))

