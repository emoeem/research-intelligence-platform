# INSTALL：安装与独立部署（P10-4，ROADMAP P10-6 四文档之一）

> 状态：生效中；日期：2026-11-14；适用版本：Phase 10 批次工作树（未 commit）。
> 本文命令已在 fresh clone 演练逐条实跑（docs/phase10/P10-4-BATCH-REPORT.md 第五节演练记录）。
> 相关：docs/guides/BACKUP-RESTORE.md、docs/guides/MIGRATION.md、docs/guides/UPGRADE.md、
> README.md、docs/phase0/PHASE-ROADMAP.md §11.4（独立部署演练验收命令）。

## 一、前提与红线

| 项 | 值 |
| --- | --- |
| 操作系统 | Linux（本机 CachyOS 口径实测；其他发行版按包管理器自适配） |
| Rust | rustup 稳定工具链（workspace `rust-version` 见根 Cargo.toml；rmcp 3.10.0 要求 MSRV 1.88） |
| Python | 3.10+（rp_server.py 与 worker 全部 stdlib，零 pip 安装） |
| Node | >= 22.12（仅 web/ 前端工作台与测试面需要；纯 Rust + Python 使用可跳过第五节） |
| 磁盘 | vendor/ 已入库（离线构建），target/ 构建产物约 2 GiB 级 |
| 红线 | 不在宿主机安装任何系统级依赖；npm 包只进 web/ 项目目录；不用 sudo |

浏览器（视觉门禁用，可选）：不装系统级 chromium，用 Playwright 用户缓存
（`npx playwright install chromium` 只写 `~/.cache/ms-playwright`），或
`PW_CHROMIUM=<chrome 路径>` 显式注入（机器无关口径见 web/playwright.config.ts 头注）。

## 二、获取仓库与离线门禁验证

~~~bash
git clone git@github.com:emoeem/research-intelligence-platform.git research-platform
cd research-platform
cargo test --offline --workspace      # vendor 基线验证：离线全量测试（零网络）
cargo clippy --offline --workspace --all-targets -- -D warnings
~~~

判定：两条命令 exit 0（批次报告记录的实测数：cargo test 全 workspace 通过、clippy 零告警）。
构建失败先查 Rust 工具链版本（`rustc --version`），vendor/ 与 Cargo.lock 必须与提交一致
（不要手工改动 vendor/；新依赖走 UPGRADE.md 第二节的增量 vendor 流程）。

## 三、数据面初始化（二选一）

### 路径 A：空库（迁移链初始化，最小可用）

~~~bash
cargo build --offline -p resource-engine --bin rp-resolve
target/debug/rp-resolve --db migrations/platform.db --migrate-dir migrations --writable --keywords smoke --limit 5
# stderr: rp-resolve: applied 19 migration(s)；stdout 为 resolve 结果 JSON（空库零命中属正常）
~~~

初始化到 `migrations/platform.db`（rp_service.sh 服务单元与 T0 知识工具
scripts/rip_kb_search.py 都按该路径读库，演练实证：缺该文件时工具 run 落
FAILED/EXECUTION，库在位后 SUCCEEDED）。
`migrations/001_init.sql` 到 `019_external_api_audit.sql` 全链在目标库按 user_version 幂等执行
（执行器语义见 docs/guides/MIGRATION.md 第二节）。

### 路径 B：真实数据（ETL dryrun，从入库快照构建）

~~~bash
python3 migrations/etl_dryrun.py       # 只读 sources-snapshot/{catalog,knowledge}.db
# 产物 migrations/platform-dryrun.db（gitignore 运行时数据；重跑幂等覆盖，删除即回滚）
# 随后按 README 快速上手跑对账：
RP_TARGET_DB=migrations/platform-dryrun.db python3 -I migrations/verify.py
~~~

注：全量批（platform-fullbatch.db 口径）需要 26 个活动源库（本地安全工作区数据），
fresh 环境不可得，属已知环境限制（docs/phase10/P10-1-BATCH-REPORT.md 同口径）；
verify.py 的 evidence 段在缺源库环境按两侧对称崩溃点对账（selftest_phase10_backup.sh 口径）。

## 四、启动 Web 工作台（rp_server，stdlib UI + Token 鉴权）

~~~bash
# dev 直跑（前台）：
RP_PORT=7800 RP_DB_PATH=migrations/platform.db \
  RP_AUTH_TOKEN=$(python3 -c 'import secrets; print(secrets.token_hex(32))') \
  python3 apps/server/rp_server.py
# 另开终端冒烟：
curl -s -H "Authorization: Bearer $RP_AUTH_TOKEN" http://127.0.0.1:7800/api/overview | head
~~~

生产形态（systemd 用户服务，无 sudo）：

~~~bash
apps/server/rp_service.sh gen-token      # 令牌文件仓库根 .rp-workbench-token（600）
apps/server/rp_service.sh install        # 写 ~/.config/systemd/user/rp-workbench.service 并启动
apps/server/rp_service.sh status|logs|stop|restart|uninstall
~~~

服务单元固定 `RP_DB_PATH=<repo>/migrations/platform.db`：先用第三节路径 A/B 把该文件建出来
再 install（空文件不算初始化）。ETL 路径 B 的产物是 platform-dryrun.db：用它跑服务时
`RP_DB_PATH=migrations/platform-dryrun.db` 显式覆盖（注意 T0 知识工具仍按
migrations/platform.db 只读检索，工具面与服务面可在同一库对齐：把 dryrun 产物复制为
platform.db 或两种路径各自成对）。

## 五、React Workbench（web/，可选）

~~~bash
cd web && npm install                   # npm 包只进 web/（红线）；node_modules 不入库
RP_API_TARGET=http://127.0.0.1:7800 npm run dev    # vite 代理 /api 到 rp_server
npm run build                           # tsc --noEmit + vite build，产物 web/dist
npm run test:visual:gate                # 构建后视觉门禁（axe AA + 快照基线 + 零 console 错误）
~~~

视觉门禁在缺浏览器的机器先落用户缓存浏览器，再注入：

~~~bash
npx playwright install chromium         # 只写 ~/.cache/ms-playwright，零系统级安装
PW_CHROMIUM=$HOME/.cache/ms-playwright/chromium-*/chrome-linux64/chrome npx playwright test
~~~

快照基线在 web/qa/visual/baseline/（统一字体环境：Noto Sans CJK SC 注入钉死）；
跨机器或字体环境变更时整目录重落：`npx playwright test --update-snapshots`
（web/playwright.config.ts 头注口径），不做单张补丁。

## 六、冒烟清单（安装完成的判定）

~~~bash
# 1. resolve（资源检索）：
target/debug/rp-resolve --db migrations/platform.db --keywords smoke --limit 5
# 2. search（FTS 索引构建 + 查询；catalog.db 用入库快照）：
cargo build --offline -p search-engine --bin rp-search-index
target/debug/rp-search-index build --platform-db migrations/platform.db \
  --catalog-db migrations/sources-snapshot/catalog.db --index-db /tmp/idx.db
target/debug/rp-search-index query --index-db /tmp/idx.db --keywords smoke --limit 10
# 3. 一次工具 run（T0 python_worker 本地面，零网络零容器；前置：rip-data/ 目录在位，
#    migrations/platform.db 已初始化，工具脚本按该路径只读检索）：
mkdir -p rip-data
cargo build --offline -p tool-registry --bin rp-tool -p mcp-server
target/debug/rp-tool register --manifest manifests/tools/rip.kb.keyword_search.json --db rip-data/tool-runs.db
target/debug/rp-tool run --db rip-data/tool-runs.db --tool rip.kb.keyword_search --input '{"keyword":"smoke"}'
target/debug/rp-tool status --db rip-data/tool-runs.db --run <上一步输出的 run_id>
~~~

判定：三条冒烟命令 exit 0 且输出为合法 JSON（rp-tool run 输出 run_id 且 status=SUCCEEDED，
不 FAILED）。工具 run 的派发链经 rp-mcp（MCP 通道）与外部 AI 同源。

## 七、宿主机干净度核对（验收句「宿主机保持干净」）

部署前后各留一次快照并 diff（ROADMAP §11.4 原口径）：

~~~bash
for c in podman virsh virt-install qemu-img docker quickemu node python3 cargo npx; do
  command -v "$c" || echo "$c=MISSING"
done > /tmp/host-tools-after.txt
diff /tmp/host-tools-before.txt /tmp/host-tools-after.txt   # 期望：无新增条目
~~~

## 八、已知限制

1. podman/VM Runner 面（真容器派发、VM 故障演练）需要宿主机有 podman；本机未安装时
   工具 run 走 python_worker 本地面（T0/T1 工具全可用），VM 长演练不在本地口径
   （ROADMAP P10-4「模型在环与 VM 长演练除外」）。
2. embedding 模型面（bge-small-zh ONNX）缺模型时自动回退 hash 后端并在响应标注
   backend（README 口径），fresh 环境首次使用显示回退态属预期。
3. 本仓为私有工程线（无顶层 LICENSE，见 docs/phase10/DEPENDENCIES-RUST.md 第一节）。
