# RT-7 批次验收报告 · CI 与发布策略核验（CI 修固 + 发布矩阵三面实测 + selftest 陈旧钉收口）

- 批次：RT-7（RELEASE 前最后一个 RT 批；NATIVE-FIRST :229/:242 验收词形对照）
- 日期：2026-11-23
- 基线：HEAD `dca07555`（WSW-8 收口在链；RT-5/6 `f8d38107` 先行在链）+ 工作树 1 行未跟踪 = 任务书自身（`docs/phase0/RT-7-TASK.md`，禁 commit 纪律下的预期态）
- 任务书：[RT-7-TASK.md](RT-7-TASK.md)；核验专档：[RT-7-CI-RELEASE-VERIFICATION.md](RT-7-CI-RELEASE-VERIFICATION.md)
- 结论：判据 1-12 全 PASS（判据 8 带程序注——中间态漂移 + stable 路径复现收口，详见坏消息 1）；八红线全守住；零迁移零新依赖

---

## §1 改动文件清单（路径 + 行数）

| # | 文件 | 类型 | 规模 | 内容 |
|---|---|---|---|---|
| 1 | `.github/workflows/ci.yml` | 修改 | +12/-8 | D1①③④：timeout-minutes ×4（rust 45 / parity 20 / python-workers 15 / web 15）+ checkout@v5 ×4 + setup-python@v6 + setup-node@v5 + dtolnay@1.99.1 ×2；cache v4 维持；测试/clippy/lock/触发/权限/concurrency 零 diff |
| 2 | `rust-toolchain.toml` | 新建 | 7 行 | D1②：`channel = "1.99.1"` + `components = ["clippy"]` + 出处注 |
| 3 | `apps/server/selftest_wsw7.sh` | 修改 | +9/-9 | D2①：:285-294 块——块注收口态词形（WSW-8 :166 出处注）+ :292/:293 期望 3→0 + :294 399→417；:290-291 变量行与 :296 CROSS 零触碰 |
| 4 | `apps/server/selftest_wsw4.sh` | 修改 | +1/-1 | D2②：:39 单行 28→30（label 携行注词形） |
| 5 | `docs/phase0/RT-7-CI-RELEASE-VERIFICATION.md` | 新建 | 182 行 | D3 四节专档（CI 修固 / build 三面 / containerfiles 双面 / RELEASE 对账输入面） |
| 6 | `docs/phase0/RT-7-BATCH-REPORT.md` | 新建 | 本档 | 判据对照 + 坏消息 + 会签声明 + 观察池 |
| 7 | `docs/README.md` | additive | +2 行 | 索引登记两份新档（RT-5/6 行后插入，:52-53 新位） |

批外文件（非本批改动、基线态如实登记）：`docs/phase0/RT-7-TASK.md`（侦察落稿，未跟踪 1 行——主会话收口处置）。

## §2 判据 1-12 逐条实测对照

| # | 判据 | 状态 | 实测（exit code + 关键输出行） |
|---|---|---|---|
| 1 | CI yml 修固面 | PASS | `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` → echo `YAML_OK`（exit 0）；`grep -c "timeout-minutes"` = **4**；ref 对账 grep：checkout@v5 ×4（:36/:70/:95/:121）+ dtolnay@1.99.1 ×2（:39/:73）+ cache@v4 ×2（:44/:76）+ setup-python@v6（:98）+ setup-node@v5（:124）；`git diff ci.yml` = 12 insertions / 8 deletions 两类词形，零测试/clippy/触发面 diff |
| 2 | 工具链钉定一致性 | PASS | 三处一致 grep：rust-toolchain.toml `channel = "1.99.1"`（:7）= ci.yml `@1.99.1`（:39/:73）= `rustc --version` = `rustc 1.99.1 (58cf8f9c3 2026-10-13)`；落点前后 rustc --version 逐字一致（本地零下载反证） |
| 3 | timeout 帽值有基线依据 | PASS | 对照表入专档 §1.1（基线 3m53s/1m9s/19s/32s = RELEASE PHASE5 :18；帽 45/20/15/15 = D1① 声明面原值；倍率 11.6×/17.4×/47.4×/28.1× 全宽松方向）；`git diff ci.yml` 零测试面词形反证 |
| 4 | selftest_wsw7 收口回归 | PASS | `RP_WSW_BIN=/home/emo/rt3-target/debug/rp-wsw bash apps/server/selftest_wsw7.sh` → **PASS=34 SKIP=4 FAIL=0** + `SELFTEST_WSW7: ALL GREEN`（exit 0；SKIP = nuclei/zap e2e 等环境实态分开计数）；三重钉行回显 `[ok] Cargo.lock diff 收口后恒 0 = 0` / `[ok] Cargo.lock diff 无其它行（收口后恒 0） = 0` / `[ok] Cargo.lock [[package]] 417 恒定 = 417`；`git diff` 只含 :285-294 块声明行 |
| 5 | selftest_wsw4 收口回归 | PASS | `bash apps/server/selftest_wsw4.sh` → **PASS=34 FAIL=0** + `SELFTEST_WSW4: ALL GREEN`（exit 0）；`git diff` = :39 单行 |
| 6 | selftest_wsw56 零改动回归 | PASS | 文件零 diff（不在改动清单）；`RP_WSW_BIN=... bash apps/server/selftest_wsw56.sh` → **PASS=42 SKIP=0 FAIL=0** + `SELFTEST_WSW56: ALL GREEN`（exit 0——42/0 基线精确延续） |
| 7 | release build 面 | PASS | `time CARGO_TARGET_DIR=/home/emo/rt3-target cargo build --release --offline --workspace` → exit 0，**real 1m46.129s**（user 16m23s），211 crate 重编译（工具链路径切换部分失效——rustc -vV 两目录逐字节一致）；release du **2.1G**（前 1.1G）；`git diff Cargo.toml Cargo.lock vendor/ | wc -l` = **0**；417 / reqwest=1 恒定 |
| 8 | release 终钉对账 | PASS（程序注） | 构建前 sha256sum = `a226fb56…`（PIN 预置态）；全量构建后漂移 `228e4a9a…`（坏消息 1 归因闭环）；stable 路径复跑 manifest 注册命令（9.24s）→ **`a226fb56…` 字节级复现**；收口态 sha256sum = manifest 孪生 binary_sha256 一致（判据 8 词形 PASS）；manifest 零触碰（禁回钉词形成立） |
| 9 | containerfiles 静态核验表 | PASS | 11 族逐族行入专档 §3.1（FROM digest / 文件集 / manifest 孪生 / 台账三处一致）；rip-zap `@sha256:000…000` 64 零占位在案（:32）；e2e 常量 podman_host_e2e.rs:36 `MANIFEST_DIGEST = 1dbc67084d…` 零 diff |
| 10 | containerfiles 真实构建面 | PASS | 五族 `podman build -t localhost/<family>:rt7-verify …` **exit 0 ×5**（2.927/2.608/2.644/2.635/0.294s，构建日志零 Pulling 零包安装——零出网面）；`podman rmi` ×5 exit 0；after `podman system df` = before 逐项一致（17 images 8.198GB / 5 containers / 13 volumes）+ tag 清单同列 11 条——零残留零登记漂移 |
| 11 | workspace 全量与静态门零红 | PASS | 收口门 `cargo test --offline --workspace --no-fail-fast` → **1337 passed / 0 failed / 2 ignored**（TEST_EXIT=0——基线零红延续）；`cargo clippy --offline --workspace --all-targets -- -D warnings` → CLIPPY_EXIT=0；`cargo fmt --all -- --check` → **120 文件 / 936 hunks** 精确持平存量红（零扩大——D4；7 交集文件全在：migration_idempotent 1 / caller_auth 9 / exchange_persist 17 / fuzz_job 41（全仓最高）/ sender 18 / diff 7 / fuzz/persist 3 hunks）；`cargo deny check advisories` → `advisories ok` exit 0 + `cargo deny check licenses` → `licenses ok` exit 0 |
| 12 | 零迁移零新依赖与收口 | PASS | migrations/001-030 零 diff；生产库五文件 mode=ro 指纹逐文件回显 = **1/1/1/0/0**（`for f in …; do sqlite3 "file:$f?mode=ro" "PRAGMA user_version"; done`——selftest_wsw56.sh :623-628 先例词形；任务书 §5 注释「逐文件 30」为侦察侧词形误差，见坏消息 3）；`git status --porcelain` = 8 行（§2.1 落点 7 + 任务书 1，见 §5）；Cargo.lock 417 / reqwest=1 恒定；vendor/ 零 diff |

## §3 build / 容器核验实测数字（专档详表摘录）

- release build：exit 0 / wall **1m46.129s** / release du **2.1G**（构建前 1.1G）/ 211 crate 重编译 / `[profile.release]` 零触碰。
- 终钉对账：pre `a226fb56…` → post-build `228e4a9a…`（漂移）→ stable 路径复跑注册命令 9.24s → `a226fb56…`（字节复现，PIN_MATCH 收口态）。
- 容器五族：exit 0 ×5（wall 2.927/2.608/2.644/2.635/0.294s）+ 构建期零出网（FROM 本地 + COPY + RUN mkdir/chmod）+ rmi 即删 + before/after 清点逐项一致（df 三行 + tag 清单 11 条）。
- 基座：`podman pull python:3.12-slim@sha256:4840d514…a54` exit 0（store 计数 17→17 零漂移——digest 形态在位去重）；nuclei 基座在位反证（935MB）。
- 磁盘快照（构建前）：`/media/emo` 477G 总 / **279G 可用**（42% 用——无近满盘风险）。

## §4 owner 会签等效声明专节（D2③——三件在案预授权词形合订，禁静默修改）

本批按 D2 收口 selftest 四钉（wsw7 三 ck 行 + wsw4 一行）。owner 会签等效 = 以下三件在案预授权词形合订（原文取证 2026-11-23，行号按词形重定位）：

1. **RT-5-6-BATCH-REPORT.md :220（观察池⑧，本批新增项）**（任务书引 :218，行漂 2 行词形同）：
   > ⑧ **本批新增**：selftest_wsw7.sh :292-293 收口后陈旧钉修复批（「Cargo.lock diff=3 行」检查改为条件化或按收口态重钉；**须 WSW-7 owner 会签**——与 selftest_wsw4 28 钉同族） | §8-4 承接

   配套证据链：RT-5-6-BATCH-REPORT.md :205（2 FAIL 实测：`git diff Cargo.lock` 检查已随 WSW-7 收口 `7a82804a` 落地恒 0，HEAD 纯净基线同败；红线 6 禁顺手改——登记观察池⑧留独立修复批）。

2. **WSW-8-BATCH-REPORT.md :166（WSW-7 产物 owner 视角预授权词形）**：
   > 4. **selftest_wsw7.sh 零改动回归 = 4 FAIL（全部为 D1 声明的基线跃迁面）**：……四钉均为 D1 明文声明的新基线词形覆盖（selftest_wsw4.sh 28 陈旧钉已知红同款词形：**测试钉值零改动维持，禁顺手改**——**收口后 417 为新基线，wsw7 四钉随下一链尾演进批随更，观察池登记**）……

   即：417 新基线钉与 wsw7 四钉随更 = WSW-8 报告明文预授权；本批 = 该句指向的「下一链尾演进批」。
3. **RELEASE_2026-11-17_RIP3-PHASE5.md :63 §5（钉值同步随收口批惯例先例）**：
   > ## 5. 本收口批：selftest 全集四处预存钉值同步（坏消息单列 + 教训）

三件合订 → 本批 D2 收口（钉值同步非测试逻辑改写；actual 均为既有事实——:292/:293 干净树实测 0、:294 实测 417、wsw4 :39 实测 30）= 预授权范围之内，非静默修改；边界守住：wsw7 除 :285-294 块外零触碰（:92 digest 钉 / :296 CROSS 钉 / 汇总逻辑零 diff 实测），wsw4 除 :39 外零触碰，其余 selftest_*.sh 零 diff（wsw56 42/0 零改动回归实证）。

## §5 §5 任务书命令实跑回显汇总（exit code + 关键行）

| 命令 | 结果 |
|---|---|
| `git status --porcelain \| wc -l`（S1） | 1（= 任务书自身未跟踪行） |
| `git log --oneline -1` | `dca07555 WSW-8 Browser Evidence 收口：chromiumoxide 0.9.1 vendor 基线跃迁（399→417）…` |
| `grep -cE '^\[\[package\]\]' Cargo.lock` | 417 |
| `grep -c 'name = "reqwest"' Cargo.lock` | 1 |
| `ls migrations/*.sql \| wc -l` | 30 |
| `sqlite3 "file:migrations/dev-b1.db?mode=ro" "PRAGMA user_version"` | 30 |
| `CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast`（基线） | 1337 passed / 0 failed / 2 ignored，EXIT=0 |
| `podman system df` + `podman images`（before） | 17 images（10 活跃）8.198GB / 5 containers 11.74GB / 13 volumes 568MB；tag 清单 11 条 |
| `python3 -c "import yaml; …"` | `YAML_OK`（exit 0） |
| `grep -c "timeout-minutes" ci.yml` | 4 |
| `grep -n "uses: …" ci.yml` | 10 行 ref 逐处对账（v5/v6/v5/v4 + 1.99.1 ×2） |
| `grep -n "channel" rust-toolchain.toml && rustc --version` | `channel = "1.99.1"` + `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |
| `RP_WSW_BIN=… bash apps/server/selftest_wsw7.sh` | PASS=34 SKIP=4 FAIL=0，ALL GREEN |
| `bash apps/server/selftest_wsw4.sh` | PASS=34 FAIL=0，ALL GREEN |
| `RP_WSW_BIN=… bash apps/server/selftest_wsw56.sh` | PASS=42 SKIP=0 FAIL=0，ALL GREEN |
| `time … cargo build --release --offline --workspace` | exit 0，real 1m46.129s |
| `sha256sum /home/emo/rt3-target/release/rp-wsw-browser`（收口态） | `a226fb56c8061eb16bf739f3213705fd8e2b32d4977c11b7ee97e8274f0a564e`（= manifest 孪生） |
| `git diff Cargo.toml Cargo.lock vendor/ \| wc -l` | 0 |
| `podman pull python:3.12-slim@sha256:4840d514…` | exit 0（store 计数 17→17） |
| `podman build -t localhost/<family>:rt7-verify …` ×5 | exit 0 ×5 |
| `podman rmi …rt7-verify` ×5 | exit 0 |
| `podman system df && podman images`（after） | 与 before 逐项一致 |
| `for f in …五库…; do sqlite3 "file:$f?mode=ro" "PRAGMA user_version"; done` | 1 / 1 / 1 / 0 / 0（逐文件） |
| `cargo fmt --all -- --check`（计数） | exit 1（存量红预期），120 文件 / 936 hunks |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | CLIPPY_EXIT=0 |
| `cargo deny check advisories && cargo deny check licenses` | `advisories ok` + `licenses ok`，双 exit 0 |
| `git status --porcelain -- crates/ migrations/ vendor/ Cargo.lock Cargo.toml web/ workers/ integrations/rtis/ \| wc -l` | **0** |
| `git status --porcelain \| wc -l`（收口） | **8**（= §2.1 落点 7 行 + RT-7-TASK.md 1 行批外未跟踪） |
| `for i in 1 2 3 4 5; do gh run list --limit 3 && break \|\| echo "EOF retry $i"; sleep 10; done` | 第 1 次 EOF（`failed to get runs: Get "https://api.github.com/...": EOF`）→ `EOF retry 1` → 第 2 次成功列出 3 run（WSW-8 收口 run 45061902514 = 9m32s success 等）——A5(d) 重试词形实测成立 |

**纪律注**：环境纪律「禁 git commit / git add / git push」执行到位——任务书 S6「显式 add 本批清单」步骤未执行（红线 8 词形与环境纪律冲突时从严），显式清单 = §1 表 7 文件，留主会话审查后自行 add/commit。

## §6 坏消息单列

1. **工具链安装路径 sha 敏感性（新实证，主会话裁量项）**：rustup `1.99.1-x86_64-unknown-linux-gnu`（版本化目录）与 `stable-x86_64-unknown-linux-gnu` 为同版本两物理安装（`rustc -vV` 逐字节一致）；rust-toolchain.toml 落地后首次全量 release 构建触发 211 crate 部分重建 → `rp-wsw-browser` sha256 由 `a226fb56…`（WSW-8 终钉）漂移至 `228e4a9a…`。处置：以 `RUSTUP_TOOLCHAIN=stable-x86_64-unknown-linux-gnu cargo build --offline --release -p wsw-runtime --bin rp-wsw-browser`（= manifest provenance 注册命令原形）复跑 → 9.24s → **`a226fb56…` 字节级复现**，PIN_MATCH 收口态恢复，manifest 零触碰。归因结论：**同工具链路径重建 = 字节可复现**（漂移面 = 工具链安装路径切换，非编译器版本、非源码）；WSW-8 manifest known_limits「release 重建即漂移」词形适用面应收窄为「跨路径/跨版本重建漂移」——该补注属 WSW-8 产物面，本批禁改（禁回钉禁重登记词形遵守），留主会话裁量。**运维含义**：rust-toolchain.toml 在位时，默认工具链全量 release 构建必漂移终钉；终钉恢复 = stable 路径复跑注册命令。
2. **GitHub API 间歇 EOF 实测复现**：`gh run list` 第 1 次 `Get "https://api.github.com/...": EOF`，retry 循环第 2 次成功——A5(d) 预测成立；CI watch 收口流程必须带重试（词形已实测入 §5 表尾行）。
3. **生产库指纹词形勘误（任务书侦察侧词形误差，如实登记）**：任务书 §5 注释「逐文件 30」与判据 12 字面——实测五库 = **1/1/1/0/0**（platform.db 水位 1 = 未触碰基线；selftest_wsw56.sh :623-628 先例与 selftest_wsw7.sh :282 钉值词形均以 1 为基线——「30」属 dev-b1.db（实测 30）与 migrations/*.sql 计数（30）面）。判据 12 操作词形 = 逐文件 mode=ro 回显（已执行），判据状态 PASS 以先例词形为准。
4. **action ref 升级后 CI run 未实录**：本批零 push（红线 8），升级后四 job 绿 run 数字待主会话 push 后回填（RELEASE 四.二节已留回填位；观察池⑦登记复测）。
5. **任务书行号漂移 3 处（词形重定位，无实质影响）**：RT-5-6 观察池⑧ 实际 :220（任务书引 :218）；RT-7-TASK.md §5 基线记录「fmt 120 文件」初测因 sed 抽取误差得 930，全路径去重重算 = **120**（与 A9 一致）；WSW-8 :166 引用词形精确命中。

## §7 未解决项（本批零实现，全部观察池登记）

1. **fmt 全仓收敛独立 chore 批**：120 文件 / 936 hunks 存量红精确持平零扩大（D4）；链尾 4 交集（migration_idempotent / caller_auth / exchange_persist / fuzz_job 41 hunks 全仓最高）+ 跨轨 3 交集（sender / diff / fuzz/persist）全在——独立评审窗口禁夹带（RT-5/6 D3 词形延续）。
2. **deps 层旧 fingerprint 判定批**（RT-5/6 观察池③承接）。
3. **btrfs balance 执行**（RT-5/6 观察池④承接——sudo 步骤用户裁量）。
4. **镜像族告警阈值定稿**（RT-5/6 观察池⑤承接——11 族台账 + 本批 §3.1 核验表为输入）。
5. **ZAP 镜像面复测**（WSW-7 选项 B——rip-zap 64 零占位核验行承接）。
6. **WSW-9 性能/可靠性验收批**（WSW-8 §7.1 移交面——本批零触碰确认：browser 并发池/实例复用池/Windows 词表/性能基线 1372ms 冷 / 1188ms 热均未动）。
7. **action ref 升级后 CI 稳态复测**：主会话 push 后 watch（EOF 重试词形已实测）；deprecation 警告零残留复核；四 job run 数字回填 RELEASE 四.二节。
8. **manifest known_limits 词形补注裁量**（坏消息 1——「release 重建即漂移」收窄为跨路径/跨版本面；WSW-8 产物面，本批禁改）。

## §8 WSW-9 + RELEASE 前置输出确认

- RELEASE 五节脊柱输入面 = 专档第四节（批次汇总/门禁数字/交付总账/已知取舍/回滚方式 + 钉值同步清单四钉表）——RELEASE 立档直接引用；本批禁替 RELEASE 立档、docs/releases/ 34 份零 diff 实测。
- WSW-9 交接零新增声明：性能基线/并发池/复用池/Windows 词表 = WSW-8 §7.1 移交面零触碰（A14 对照）；本批落点全在 CI 配置/selftest 钉值/文档面，wsw-runtime src 零 diff。
- 034 不立项确认：本批全零 schema 面（migrations 001-030 零 diff + 五库指纹回显 + 链尾十六钉零随更）；编号登记 = 031 WSW-7/WSW-8 双占 + 032 RT-5/6 + 033 空缺（勘误遗留）→ RT-7 占 034 位且零迁移（D5）。

## §9 八红线逐条守住反证

| # | 红线 | 反证 |
|---|---|---|
| 1 | CI 质量门零缩减 | `git diff ci.yml` 两类词形（timeout + ref），测试/clippy/parity/lock/npm 五步逐行零删改；帽值 = D1① 声明面全宽松方向 |
| 2 | CI 面禁绝对钉 | ci.yml 零 Cargo.lock 步骤零新增（lock 面维持 RT-2 唯一步骤）；417 口径只进判据与 selftest 重钉 |
| 3 | Rust 面零 diff | `git diff Cargo.toml Cargo.lock vendor/` = 0；crates/** 零 diff；rust-toolchain.toml 不进 lock（417 恒定）；[profile.release] :117-119 零触碰 |
| 4 | selftest 收口边界 | diff 只含 wsw7 :285-294 块 + wsw4 :39 单行；会签声明 = §4 专节（禁静默词形遵守） |
| 5 | 生产库零触碰 + 034 不立项 | 五库 mode=ro 指纹回显 1/1/1/0/0；migrations 零 diff；零新表零迁移 |
| 6 | 容器面边界 | 只建 rt7-verify 标签 + rmi 即删；登记面三处 digest 零改动；受控拉取仅 python:3.12-slim@4840d514 一面；before/after 清点逐项一致；禁 pull -a / 禁 prune 遵守；宿主零安装 |
| 7 | fmt chore 不并入 | 120/936 零扩大；crates/** 零 diff；本批落点全不入 fmt 面 |
| 8 | 边界纪律 | docs/releases/ 34 份零 diff；WSW-9 面零触碰；api EOF 重试 = 流程词形不进 yml；零 commit / 零 add / 零 push（S6 add 步骤按环境纪律跳过——§5 纪律注） |
