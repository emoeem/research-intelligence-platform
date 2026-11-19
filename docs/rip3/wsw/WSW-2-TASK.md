# WSW-2 任务书 · Proxy / MITM Runtime

- 批次：WSW-2（PLAN §九 :604：Listener、HTTP/HTTPS CONNECT、TLS CA、拦截队列、Forward / Drop；**不包含透明代理、HTTP/3、History/Repeater UI**）
- 日期：2026-11-19
- 上位：`docs/rip3/wsw/04-ADR.md`（ADR-0004，D1 钉版 / D2 POC 不转正 / D4 IP SAN 必做 / 差距七项）、`docs/rip3/wsw/05-DESIGN-AND-THREAT-MODEL.md`（§2 状态机 / §3 数据归属 / §4 八条边界）、`docs/rip3/wsw/WSW-2-ADR.md`（本批十项裁决 W2-D1～D10，**本批模块边界/表形状/闸序/存储位的裁决以它为准**）、`docs/rip3/wsw/WSW-CONTINUATION-REDLINES.md`（八步循环 + 九条红线）
- 状态：设计定稿，待实施
- 硬验收口径声明：原手册 `/media/emo/code/pensting/WSW-CONTINUATION-WORKFLOW.md` 已丢失（`WSW-CONTINUATION-REDLINES.md:3-5` 固化结论），其"§4 W2 硬验收行"按 REDLINES 第 5 条从仓库内可考来源重建：`.zcode/workflow-drafts/WSW-2-主体批次.dwf.ts:1,77,114-135,163-171` + PLAN §九 :604 + ADR-0004 D4/D5。本任务书 §四即重建后的硬验收全集；外部口径冲突以 PLAN / ADR-0004 / 本档为准。

---

## 一、目标（可验收定义）

1. **Listener socket 运行时**：hudsucker 0.25.0 组装进 `crates/wsw-runtime`（本 ADR D1 五模块），只听 loopback（非 loopback 拒绑 exit 级拒绝 + accept 源二次校验），驱动既有 `ListenerState` 六态；HTTP 明文面与 HTTPS CONNECT MITM 双面可用（本机靶场实测）。
2. **WswAuthority 补 IP SAN**（ADR-0004 D4 必做）：自实现 `CertificateAuthority` trait，IP 直连目标动态证书含 `SanType::IpAddress`；CA 持久化复用；证书缓存（moka cap=1000，TTL=有效期半程）；CA 私钥/证书落 `rip-data/wsw-ca/`（0600/0700），指纹展示、导出 PEM、重生成全为用户显式动作；**禁静默安装、禁默认改系统代理**。
3. **拦截队列持久化**：024 增量迁移三表（wsw_listener/wsw_session/wsw_intercept），SQLite 链头 023→024；内存 `PendingQueue`（既有原语）+ 持久层双面；TTL/队满/停机全部强制 EXPIRED 终态；Forward / Drop / Modified-Forward 三动作。
4. **Scope 闸后端强制**：include/exclude、重定向逐跳二次校验（帽接线留 WSW-3，W2-D5 查点 B 修订批口径——本批保障 = 每跳经代理即新请求复走查点 A）、DNS rebinding 防护（解析 IP 过闸 + Pinned-IP，集成级负例 = 解析桩注入，§四.2）、私网/回环/链路本地/云元数据拒绝（云元数据 deny-never-allow）；无规则 default deny；拒绝记录可见。
5. **本机实验台实拍**：真实 chromium 经代理访问 `scripts/local_range.py --port 8443` 靶场，截图 ≥4 张入库 `docs/rip3/batches/browser-e2e/wsw-2/`；HTTPS CONNECT curl + openssl s_client 实证 issuer 与 IP SAN；`ss -tlnp` 实证仅绑 loopback。
6. **W0 遗留两项验收**：上游纯 h2-only 本地夹具实测（禁外联）+ 容器内 aws-lc-sys 构建（podman builder 层，失败回退增补 cmake 如实记录）。
7. **UI 最小面**：两页——代理状态页 + 证书状态页（本 ADR D8）；History/拦截编辑器/Repeater/Fuzzer UI 全部留 W3+（明文边界）。

非目标（本批不做，做了即越界）：透明代理 / TPROXY / HTTP-3（PLAN :604 不包含列）；`wsw_exchange` 元数据表与 audit-engine Emit 接线（W3，本 ADR D9）；History/Repeater/Fuzzer UI 与发送面（W3/W4）；WebSocket Workbench（W5；本批仅 WS 帧透传挂载，POC 已证面）；拦截规则结构化编辑器（本批最小 host/method/path 匹配）；本机访问令牌完整方案（本批 = accept 源 loopback 二次校验 + API Bearer，令牌面 WSW-3，如实登记）；性能全量基线（PLAN §11.6 其余项随批如实写缺测）。

## 二、文件清单草案

| 文件 | 动作 | 内容 |
|---|---|---|
| `crates/wsw-runtime/Cargo.toml` | 修改 | 依赖增补（全部 lock 已有，等号钉既有版本）：hudsucker `=0.25.0`（features 同 POC）、rcgen 0.14.10 经 hudsucker 再导出、`moka = "=0.12.16"`、`time = "=0.3.55"`、`rand = "=0.10.4"`（lock 实值 hudsucker 0.25.0 依赖链，修订批按实现修正 0.9.5→0.10.4）、hyper-rustls 0.27、webpki-roots 1.0、http-body-util、database、domain、wsw-message、serde/serde_json；`tokio = { workspace = true, features = ["net"] }`（POC 先例 `crates/wsw-poc/Cargo.toml:37-38`） |
| `Cargo.lock` | 修改（仅 wsw-runtime 依赖清单行） | **`[[package]]` 总数 400 不变**；vendor 零变更；registry package 375 不变 |
| `crates/wsw-runtime/src/lib.rs` | 修改 | 模块声明 + crate doc 更新（现"no socket IO"声明作废，`src/lib.rs:1-2`）；既有根原语不动 |
| `crates/wsw-runtime/src/authority.rs` | 新建 | WswAuthority：CertificateAuthority trait 实现 + IP SAN/DNS SAN 分支 + moka 缓存 + CA 持久化读写（0600 no-clobber）+ 指纹（本 ADR D2/D3） |
| `crates/wsw-runtime/src/scope.rs` | 新建 | ScopeRules/check/audit_resolved：闸序 scheme→host→port→path→IP 审计 + 云元数据词表 + mapped 归一（本 ADR D5） |
| `crates/wsw-runtime/src/listener.rs` | 新建 | bind 守卫 + accept 源二次校验 + Proxy 组装 + ListenerState 生命周期；`listener::handler` 子模块：HttpHandler（流式 + 帽值 + 拦截队列 + scope 调用）/ WebSocketHandler（透传）/ 错误五态 ProxyErrorClass（本 ADR D1/D6/D7） |
| `crates/wsw-runtime/src/persist.rs` | 新建 | 024 三表 store：listener/session 登记、intercept enqueue/resolve/重启对账（本 ADR D4/D6） |
| `crates/wsw-runtime/tests/` | 新建 | `scope_gate.rs`（闸序正负例）、`authority_ca.rs`（SAN/缓存/持久化）、`intercept_queue.rs`（队列+持久化+对账）、`mitm_loopback.rs`（本地靶场 MITM e2e，含 h2-only 夹具） |
| `migrations/024_wsw_intercept.sql` | 新建 | 三表 + 索引 + schema_version(24) + `PRAGMA user_version=24` 双写；纯新增零 ALTER；禁改历史迁移（本 ADR D4） |
| `apps/server/rp_server.py` | 修改 | WSW 端点：listener start/stop/status、CA status/fingerprint/export/regenerate（显式 confirm 语义）、intercept 队列只读摘要；Bearer 鉴权同既有面（`rp_server.py:3157-3168` 先例） |
| `web/`（导航 + 两页组件） | 修改/新建 | 代理状态页 + 证书状态页（本 ADR D8）；不建平行设计系统（`docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md` 必读） |
| `web/scripts/rip3_wsw2_browser_e2e.mjs` | 新建 | chromium 经代理访问靶场 e2e，骨架复制 `web/scripts/rip3_p5_9c_browser_e2e.mjs` 模式（playwright chromium 用户缓存探针原样复用） |
| `docs/rip3/batches/browser-e2e/wsw-2/*.png` | 新建 | 实拍截图 ≥4 张 |
| `docs/rip3/batches/WSW-2-BATCH-REPORT.md` | 新建 | A-K 十一节批次报告（坏消息单列、数字实测带来源） |
| `docs/rip3/wsw/WSW-1-BATCH-REPORT.md` | 修改 | §5/§6 过时 CI 终态回填（实态：收口批 077a8dd4 + CI 44293771053 绿 + DI-1 2cef9439/44394568183 绿，按 git log 实测改写，不改其他节） |

不动面：`crates/wsw-message/**`（只消费公开面 `crates/wsw-message/src/lib.rs:40-48`）、`crates/wsw-poc/**`（D2 不转正不删除不重构）、`migrations/0*.sql` 历史（禁改）、`migrations/platform*.db`（零触碰）、workspace `Cargo.toml` tokio features（net 增补只落 wsw-runtime 自己的 Cargo.toml）、`vendor/**`。

## 三、批内步骤（S1→S10，每步过门再进）

1. **S1 底座依赖面**：wsw-runtime Cargo.toml 依赖增补 + tokio net feature；`cargo check -p wsw-runtime --offline` 绿；`grep -cE '^\[\[package\]\]' Cargo.lock` = 400 断言；`cargo deny check licenses` 绿。
2. **S2 authority 模块**：WswAuthority（IP SAN 四例单测：127.0.0.1/[::1]/v6 纯/::ffff: mapped → 全 IpAddress；DNS SAN 例；缓存命中同 Arc 例；CA 持久化加载复用例；私钥 0600 权限断言）。门：单测绿 + SAN 断言逐位。
3. **S3 scope 模块**：ScopeRules/check/audit_resolved + 云元数据词表；正负例单测（include 命中/exclude 优先/端口不符拒/private 拒（无 allow_private）/private 过（显式 allow_private + include 留痕）/云元数据拒（allow_private 也拒）/mapped 拒/max_redirects 帽）。
4. **S4 listener 模块**：bind 守卫（loopback 拒绑 + accept 源二次校验）→ Proxy 组装 → 生命周期六态 → listener::handler（流式读 + 头/体帽 + 拦截队列接入 + scope 双查点 + 错误五态 ProxyErrorClass + 脱敏日志投影）。门：`ss -tlnp` 非例拒绑实测 + 本地靶场 curl 明文/CONNECT 双面通。
5. **S5 persist 模块 + 024 迁移**：三表 SQL + store 接口；测试内存库与开发库副本（dev-b1 临时副本）双跑 user_version=24；源库 SHA-256 前后不变（023 S6 同范式）；重启对账单测（queued 未过期回载/过期→expired/停机全 expired）。
6. **S6 集成测试**：`crates/wsw-runtime/tests/` 四文件；MITM e2e 用 `scripts/local_range.py --port 8443`（127.0.0.1 绑定，禁外联）；正负例矩阵见 §四.2。门：`cargo test -p wsw-runtime --offline -- --test-threads=1` 全绿。
7. **S7 CA 管理面 + 最小 UI**：rp_server WSW 端点（显式 confirm 语义的 regenerate/export；禁任何系统代理/信任库写调用）+ web 两页；web touched → `npm --prefix web run build` + `npm --prefix web run test` 绿。
8. **S8 实拍**（§四.3 全项）：chromium e2e 截图 ≥4 张、CONNECT+IP SAN openssl 实证、loopback 绑定 ss 实证、CA 私钥 600 + git status 核对不入库。
9. **S9 W0 遗留两项**（§四.4）：h2-only 夹具实测 + podman builder 层 aws-lc-sys 构建（Bash 600s 上限分步或后台跑；失败回退 cmake 如实记录）。
10. **S10 收口**：性能延迟增量实测面（§四.5）；批次报告 A-K + WSW-1 报告 CI 回填；任务书/ADR/DESIGN 一致性核对（DESIGN §2.1 入口位若写 wsw-proxy 名称，以本 ADR D1"wsw-runtime 原地扩展"为准回改一处）。

## 四、完成标准（逐条可验收；验收 = 命令实跑回显）

### 4.1 定向门禁（工作流门禁面，`.zcode/workflow-drafts/WSW-2-主体批次.dwf.ts:114-135` 原样）

```bash
cd /media/emo/code/pensting/research-platform
cargo test --offline -p wsw-runtime -- --test-threads=1          # 全绿（基础面 9 测试 + 本批新增全量）
cargo clippy --offline -p wsw-runtime -p wsw-message --all-targets -- -D warnings   # 绿
cargo deny check licenses                                        # 绿
grep -cE '^\[\[package\]\]' Cargo.lock                           # 400（零新外部包红线）
python3 -c "import sqlite3,glob,sys
for db in sorted(glob.glob('migrations/*.db')):
    if 'platform' in db.split('/')[-1]: continue
    print(db, sqlite3.connect('file:'+db+'?mode=ro',uri=True).execute('PRAGMA user_version').fetchone()[0])"
# → 至少一个开发库副本为 24；platform*.db 不出现在写入面
git status --porcelain web | grep -q . && { npm --prefix web run build && npm --prefix web run test; }   # web touched 才跑
```

### 4.2 安全负例矩阵（集成测试断言逐条落库，禁恒真断言）

| 用例 | 断言 |
|---|---|
| 非 loopback 拒绑 | bind 0.0.0.0/:: 外地址 → 启动失败 + exit 级错误（`src/lib.rs:15-17` ConfigError::NonLoopback 消费） |
| accept 源非 loopback | 模拟非回环对端 → 连接拒 + 日志五态 |
| 超范围拒（无规则 default deny） | 未配置 scope 规则 → 请求 403 + `scope_decision=out_of_scope_blocked` 记录可见 |
| include 命中放行 | 配置 include 127.0.0.1:8443 + allow_private → 200 + 留痕行 |
| exclude 优先 | include *.example + exclude host → exclude 命中拒 |
| 重定向越界拒 | 靶场重定向到 scope 外 host → 第二跳拦截 + 记录 |
| DNS rebinding 拒（集成级） | 域名解析指向私网 IP → 解析 IP 过闸拒 + remote_ip 落行。集成级实现 = 解析桩注入（`DnsResolver` trait 可注入面，免改 /etc/hosts）：`AuditedConnector::call` 拒 → `ConnectorError::ScopeRejected` 携带 remote_ip（经 `source()` 链入下溯链）→ `respond_to_scope_reject`（`handle_error` 查点 C 决策同一实现）403 + `x-wsw-scope-decision` 头——`tests/scope_gate.rs` 尾部两例真实执行；`audit_resolved` 本体单测级另在册 |
| 云元数据 deny-never-allow | allow_private=true + include 169.254.169.254 → 仍拒（reason=metadata_endpoint） |
| mapped 归一 | ::ffff:127.0.0.1 目标 → 按 V4 私网判定 |
| CA 不静默安装 | 全代码面 grep 无 networksetup/gsettings/注册表调用；导出/重生成端点无 confirm 拒绝执行 |
| 队列 TTL/队满/停机 | 三路径全部 expired 终态（既有单测面 + persist 对账面） |

### 4.3 实拍验收面

1. chromium（playwright 用户缓存）配置代理走本地代理访问 `scripts/local_range.py --port 8443`，断言经代理捕获（日志面/wsw_body_blob 行面证据）+ 截图 ≥4 张入库 `docs/rip3/batches/browser-e2e/wsw-2/`（截图文件存在且非空）。
2. HTTPS CONNECT MITM：`curl --cacert <导出 CA PEM> -x http://127.0.0.1:<port> https://<靶场 TLS 端点>` 200；`openssl s_client` 实见 issuer=WswAuthority 根 CA 且 SAN 含 IP。
3. `ss -tlnp` 实证代理仅绑 127.0.0.1（或 ::1）。
4. CA 私钥文件权限 600（`stat -c %a`）且 `git status` 不含 `rip-data/`。
5. 全部命令原文 + 关键回显进批次报告 E 节。

### 4.4 W0 遗留两项（如实记录，禁冒充 PASS）

1. **纯 h2-only 上游实测**：本地 h2-only 服务夹具（hyper http2_only，绑 loopback 高位端口）经代理请求 → 断言成功 + alpn=h2 记录；做不到 → 差距与原因如实进批次报告 G 节（ADR-0004 差距 (b) 收口状态二选一：收口或登记继续遗留）。
2. **容器内 aws-lc-sys 构建**：`podman build` 主 `Containerfile`（builder 层 gcc+libc6-dev 无 cmake，`Containerfile:23,31`）；失败 → 回退增补 cmake 重跑并如实记录（差距 (f) 收口状态二选一）。宿主机零系统级安装。

### 4.5 性能实测面

单请求代理延迟增量：直连靶场 vs 经代理同请求（curl `-w` time_total，repeat≥3 取中位数），数字进批次报告 F 节；并发连接数/大 Body 内存峰值可达则测，不可达如实写缺测（PLAN :754 禁预填）。

## 五、红线清单（批内任何步骤不得越过）

1. **Rust-First**：代理核心全 Rust（PLAN §6.1 :444、§十二 :760）；Python/Node 只允许在靶场脚本（既有）与浏览器 e2e 骨架（既有模式）出现。
2. **零新外部包**：Cargo.lock `[[package]]` = 400 不变；wsw-runtime 新依赖全部等号钉 lock 既有版本（moka/time/rand 为传递依赖转正直接声明，WSW-1 sha2/base64 先例）；vendor 零变更；registry package 375 不变。
3. **tokio 仅 net feature 增补**：只落 `crates/wsw-runtime/Cargo.toml` 一处，workspace features 与其他 crate 不动（POC 先例 `crates/wsw-poc/Cargo.toml:38`）。
4. **024 迁移纪律**：增量迁移只新增；禁改历史迁移文件；ALTER 不入本批；只在测试内存库与开发库副本执行；`migrations/platform*.db` 零触碰（023 文件头 :30 纪律）；链头 023→024 单行推进。
5. **loopback 默认**：代理只听 127.0.0.1/::1；非 loopback 拒绑；accept 源二次校验；禁止匿名开放代理姿态（PLAN §7.1 :512-514）。
6. **CA 红线**：禁静默安装/信任根 CA；禁默认改系统代理（禁 networksetup/gsettings/注册表写面）；CA 私钥 0600 落 rip-data/wsw-ca/，不入 Git/日志/普通 Artifact；PEM 导出与重生成必须显式用户动作。
7. **Scope 后端强制**：闸只在 Rust 后端；default deny；云元数据 deny-never-allow；out-of-scope 拒绝记录必须可见（PLAN :528）。
8. **wsw-poc 不转正**：不删除、不重构、不加功能（ADR-0004 D2）；POC 代码只读参照。
9. **不建第二套事实体系**：六 ID/trace/artifact/evidence 引用走既有链；新表只在 024；`wsw_exchange`/事件流水不预建（本 ADR D9）；RTIS 零接触（`crates/intel-db/src/lib.rs:10`）。
10. **测试纪律**：禁外联（靶场 local_range loopback）；集成/容器测试 `--test-threads=1` + timeout 保护；同一失败方法最多两次，第二次无新信息换本质不同方法并如实记录（REDLINES 红线 7）；测试红只修代码不改测试钉值（除非坐实钉值过时并如实记录）。
11. **宿主机零系统级安装**：不用 sudo；容器面走既有 podman 镜像工具链，缺能力如实报告不装（REDLINES 红线 8）。
12. **敏感面**：Cookie/Authorization 等凭据不进日志/错误/截图（`wsw_message::redact` 挂日志投影）；错误日志只落 class/error_code/cause 摘要，禁整头/整体 body（DESIGN §4.5）。
13. **收口纪律**：显式 `git add` 本批文件清单（禁 add -A；`.auto/` 不加）；commit/push 按 REDLINES 批次循环自走；CI 红不进下一批。

## 六、退出条件与回退

- 定向门禁（§四.1）两轮修复仍红 → 批次挂起，改动留工作树不 commit，批次报告如实记录挂起点（不允许降标准换"通过"）。
- 024 若与 023 链冲突：迁移重新编号（025+），本 ADR D4 / DESIGN §3.2 同步改号，禁改历史迁移。
- 实拍或 W0 遗留两项失败：如实记录差距与原因进批次报告 G 节，门禁面（§四.1）不因此降级；不得用"计划中"冒充实测。
- 回退面：本批全部改动限于 wsw-runtime/024/rp_server 端点/web 两页/文档；`git revert` 对应 commit 即回；CA 私钥目录属运行时数据，不入库故无回滚面。
