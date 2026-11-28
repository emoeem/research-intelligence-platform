# HTTPS-CUTOVER-RUNBOOK（PB-007 · 2026-11-28）

状态：P2/Optional 的切换产物就绪。本机已实测「TLS 终结反代 + 生产探针」全链
（`apps/server/selftest_https_entry.sh` 10/10），真实公网切换仍需域名/证书决策，
步骤如下，按序执行，任何一步失败即停且不切。

## 前置（用户决策点）

1. 域名与 DNS：`[DOMAIN]` A/AAAA 记录指到出口 IP；本地 7800 不对公网开放。
2. 证书：Let's Encrypt（Caddy 自动）或私有 CA（探针配 `RP_PRODUCTION_CACERT`）。
3. 只读探针凭据：`rp-admin key-gen` 签只读 caller（T1/T0），探针用
   `RP_PROBE_TOKEN`（GET 与写同面鉴权；token 永不进日志/仓库）。

## 切换步骤

1. 反代上线：`containerfiles/https-cutover/Caddyfile`（Caddy 自动签发）或
   `nginx.conf.sample`（手动证书）。上游 = 本机 rp_server 7800。
2. 预检（旧入口不切）：`curl -fsS http://127.0.0.1:7800/api/overview`（内网可达）。
3. 探针验收（切换闸，全部过才切）：
   ```bash
   RP_PRODUCTION_URL=https://[DOMAIN] \
   RP_PROBE_TOKEN=<只读token> \
   bash scripts/rip_production_probe.sh
   # 预期: PRODUCTION_PROBE=PASS（/api/overview + 根 + 证书三项全过）
   ```
4. DNS 切换 + 旧 7800 入口停用（M8 STAGED→DEFAULT 需用户验收后才执行）。
5. 回滚：反代下线即回 7800 直连；DNS TTL 预估 ≤ 演练值。

## 红线

- 探针零写操作；证书私钥权限 0600；HSTS 一次开启后按 RFC 6797 难即时回滚，
  首次上线用短 max-age（样例已含 31536000，正式切可先临时降）。
- 本机演练夹具（apps/server/https_proxy_probe.py）只用于本地验收，非生产组件。
