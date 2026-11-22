# RT-7 批任务书 · CI 与发布策略核验（CI 修固 + 发布矩阵三面实测 + selftest 陈旧钉收口；零迁移零新依赖）

- 批次：RT-7（NATIVE-FIRST §九 :229「RT-7 CI 与发布策略核验：原生构建矩阵、完整测试/Clippy 保持、容器工具仅在需要时构建」验收 :242「GitHub CI 完整 Workspace 测试与 Clippy 保持原有范围」；RELEASE 前最后一个 RT 批）
- 日期：2026-11-23
- 状态：任务书已定稿（2026-11-23 侦察后落稿；实施按本任务书执行；禁 git add/commit/push，工作树留主会话审查收口）
- 依据：[NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md) :229（RT-7 交付定义 = 原生构建矩阵 / 完整测试与 Clippy 保持 / 容器工具仅在需要时构建）/:242（GitHub CI 完整 Workspace 测试与 Clippy 保持原有范围 = 不缩小现有质量门）/:233-241（验收标准逐条——不缩小质量门、生产 `platform*.db` 零误触）+ [containerfiles/README.md](../../containerfiles/README.md)（digest 固定规则两节 / 缓存策略「Adapter 只探不拉」/ 构建与验证命令词形）+ [RT-5-6-TASK.md](RT-5-6-TASK.md)（结构先例 + D3 fmt 独立批词形）+ [RT-5-6-BATCH-REPORT.md](RT-5-6-BATCH-REPORT.md) :205/:218（selftest_wsw7 :292-293 陈旧钉 2 FAIL 实测 + 观察池⑧「条件化或按收口态重钉；须 WSW-7 owner 会签」）+ [WSW-8-BATCH-REPORT.md](../rip3/wsw/WSW-8-BATCH-REPORT.md) :81/:108/:166（收口时 4 FAIL 全部 = D1 声明基线跃迁面 + **「收口后 417 为新基线，wsw7 四钉随下一链尾演进批随更，观察池登记」= owner 预授权词形**）/:168（镜像 digest 漂移实录）+ [WSW-8-BATCH-REPORT.md](../rip3/wsw/WSW-8-BATCH-REPORT.md) §7.4 :202-203（后续批门禁：lock 对比以 HEAD 为准 `git show HEAD:Cargo.lock | grep -cE '^\[\[package\]\]'` + reqwest 双检 + deny 双检 exit 0）+ [RELEASE_2026-11-17_RIP3-PHASE5.md](../releases/RELEASE_2026-11-17_RIP3-PHASE5.md) :18（CI 四 job 全绿基线数字：Rust workspace 3m53s / parity 1m9s / Web 32s / Python 19s）/:63 §5（「selftest 全集四处预存钉值同步」先例 = 钉值同步随收口批纪律）+ [PHASE-ROADMAP.md](PHASE-ROADMAP.md)（RT-7 排程词形）
- 前置：WSW-8 已收口（HEAD `dca07555`，`git status --porcelain` = 0 行实测 2026-11-23）；RT-5/6 已收口（`f8d38107` 在链）；workspace 基线 = WSW-8 收口 1337 passed 零红（WSW-8-BATCH-REPORT.md :108「185 binary，1337 passed / 0 failed」）；Cargo.lock 新基线 417 / reqwest=1（chromiumoxide 0.13.5 传递——WSW-8 §7.4 台账）
- 设计裁决：本批特有裁决见 §0 D1-D6（**不另立 ADR**——RT-5/6-TASK 先例，裁决内嵌本档）
- 现状锚点：§1 A1-A14 均为 2026-11-23 本会话实测（HEAD `dca07555`、工作树 0 行、Cargo.lock 417、migrations 30 实测），实施时若行号漂移按词形重定位并以本档词形为准

---

## 0. 设计裁决（本批特有，ADR 内嵌）

| # | 裁决 | 理由 |
|---|---|---|
| D1 | **CI 修固面清单（本批四项进，三项登记不进）**：①**timeout-minutes 补齐**（现状 0 处——grep 实测；GitHub 默认 job 上限 360min 形同虚设）：四 job 各加一处帽——rust 45 / parity 20 / python-workers 15 / web 15（基线 = RELEASE PHASE5 :18 实测 3m53s/1m9s/19s/32s，帽 = 基线 6-10 倍余量；**只加帽不改测试面**——「不为验收提速调高」纪律 = 帽值只为挡悬挂不缩门）；②**Rust 工具链钉定**：新建 `rust-toolchain.toml`（channel = 实施时点 `rustc --version` 实测串，本会话基线 1.99.1；components = ["clippy"] 镜像 ci.yml:40 现状词形）+ ci.yml 两处 `dtolnay/rust-toolchain@stable`（:38/:71）→ `@<同版本串>`——**本地零下载零行为变化**（rustup stable active = 1.99.1 同版实测，rust-toolchain.toml 解析同版本）；③**actions Node 20 deprecation 升级**：checkout@v4→v5（:35/:68/:92/:117 四处）/ setup-python@v5→v6（:95）/ setup-node@v4→v5（:120）/ cache 维持 v4（实施时点以官方 release 核验逐个填入；EOF 网络不稳时重试获取——A11）；④**lock 步骤零新增**：ci.yml lock 面 = 维持 RT-2 Python lock 静态校验唯一步骤（ci.yml:102-104）——**Cargo.lock 绝对钉禁进 ci.yml**（399 陈旧钉教训：绝对数钉随基线演进恒红），vendor↔lock 一致性已由 `cargo test --offline` 构建隐式核验（vendored-sources 缺 crate 即 build fail），lock 新口径 417 = 批门禁词形进本任务书判据与 selftest 重钉（D2），不进 yml。**登记不进**：python patch 浮动维持 "3.12" minor 钉（:97——CI Python 面 = 纯 stdlib unittest，13 测试文件零 numpy/onnxruntime import 实测，patch 漂移无 ABI 面）；concurrency/permissions/触发面零改动（:14-25 已最小权限）；api EOF 重试进流程词形不进 yml（GitHub API 间歇 EOF = 本会话 `gh run list` "Get https://api.github.com/...: EOF" 实测——S6 CI watch 收口命令带重试循环） | :242「保持原有范围」= 只加固不缩门不扩门；timeout/钉版/action 升级全部纯 YAML 面零 Rust diff；绝对钉教训自证 = selftest_wsw7 :294 399 钉在 WSW-8 落地次日即成陈旧红（A9）——CI 面禁重蹈；dtolnay@stable 浮动 = CI 工具链与本地基线漂移风险（本地 1.99.1 2026-10-13 面世后 stable 必再前进） |
| D2 | **selftest 陈旧钉收口（owner 会签等效声明）**：①**selftest_wsw7.sh 三钉收口**（apps/server/selftest_wsw7.sh:285-294 块，本会话实测三钉全红）：:292/:293「Cargo.lock diff = wsw-runtime 依赖块 3 行」双钉 → 收口态断言改写 = 干净树 `git diff Cargo.lock` 双检期望 3→0（label 词形同步「收口后恒 0」）；:294 `[[package]] 399` → **417 新基线钉**（WSW-8 §7.4 口径）+ 行注「随链尾演进批同步——WSW-8 报告 :166 词形」；:296 CROSS 跨轨零 diff 钉零触碰（干净树 0 = 过，实测）；②**selftest_wsw4.sh :39 单行钉值同步**：`user_version=28` → `30`（链头实测——dev-b1.db user_version=30 + migrations/*.sql=30 双证）+ 行注同词形；③**owner 会签等效声明** = 三件在案预授权词形合订入批次报告：RT-5-6-BATCH-REPORT :218 观察池⑧（「条件化或按收口态重钉；须 WSW-7 owner 会签」立项条件）+ WSW-8-BATCH-REPORT :166（WSW-7 owner 视角明文「417 为新基线，wsw7 四钉随下一链尾演进批随更，观察池登记」= 预授权词形）+ RELEASE_2026-11-17_RIP3-PHASE5.md :63 §5 先例（「selftest 全集四处预存钉值同步」= 钉值同步随收口批惯例）；④**边界**：只动 :285-294 块三 ck 行与 wsw4 :39 单行——selftest 其余钉值零触碰、测试逻辑零重构（钉值同步非改测试，A9 实测三钉 expected 值全部落后于既有收口态，actual 均为既有事实） | RT-5/6 期 2 FAIL（:205 实测）+ WSW-8 落地后 [[package]] 399 钉再红（417≠399）= 三钉现状恒红，与「零改动回归 exit 0」收口判据直接冲突；RT-7 = CI/发布矩阵核验批，selftest 回归绿是本批判据面——钉值同步主题契合；三件预授权在案 = owner 会签等效（本批报告坏消息单列 + 合订声明专列，禁静默修改） |
| D3 | **发布矩阵核验形状 = build 三面 + containerfiles 双面 + RELEASE 惯例对账**：①**release build 三面**：(a) `CARGO_TARGET_DIR=/home/emo/rt3-target cargo build --release --offline --workspace` 全量一次（time + du 实测入档；[profile.release] strip="debuginfo" Cargo.toml:117-119 唯一 profile 段**零触碰**——RT-5/6 D1④ release 面无数据不验证词形延续）；(b) release 二进制登记面对账：WSW-8 终钉 rp-wsw 0.1.4 + binary_sha256 双文件一致（WSW-8-BATCH-REPORT :170「终态 = 0.1.4 + a226fb56 双文件一致 PIN_MATCH 实测」）——本批 sha256sum 复算对账 manifest 孪生（manifests/tools/rip.wsw.browser.json/yaml）；(c) **vendor 与发布物关系裁决**：vendor/ 随 git 树入库（git ls-files 20009 文件 / 387M du 实测）= 发布源树成分，`.cargo/config.toml` vendored-sources 四行 = 离线构建唯一入口；发布物二进制不含 vendor（链接面零目录依赖——可执行文件自含）；镜像属运行时数据不入库（containerfiles/README 头词形）；②**containerfiles 双面**：(a) 静态核验面 = 全 11 族零网络——FROM digest 真钉 10/11（python:3.12-slim@sha256:4840d514… 八族 + nuclei@sha256:582d5546… + paradedb pg17@sha256:7cb29053… 双段）+ rip-zap@sha256:000…000 64 零占位在案（WSW-7 D1 选项 B）逐族核验表 + 每族文件集在位（Containerfile + 工具脚本 + manifest 孪生 json/yaml）+ digest 台账↔tool-manifest↔e2e 常量三处一致 grep 对账（podman_host_e2e.rs:36 MANIFEST_DIGEST = 1dbc67084d… 钉 rip-tool-base:smoke）；(b) 真实构建面 = **五族**（rip-tool-base:smoke / rip-tool-coreutils / rip-tool-webresearch / rip-tool-source + rip-nuclei——判据 = 构建期零外联族：前四族基座 `python:3.12-slim@4840d514…` 受控拉取一次后纯标准库零包安装（README P3-1 构建口径词形），rip-nuclei 基座 `docker.io/projectdiscovery/nuclei:latest` 935MB 已在本地 store 实测）；构建 = **`rt7-verify` 验证标签**（`podman build -t localhost/<family>:rt7-verify`）→ exit 0 实测 → **`podman rmi` 验证标签即删**——登记面三处 digest 零改动（README 明词形「本地构建的 digest 不可跨主机字节级复现」——重建必漂移，本批是核验不是重建登记）；其余六族（rip-gov/rip-embed/rip-rerank/rip-parser/rip-pg17/rip-zap）= 静态核验 + 重建路径登记（gov 构建期 pypi datasketch 拉取面 / embed·rerank·parser 模型烘焙 1.1-3.4GB 面 / pg17 多阶段编译面 / zap 选项 B 未实拉面——containerfiles/README 台账实录逐族在案）；③**RELEASE 惯例对账**：docs/releases/ 34 份总账五节脊柱实测同构（一批次汇总/提交链 → 二验收对账/门禁数字 → 三交付总账 → 四已知取舍/坏消息 → 五回滚方式，+ 队列/入口附加节——RELEASE_2026-11-15_RIP3-PHASE1/P2/P4、RELEASE_2026-11-14_PHASE10 六-七节同构实测）→ 本批报告输出 **RELEASE 对账表输入面专节**（§8⑥） | :229「原生构建矩阵」交付面 = 真实 build 实测非纸面对账；「容器工具仅在需要时构建」词形 = 五族零外联面即"需要时"最小集合，其余六族登记路径不重建（模型烘焙/多阶段编译 = 部署面非核验面）；rt7-verify 即删 = 核验零登记漂移（重建必漂移 + e2e SKIP 守卫 podman_host_e2e.rs:69 在案——镜像缺位 workspace 仍绿 WSW-8 收口 1337 实证） |
| D4 | **fmt chore 不并入本批（RT-5/6 D3 裁决延续，实测数字更新）**：`cargo fmt --all -- --check` @ HEAD `dca07555` = **120 文件 / 936 diff hunks**（RT-5/6 基线 119/935，+1 文件 +1 hunk = WSW-8 browser.rs 新增面；实测 /tmp/fmt_full.txt 口径）；**链尾十六钉交集 4 文件仍在**（migration_idempotent.rs / caller_auth.rs / exchange_persist.rs / fuzz_job.rs——逐文件 grep 实测在 dirty 清单，fuzz_job.rs 41 hunks = 全仓最高脏面）+ **跨轨禁改面 3 文件仍在**（wsw-runtime/src/sender.rs / wsw-message/src/diff.rs / wsw-runtime/src/fuzz/persist.rs 逐一在清单实测）→ 全仓 fmt = 触碰链尾硬钉重排，裁决与 RT-5/6-TASK D3 :19 词形逐字延续：本批零 Rust src 改动零 fmt 触碰（判据 = fmt 计数 ≤120/936 零扩大），fmt 全仓收敛维持独立 chore 批登记 | 本批落点 = ci.yml/rust-toolchain.toml/selftest 两 sh/两份 md——全部不入 fmt 面（shell/doc 非 rustfmt 面）；7 交集文件全在 = 独立批前置条件未变 |
| D5 | **034 不立项（零迁移）+ 零新依赖**：本批落点 = ci.yml + rust-toolchain.toml + selftest 两 sh 钉值 + 两份 md——全部零数据库面；migrations/001-030 零 diff + dev-b1.db user_version=30 恒定 + 链尾十六钉零随更；**零新依赖**：rust-toolchain.toml = rustup 工具链选择文件非 crate 面（不进 Cargo.lock；components clippy 本地已在位零下载）；ci.yml action 版本 = GitHub 托管 action ref 面，零 npm/零 cargo dep；Cargo.lock 417 恒定零 diff + vendor/ 零 diff。**编号登记**：031 = WSW-7/WSW-8 双占（WSW-8-TASK D5 :21 编号勘误在案——启动词 033 勘误回 031）+ 032 = RT-5/6（RT-5-6-TASK D5 :21）+ 033 空缺（WSW-8 启动词勘误遗留）→ **本批 = 034 编号位，034 不立项 = 零迁移结论**；若实施坐实需新表 → 停批上报重新立项禁临场加迁移（028 先例） | 迁移立项判据 = 需要新表/新列/新索引；CI 修固与发布矩阵核验全零 schema 面；编号漂移如实登记避免后续批误引（WSW-8-TASK D5 词形延续） |
| D6 | **WSW-9/RELEASE 交付边界（本批只产输入面不越界）**：WSW-9 = WSW-8 §7.1 明文移交（浏览器捕获性能基线 1372ms 冷/1188ms 热移交 + 并发池/复用池零实现观察池 + Windows 路径词表待补——WSW-8-BATCH-REPORT §7.1/:186/:191/:195）——本批零触碰；RELEASE = 下一步阶段总账立档（五节脊柱 D3③）——本批只产对账输入面专节（判据回显 + 数字表 + 钉值同步清单），禁替 RELEASE 立档、禁改 docs/releases/ 既有 34 份 | :229 交付定义 = 核验面；RELEASE 立档 = 主会话阶段收口动作非 RT 批动作；越界面登记观察池 |

## 1. 现状锚点（2026-11-23 本会话实测，A1-A14）

| 锚点 | 事实 | 出处 |
|---|---|---|
| A1 RT-7 原文定义 | :229「RT-7 \| CI 与发布策略核验 \| 原生构建矩阵、完整测试/Clippy 保持、容器工具仅在需要时构建 \| 不缩小现有质量门」+ :242「GitHub CI 完整 Workspace 测试与 Clippy 保持原有范围」+ :233-241 验收清单（不缩小质量门 / 生产 platform*.db 零误触 / 清理只作用归属 RIP 对象） | docs/phase0/NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md:229,233-241,242 |
| A2 CI 全清单（单 workflow 四 job） | .github/workflows/ 仅 ci.yml；触发 = push/PR→main（:14-18）；permissions contents:read（:20-21）；concurrency cancel-in-progress（:23-25）；jobs = rust（:28-55：checkout@v4 :35 → dtolnay/rust-toolchain@stable components clippy :38-40 → cache@v4 :43-49 → `cargo test --offline --workspace` :52 → `cargo clippy --offline --workspace --all-targets -- -D warnings` :55）+ parity（:61-83：rp-mcp-parity 双通道 :83）+ python-workers（:85-108：setup-python@v5 "3.12" :95-97 → lock 完整性校验 tests.test_lock_integrity :102-104 → unittest discover :106-108）+ web（:110-136：setup-node@v4 "22" cache npm :119-124 → npm ci :128 → build :132 → vitest :136）；**timeout-minutes = 0 处**（grep 实测） | .github/workflows/ci.yml:14-136 全文 + grep 实测 |
| A3 版本钉定面 | Rust：dtolnay/rust-toolchain@**stable 浮动**（:38/:71），rust-toolchain.toml 零在位（ls 实测）；本地 rustc/cargo 1.99.1（2026-10-13）+ rustup stable active default + 存量 1.81.0/1.90.0；Python：setup-python@v5 `python-version: "3.12"` minor 级钉（:95-97）；Node：setup-node@v4 `node-version: "22"` major 级钉（:119-122） | ci.yml:38,71,95-97,119-122；rustc --version / rustup toolchain list 实测 |
| A4 CI lock 步骤实际判什么（WSW-8 399→417 CI 恒绿根因） | ci.yml 唯一 lock 步骤 = `python -m unittest tests.test_lock_integrity`（:102-104）——校验对象 **workers/python/requirements.lock.txt**（uv pip compile --generate-hashes 产物，三判据 = 每包 ≥1 sha256 hash / requirements.in 三直接依赖钉定一致 / 无 `..` 穿越，workers/python/tests/test_lock_integrity.py:1-16）；**零 Cargo.lock 步骤**；CI 恒绿三因 = (a) vendor/ 随 git 入库（20009 文件——git ls-files 实测），(b) `cargo test --offline` 经 .cargo/config.toml vendored-sources（:1-4）构建隐式核验 vendor↔lock 一致，(c) WSW-8 零触碰 requirements.lock.txt | ci.yml:102-104；workers/python/tests/test_lock_integrity.py:1-16；.cargo/config.toml:1-4；git ls-files vendor/ \| wc -l = 20009 |
| A5 CI 基线绿与既有薄弱点 | 13+ 批 CI 绿口径 = RELEASE PHASE5 :18「P5-9c 收口 run 44183557099 四 job 全绿（Rust workspace 3m53s / 协议一致性载具 1m9s / Web 工作台 32s / Python worker 19s）」；薄弱点实测：(a) timeout 零帽（A2），(b) dtolnay@stable 浮动（A3），(c) actions Node 20 runtime 面（checkout@v4/cache@v4/setup-python@v5/setup-node@v4——deprecation 警告面，升级登记 D1③），(d) GitHub API 间歇 EOF（本会话 `gh run list` → "Get https://api.github.com/repos/.../actions/runs: EOF" 实测——CI watch 收口流程需重试词形），(e) lock 步骤零 Cargo.lock 面（A4——绝对钉禁进 yml 见 D1④） | docs/releases/RELEASE_2026-11-17_RIP3-PHASE5.md:18；gh run list EOF 实测（命令原文入报告） |
| A6 selftest_wsw7 陈旧钉（本批收口面） | apps/server/selftest_wsw7.sh:285-294 跨轨锁面块：:292 `LOCK_DIFF`（期望 3，干净树实测 **0**——diff 已随 WSW-7 收口 7a82804a 落地恒 0）→ FAIL；:293 `LOCK_DIFF_ALL`（期望 3，实测 **0**）→ FAIL；:294 `[[package]]` 期望 **399**，实测 **417**（WSW-8 chromiumoxide 跃迁）→ FAIL；:296 CROSS 跨轨零 diff（干净树 0 = 过）；RT-5/6 期 2 FAIL（:292/:293——RT-5-6-BATCH-REPORT :205 实测词形）；WSW-8 收口期 4 FAIL（第四钉 = :296 vendor 18 新目录——WSW-8-BATCH-REPORT :81/:108/:166 实测 + 明文「417 为新基线，四钉随下一链尾演进批随更」） | apps/server/selftest_wsw7.sh:285-294,296；实测 grep ×3；docs/phase0/RT-5-6-BATCH-REPORT.md:205,218；docs/rip3/wsw/WSW-8-BATCH-REPORT.md:81,108,166 |
| A7 selftest_wsw4 陈旧钉（本批收口面） | apps/server/selftest_wsw4.sh:39 `ck "迁移水位 user_version=28" "28" "$UV"`——actual = 30（脚本 cp migrations/dev-b1.db + db migrate 后 PRAGMA 实测；dev-b1.db 水位 = 30 + migrations/*.sql = 30 双证）；WSW-7/WSW-8 两批红线均「维持已知红禁顺手改」——本批 D2 收口 = 三件预授权词形合订（D2③） | apps/server/selftest_wsw4.sh:35-39；sqlite3 dev-b1.db PRAGMA 实测 = 30；docs/rip3/wsw/WSW-8-TASK.md:21,157 |
| A8 观察池在案词形（收口授权链） | RT-5-6-BATCH-REPORT §9 :218 观察池⑧「selftest_wsw7.sh :292-293 收口后陈旧钉修复批（『Cargo.lock diff=3 行』检查改为条件化或按收口态重钉；**须 WSW-7 owner 会签**——与 selftest_wsw4 28 钉同族）」+ WSW-8-BATCH-REPORT :166「**收口后 417 为新基线，wsw7 四钉随下一链尾演进批随更，观察池登记**」（WSW-7 产物 owner 视角预授权）+ :221 观察池词形「wsw4 28 陈旧钉 + wsw7 四基线钉随更」+ RELEASE PHASE5 :63 §5「selftest 全集四处预存钉值同步」先例 | docs/phase0/RT-5-6-BATCH-REPORT.md:218；docs/rip3/wsw/WSW-8-BATCH-REPORT.md:166,221；docs/releases/RELEASE_2026-11-17_RIP3-PHASE5.md:63 |
| A9 fmt 存量红（D4 实测面） | `cargo fmt --all -- --check` @ HEAD dca07555 = **120 文件 / 936 hunks**（RT-5/6 基线 119/935 +1——WSW-8 browser.rs 面；hunk 计数 = `grep -c "^Diff in"` 936，文件计数 = 去重 120）；**7 交集文件全在**：链尾 4（migration_idempotent.rs / caller_auth.rs / exchange_persist.rs / **fuzz_job.rs 41 hunks = 全仓最高**）+ 跨轨 3（sender.rs / diff.rs / fuzz/persist.rs）逐文件 grep 实测在 dirty 清单 | 本会话 cargo fmt 实测（文件清单入报告附录）；docs/phase0/RT-5-6-TASK.md:19,36；docs/phase0/RT-5-6-BATCH-REPORT.md:210 |
| A10 build 与 vendor 面 | 唯一 profile 段 = Cargo.toml:117-119（`[profile.release]` + 注 :118 + `strip = "debuginfo"` :119）；vendor/ = 387M / git ls-files 20009 文件；.cargo/config.toml vendored-sources 四行（:1-4）；rt3-target release 303M（RT-5/6 A5 沿革——本批 S4 重测）；WSW-8 release 终钉 = rp-wsw 0.1.4 + binary_sha256 a226fb56 双文件一致 + dev-b1.db 注册 0.1.4 superseded_previous=true（WSW-8-BATCH-REPORT :170） | Cargo.toml:117-119；du/git 实测；docs/rip3/wsw/WSW-8-BATCH-REPORT.md:170 |
| A11 containerfiles 11 族与 FROM 钉面 | 11 族 = rip-embed / rip-gov / rip-nuclei / rip-parser / rip-pg17 / rip-rerank / rip-tool-base / rip-tool-coreutils / rip-tool-source / rip-tool-webresearch / rip-zap（ls 实测）；FROM 真钉 10/11：python:3.12-slim@sha256:4840d514… 八族（embed :10 / gov :9 / parser :12 / rerank :10 / tool-base :14 / coreutils :17 / source :17 / webresearch :17）+ nuclei@sha256:582d5546…（rip-nuclei :19）+ paradedb pg17@sha256:7cb29053… 双段（rip-pg17 Dockerfile :18 builder / :34 runtime）+ **rip-zap@sha256:000…000 64 零占位**（rip-zap :32——WSW-7 D1 选项 B 未实拉在案）；digest 三处同步规则 + 台账 = containerfiles/README.md（§digest 固定规则 / §digest 台账——rip-tool-base:smoke sha256:1dbc67084d… 等 7 行）；e2e 钉常量 = crates/tool-engine/tests/podman_host_e2e.rs:36 `MANIFEST_DIGEST = 1dbc67084d…` | containerfiles/ 11 目录 ls + 逐族 FROM grep 实测；containerfiles/README.md；crates/tool-engine/tests/podman_host_e2e.rs:36,69 |
| A12 podman 面现状（本会话实测，构建可做面裁决输入） | `podman system df`：Images 17（10 活跃）8.198GB / Containers 5（2 活跃）11.74GB / Volumes 13（5 活跃）568MB；**riptool 五族 + gov/embed/rerank/parser 产物镜像已不在本地 store**（RT-5/6 治理后移除面——podman images 实测 tagged 清单仅 rip-nuclei 935MB / rip-pg17 1.13GB / pkgbuild-builder / ubuntu-zcode / alpine / python:<none> / debian:bookworm-slim / kali-rolling / 上游 nuclei 935MB + `<none>` 2）；**python:3.12-slim 基座不在 store**（grep = 0——八族构建需基座受控拉取一次）；nuclei 基座在位（935MB）；e2e SKIP 守卫在案（podman_host_e2e.rs:69「验收镜像不在本地镜像库」→ SKIP——镜像缺位 workspace 仍绿，WSW-8 收口 1337 passed 实证） | podman system df / podman images 实测（命令原文入报告）；crates/tool-engine/tests/podman_host_e2e.rs:69 |
| A13 RELEASE 总账惯例 | docs/releases/ = **34 份** .md（ls 实测——启动词 35 为笔误如实登记）；五节脊柱同构实测（RELEASE_2026-11-15_RIP3-PHASE1/P2/P4 + RELEASE_2026-11-14_PHASE10 六-七节：一 批次汇总/提交链 → 二 验收对账/门禁 → 三 交付总账 → 四 已知取舍 → 五 回滚方式 + 六 队列；PHASE5 九节扩展含 :63 §5 钉值同步专节先例）；RELEASE 对账输入 = 各批报告判据回显 + 门禁数字 | ls docs/releases/\*.md = 34；各 RELEASE 文档 `^## ` 头实测 |
| A14 WSW-9 移交面（本批零触碰边界） | WSW-8 §7.1 浏览器捕获性能基线（冷 1372ms / 热 1188-1218ms）移交 WSW-9 性能/可靠性验收批 + 并发池/实例复用池零实现观察池 + §7.3 Windows 路径词表待补 + §7.4 vendor 新基线台账（16 新包 + 门禁词形「lock 对比以 HEAD 为准」） | docs/rip3/wsw/WSW-8-BATCH-REPORT.md:186,191,195,202-203 |

## 2. 范围

### 2.1 允许改动（落点清单）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `.github/workflows/ci.yml` | 修改 | D1①③④：四 job 各加 `timeout-minutes`（rust 45 / parity 20 / python-workers 15 / web 15）；actions 版本 bump（checkout@v5 四处 / setup-python@v6 / setup-node@v5 / cache 维持 v4 或实施时点最新 patch——逐个以官方 release 核验）；`dtolnay/rust-toolchain@stable` → `@<版本串>` 两处（:38/:71）；**测试/clippy/lock/触发/权限/concurrency 面零改动** | S2 |
| `rust-toolchain.toml` | 新建 | D1②：`[toolchain] channel = "<版本串>"`（= ci.yml ref 同串 = 本地 `rustc --version` 实测串，本会话基线 1.99.1）+ `components = ["clippy"]`（镜像 ci.yml:40 现状词形）；两文件版本串一致性判据见 §4-2 | S2 |
| `apps/server/selftest_wsw7.sh` | 修改 | D2①：仅 :285-294 块——:292/:293 期望 3→0（label 词形「收口后恒 0」）+ :294 399→417 + 块注更新（收口态词形 + WSW-8 :166 出处注）；**其余全部 ck/sk 行零触碰** | S3 |
| `apps/server/selftest_wsw4.sh` | 修改 | D2②：仅 :39 单行——28→30 + 行注（随链尾演进批同步）；**其余零触碰** | S3 |
| `docs/phase0/RT-7-CI-RELEASE-VERIFICATION.md` | 新建 | D3：发布矩阵核验专档——CI 修固面清单（逐项 before/after）+ build 三面数字表（release 构建 time/du + 终钉对账 + vendor 关系裁决词形）+ containerfiles 双面（11 族静态核验表 + 5 族真实构建实测 + 6 族重建路径登记）+ RELEASE 五节对账输入面（D3③） | S6 |
| `docs/phase0/RT-7-BATCH-REPORT.md` | 新建 | 批次报告：判据 1-12 实测对照 + 坏消息单列（action 升级后的 CI run 实录 / EOF 重试实录 / 构建可做面偏差）+ **owner 会签等效声明专节**（D2③ 三件在案词形合订）+ 观察池登记 | S6 |
| `docs/README.md` | additive | 索引登记两份新档（RT-5/6 先例） | S6 |

### 2.2 不修改

- `Cargo.toml` / `Cargo.lock` / `vendor/` / `.cargo/config.toml` **零 diff**（417 恒定——D5；`[profile.release]` :117-119 零触碰——release 面无 A/B 数据不验证，RT-5/6 D1④ 词形）；`crates/**` 全部源码与测试零 diff（fmt 存量红 120 文件零触碰——D4；podman_host_e2e.rs:36 MANIFEST_DIGEST 零改动——D3②b）。
- `migrations/` 001-030 零 diff + 链尾十六钉零随更 + 生产库五文件（platform.db / platform-dryrun.db / platform-fullbatch.db / platform-governance.db / platform-search.db）全程 mode=ro 只读指纹（034 不立项——D5）。
- `web/**`、`integrations/rtis/**` 零 diff；`workers/python/**` 零 diff（requirements.lock.txt 零触碰——CI lock 步骤判据面零变化，A4）。
- ci.yml 既有测试面零缩减零扩面：`cargo test --offline --workspace` / clippy `-D warnings` / parity job / python lock 校验 / npm 五步——逐行保留（:242 不缩小现有质量门）；触发/permissions/concurrency 词形零改动。
- selftest_wsw7.sh 除 :285-294 块三 ck 行 + 块注外零触碰（:92 digest 钉 / :192 nuclei SKIP 词形 / :296 CROSS 钉 / 汇总逻辑零改）；selftest_wsw4.sh 除 :39 单行外零触碰；**其余全部 selftest_*.sh 零 diff**（selftest_wsw56.sh 42/0 零改动回归基准）。
- 宿主零安装零升级：rustup 已有 1.99.1 零下载（rust-toolchain.toml 解析同版本——D1②）；PyYAML 6.0.4 已在（yml lint 面——python3 -c import yaml 实测）；actionlint/yamllint 不新装（lint 面 = PyYAML parse + 词形 grep，零宿主新装红线）；零 sudo 命令。
- Podman 零清理零登记漂移：只建 `rt7-verify` 验证标签构建后 `podman rmi` 即删（D3②b）；既有 11 tagged 镜像 + 5 容器 + 13 卷零触碰反证（before/after 清点一致）；**禁 pull -a、禁全局 prune**；受控拉取仅限 `python:3.12-slim@sha256:4840d514…` 精确 digest 一面（构建五族前置——若实施时网络不可用则五族构建降级为静态核验 + 坏消息登记，静态面判据不豁免）。
- `docs/releases/` 34 份零 diff（D6）；WSW-9 面（browser 并发池/Windows 词表/性能验收）零触碰（A14）；fmt chore 批 / deps 判定批 / ZAP 选项 B 复测——观察池不进（D4/D6）。

## 3. 实施步骤（S1→S6 串行，每步过门再进）

1. **S1 门禁基线核验**：`git status --porcelain` = 0 + HEAD = `dca07555`；`grep -cE '^\[\[package\]\]' Cargo.lock` = 417 + `grep -c 'name = "reqwest"' Cargo.lock` = 1（WSW-8 §7.4 双检词形）；`ls migrations/*.sql | wc -l` = 30 + dev-b1.db user_version = 30；A1-A14 锚点词形复核（ci.yml 行号 / selftest 两钉行 / containerfiles FROM 串）；`rustc --version` 实测记版本串；**workspace 全量一次基线**：`CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast`（预期 1337 passed 零红延续——WSW-8 :108；如有红先停批上报）；`podman system df` + `podman images` before 快照入档；**门 = 基线数字表落报告底稿**。
2. **S2 CI 修固落点**：ci.yml 四处 timeout-minutes + action 版本 bump + dtolnay 版本钉（D1①③④）+ rust-toolchain.toml 新建（D1②，channel = S1 实测串）；lint 面：`python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` exit 0（PyYAML 6.0.4）+ 词形 grep 判据（§4-1/§4-2）；两文件版本串一致性 + `rust-toolchain.toml` 本地解析零下载反证（`rustc --version` 落点前后一致）；**门 = lint exit 0 + 判据 1/2 全过 + `git diff .github/workflows/ci.yml` 只含 D1 声明面**。
3. **S3 selftest 陈旧钉收口**：selftest_wsw7.sh :285-294 块改写（D2①——三 ck 行 expected 值与 label 词形 + 块注）+ selftest_wsw4.sh :39 单行（D2②）；修正后回归实测：`RP_WSW_BIN=/home/emo/rt3-target/debug/rp-wsw bash apps/server/selftest_wsw7.sh`（预期 FAIL=0——417 新钉 + 收口态双 0 + :296 CROSS 过；SKIP 面按镜像/环境实态分开计数）+ `bash apps/server/selftest_wsw4.sh`（预期 FAIL=0——30 钉）；selftest_wsw56.sh 零改动回归（42/0 基线）；**门 = 三套 selftest FAIL=0 + `git diff` 只含声明行**。
4. **S4 发布矩阵 build 三面**：(a) `time CARGO_TARGET_DIR=/home/emo/rt3-target cargo build --release --offline --workspace`（exit 0 + wall/du 实测）；(b) release 终钉对账：WSW-8 manifest 0.1.4 + binary_sha256 sha256sum 复算一致（manifests/tools/rip.wsw.browser.json/yaml 孪生）+ `[profile.release]` 零触碰反证（`git diff Cargo.toml` = 0）；(c) vendor 关系裁决词形落专档（D3①c——20009 文件/387M + 发布源树成分 / 二进制自含 / 镜像运行时数据不入库三段）；**门 = build exit 0 + 数字表落专档 + Cargo.toml/Cargo.lock 零 diff**。
5. **S5 containerfiles 双面核验**：静态面 = 11 族核验表（FROM digest 逐族 + 文件集 + manifest 孪生 + 台账↔manifest↔e2e 常量三处一致 grep——1dbc67084d… 词形）；真实构建面 = 五族 `rt7-verify` 标签（D3②b）：基座预置（python:3.12-slim@4840d514… 受控拉取一次 + nuclei 基座在位反证）→ 逐族 `podman build -t localhost/<family>:rt7-verify -f containerfiles/<family>/Containerfile containerfiles/<family>/` exit 0 实测 → `podman rmi` 即删 → after `podman images`/`podman system df` 与 before 一致（零登记漂移 + 零残留反证）；网络不可用降级路径 = 静态面照交 + 坏消息登记（§2.2 词形）；六族重建路径登记落专档；**门 = 核验表全 11 族落档 + 5 族 build exit 0（或降级登记）+ before/after 清点一致**。
6. **S6 文档与收口**：RT-7-CI-RELEASE-VERIFICATION.md（四节）+ RT-7-BATCH-REPORT.md（判据对照 + 坏消息 + **owner 会签等效声明专节** D2③ + 观察池）+ docs/README.md 索引；§5 全部命令实跑（workspace 全量一次零红 + clippy + deny 双检 + 生产库五库指纹 30）；CI watch 收口重试词形实测（`gh run list`/`gh run watch` EOF 时 retry 循环——A11(d) 流程面，命令原文入报告）；`git status --porcelain` 只含 §2.1 落点核对（crates/ / migrations/ / vendor/ / Cargo.lock / Cargo.toml / web/ / workers/ 零 diff 实证）；显式 add 本批清单（禁 add -A——本批只落文件不 commit）。

## 4. 完成标准（判据表；验收 = 命令实跑回显）

| # | 判据 | 验收 |
|---|---|---|
| 1 | **CI yml 修固面** | `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` exit 0；`grep -c "timeout-minutes" .github/workflows/ci.yml` = 4（每 job 一处，数字 = D1① 帽值）；action ref 版本串四类逐处 grep 对账（checkout/setup-python/setup-node/cache）；`git diff ci.yml` 词形 = timeout 行 + ref 行两类，零测试/clippy/触发面 diff |
| 2 | **工具链钉定一致性** | `rust-toolchain.toml` channel 串 = ci.yml dtolnay ref 串 = S1 `rustc --version` 实测串三处一致（grep 对账）；落点前后 `rustc --version` 不变（本地零下载零行为变化反证） |
| 3 | **timeout 帽值有基线依据** | RELEASE PHASE5 :18 四 job 实测数字（3m53s/1m9s/19s/32s）入专档对照表；帽值 = 基线 6-10 倍词形逐 job 对照；零测试面改动反证（diff 词形） |
| 4 | **selftest_wsw7 收口回归** | `RP_WSW_BIN=... bash apps/server/selftest_wsw7.sh` → FAIL=0（417 新钉 + 收口态双 0 + 跨轨 0）；`git diff apps/server/selftest_wsw7.sh` 只含 :285-294 块声明行 |
| 5 | **selftest_wsw4 收口回归** | `bash apps/server/selftest_wsw4.sh` → FAIL=0（user_version=30 钉）；`git diff apps/server/selftest_wsw4.sh` = :39 单行 |
| 6 | **selftest_wsw56 零改动回归** | `RP_WSW_BIN=... bash apps/server/selftest_wsw56.sh` 零 diff 回归 PASS/SKIP 分开计数 exit 0（42/0 基线） |
| 7 | **release build 面** | `cargo build --release --offline --workspace` exit 0 + wall/du 数字表；`git diff Cargo.toml Cargo.lock vendor/` = 0（[profile.release] 零触碰 + 417 恒定反证） |
| 8 | **release 终钉对账** | rp-wsw 0.1.4 manifest 孪生 binary_sha256 与 sha256sum 复算一致（WSW-8 :170 终钉词形）；不一致 → 停批上报（禁回钉禁重登记——登记面属 WSW-8 产物） |
| 9 | **containerfiles 静态核验表** | 11 族逐族行（FROM digest / 文件集 / manifest 孪生 / 台账三处一致）；rip-zap 64 零占位在案词形；e2e 常量 :36 零 diff |
| 10 | **containerfiles 真实构建面** | 五族 `podman build -t ...:rt7-verify` exit 0 实测（逐族命令原文 + 时长）+ `podman rmi` 即删 + after `podman images`/`system df` 与 before 一致；网络不可用降级 = 静态面全交 + 坏消息登记（判据降级词形明示） |
| 11 | **workspace 全量与静态门零红** | `cargo test --offline --workspace --no-fail-fast` 零红（1337 基线延续）+ `cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0 + `cargo fmt --all -- --check` 计数 ≤120 文件 / ≤936 hunks（存量红零扩大——D4）+ `cargo deny check advisories && cargo deny check licenses` 双检 exit 0 |
| 12 | **零迁移零新依赖与收口** | migrations 001-030 零 diff + 生产库五文件 mode=ro `PRAGMA user_version` = 30 逐文件回显（selftest_wsw56.sh :623-628 先例词形）+ `git status --porcelain` 只含 §2.1 落点（≤7 行）+ Cargo.lock 417 / reqwest=1 恒定 + vendor/ 零 diff |

## 5. 测试面（收口命令）

```bash
# 门禁基线（S1）
git status --porcelain | wc -l          # 0
git log --oneline -1                    # dca07555（WSW-8 收口链在位）
grep -cE '^\[\[package\]\]' Cargo.lock  # 417（WSW-8 新基线；HEAD 口径 = git show HEAD:Cargo.lock | grep -cE ...）
grep -c 'name = "reqwest"' Cargo.lock   # 1（chromiumoxide 传递双检——WSW-8 §7.4 词形）
ls migrations/*.sql | wc -l             # 30
sqlite3 "file:migrations/dev-b1.db?mode=ro" "PRAGMA user_version"   # 30
# Rust 全量（2026-11-23 纪律：workspace 一次必须跑；基线 = WSW-8 收口 1337 passed 零红）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast
# 静态门
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check > /tmp/fmt_after.txt 2>&1; grep -c "^Diff in" /tmp/fmt_after.txt   # ≤936 hunks（存量红零扩大，D4）
cargo deny check advisories && cargo deny check licenses
# CI yml lint 与词形（S2）
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo YAML_OK
grep -c "timeout-minutes" .github/workflows/ci.yml    # 4
grep -n "uses: actions/\|uses: dtolnay/" .github/workflows/ci.yml   # ref 版本串逐处对账
grep -n "channel" rust-toolchain.toml && rustc --version             # 版本串三处一致（§4-2）
# selftest 收口回归（S3）
RP_WSW_BIN=/home/emo/rt3-target/debug/rp-wsw bash apps/server/selftest_wsw7.sh    # FAIL=0（417 新钉）
bash apps/server/selftest_wsw4.sh                                                 # FAIL=0（30 钉）
RP_WSW_BIN=/home/emo/rt3-target/debug/rp-wsw bash apps/server/selftest_wsw56.sh   # 零改动回归 42/0
# 发布矩阵 build 面（S4）
time CARGO_TARGET_DIR=/home/emo/rt3-target cargo build --release --offline --workspace
sha256sum <rp-wsw release 二进制路径>   # 对账 manifests/tools/rip.wsw.browser.json binary_sha256（0.1.4 a226fb56 终钉）
git diff Cargo.toml Cargo.lock vendor/ | wc -l   # 0
# containerfiles 双面（S5——静态核验表入专档；真实构建五族）
podman system df && podman images --format "{{.Repository}}:{{.Tag}}" | sort   # before 快照
podman pull python:3.12-slim@sha256:4840d514c583a4d3d8cad3940e68dee6f0f6f3635c12fcd0a3d74c21a5b93a54   # 基座受控拉取一次（网络不可用→降级词形）
podman build -t localhost/rip-tool-base:rt7-verify -f containerfiles/rip-tool-base/Containerfile containerfiles/rip-tool-base/
podman build -t localhost/rip-tool-coreutils:rt7-verify -f containerfiles/rip-tool-coreutils/Containerfile containerfiles/rip-tool-coreutils/
podman build -t localhost/rip-tool-webresearch:rt7-verify -f containerfiles/rip-tool-webresearch/Containerfile containerfiles/rip-tool-webresearch/
podman build -t localhost/rip-tool-source:rt7-verify -f containerfiles/rip-tool-source/Containerfile containerfiles/rip-tool-source/
podman build -t localhost/rip-nuclei:rt7-verify -f containerfiles/rip-nuclei/Containerfile containerfiles/rip-nuclei/
podman rmi localhost/rip-tool-base:rt7-verify localhost/rip-tool-coreutils:rt7-verify localhost/rip-tool-webresearch:rt7-verify localhost/rip-tool-source:rt7-verify localhost/rip-nuclei:rt7-verify
podman system df && podman images --format "{{.Repository}}:{{.Tag}}" | sort   # after = before（零残留反证）
# 生产库只读指纹（S6）
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db migrations/platform-governance.db migrations/platform-search.db; do sqlite3 "file:$f?mode=ro" "PRAGMA user_version"; done   # 逐文件 30
# 零 diff 反证（S6）
git status --porcelain -- crates/ migrations/ vendor/ Cargo.lock Cargo.toml web/ workers/ integrations/rtis/ | wc -l   # 0
git status --porcelain | wc -l   # 仅 §2.1 落点（≤7 行）
# CI watch 收口重试词形（S6——EOF 间歇实测，retry 循环）
for i in 1 2 3 4 5; do gh run list --limit 3 && break || echo "EOF retry $i"; sleep 10; done
```

基线记录（本任务书写作时点 2026-11-23 实测）：

- HEAD `dca07555` 干净树（WSW-8 收口在链；RT-5/6 `f8d38107` 先行在链）；migrations = 30；Cargo.lock 417 / reqwest=1。
- ci.yml 四 job 零 timeout；dtolnay@stable 浮动；本地 rustc 1.99.1；actions 四类 ref = v4/v4/v5/v4。
- selftest_wsw7 三钉现红（:292/:293 期望 3 实测 0；:294 期望 399 实测 417）；selftest_wsw4 :39 期望 28 实测 30。
- fmt 存量红 120 文件 / 936 hunks（7 交集文件全在）。
- podman：17 images（10 活跃）8.198GB；python:3.12-slim 基座缺位；nuclei 基座在位；e2e SKIP 守卫在案（podman_host_e2e.rs:69）。
- vendor 387M / 20009 文件随 git 入库；PyYAML 6.0.4 在位；`gh run list` EOF 间歇实测。

## 6. 红线（八条，违反任一即停批上报）

1. **CI 质量门零缩减**（NATIVE-FIRST :229/:242 原文词形）：`cargo test --offline --workspace` / `clippy -D warnings` / parity job / python lock 校验 / npm 五步逐行保留零删改；timeout-minutes 只加帽（防悬挂）不缩窗——帽值不得低于 D1① 声明面；禁删 job、禁改触发/permissions/concurrency 词形。
2. **CI 面禁绝对钉**：Cargo.lock `[[package]]` 计数、vendor 文件数等绝对数钉禁进 ci.yml（selftest_wsw7 :294 399 钉次日即陈旧 = 教训自证——A4/A6）；CI lock 面 = offline build 隐式核验 + RT-2 Python lock 校验维持；lock 新口径 417 只进本批判据与 selftest 重钉（D2）。
3. **Rust 面零 diff**：Cargo.toml / Cargo.lock / vendor/ / .cargo/config.toml / crates/** 零 diff；rust-toolchain.toml = 工具链选择文件非依赖面（不进 lock——D5）；零新依赖零新 vendor（CI yml 改动无 Rust 面）；`[profile.release]` :117-119 零触碰。
4. **selftest 收口边界**：只动 wsw7 :285-294 块三 ck 行与 wsw4 :39 单行（钉值同步非测试逻辑改写——actual 均为既有事实）；其余钉零触碰；owner 会签等效声明 = 三件在案预授权词形合订专节入报告（D2③），禁静默修改、禁扩大为 selftest 重构。
5. **生产库零触碰 + 034 不立项**：五文件全程 mode=ro 只读指纹；migrations 001-030 零 diff + 链尾十六钉零随更；坐实需新表 → 停批上报重新立项禁临场加迁移（028 先例）。
6. **容器面边界**：只建 `rt7-verify` 验证标签且 `podman rmi` 即删——登记面三处 digest（tool-manifest.yaml/json + podman_host_e2e.rs:36）零改动（本地构建 digest 不可字节复现——README 明词形，重建登记属部署流程非本批）；受控拉取仅限 python:3.12-slim@4840d514… 精确 digest；禁 pull -a / 禁全局 prune / 既有镜像容器卷零触碰（before/after 清点一致反证）；宿主零安装（podman 6.2.0 已在位）。
7. **fmt chore 不并入**（D4）：120 文件存量红零触碰零扩大（本批落点 shell/doc/yml 全不入 fmt 面）；`crates/**` 零 diff（链尾 4 + 跨轨 3 交集文件实证在清单——fmt 收敛须独立批评审窗口禁夹带，RT-5/6 D3 词形延续）。
8. **边界纪律**：RELEASE 对账只产输入面（D6）——禁替 RELEASE 立档、禁改 docs/releases/ 34 份；WSW-9 面（性能验收/并发池/Windows 词表）零触碰（A14）；api EOF 重试 = 流程词形不进 yml（A5(d)）；禁 commit/add -A/push（本批只落文件，工作树留主会话审查收口——S6 显式 add 仅登记清单）。

## 7. 回滚

- 文件级 revert：ci.yml / rust-toolchain.toml / selftest_wsw7.sh / selftest_wsw4.sh / 两份文档 / docs/README 索引行随 `git checkout/checkout HEAD --` 逐项撤销（本批零 commit——主会话审查收口，revert 面 = 工作树丢弃；rust-toolchain.toml = 新建文件直接删除）。
- ci.yml 回滚 = ref 串回写 v4/@stable + 删 timeout 四行——CI 配置面即时生效零持久副作用；回滚后 CI 门语义 = 回到修固前态（零质量门变化——门本体从未改过）。
- 工具链零回滚面：rustup 零操作（未 update 未 install——1.99.1 本就在位）；rust-toolchain.toml 删除即回浮动态。
- rt7-verify 标签构建产物已 rmi 即删零残留（判据 10 before/after 一致反证）；受控拉取的 python:3.12-slim 基座 = 拉取一次本地留存（无删除义务——后续批构建复用；如需清理走 RT-5/6 DISK-GOVERNANCE 归属流程不属本批）；podman 其余对象零操作无回滚面。
- selftest 钉值回滚 = `git checkout HEAD -- apps/server/selftest_wsw7.sh apps/server/selftest_wsw4.sh` 即回陈旧红已知态（三钉红 + 28 钉红——不引入新失败面）。
- Cargo.lock/vendor/crates/migrations/web 零 diff 无回滚面；生产库/宿主/Distrobox 零操作无生产回滚面。

## 8. 交付物

1. **CI 修固面**：ci.yml diff（timeout ×4 + action ref 四类 bump + dtolnay 版本钉）+ rust-toolchain.toml 新建——判据 1/2/3 回显逐条入报告；CI run 全绿实录（push 后 watch——EOF 重试词形实录入报告）。
2. **selftest 陈旧钉收口**：selftest_wsw7.sh 三钉（diff 双钉收口态 + [[package]] 417 新基线）+ selftest_wsw4.sh :39（28→30）+ **owner 会签等效声明专节**（RT-5/6 :218 观察池⑧ + WSW-8 :166 预授权 + RELEASE PHASE5 :63 §5 先例三件合订）——判据 4/5/6 回显。
3. **发布矩阵 build 三面数字**：release workspace 全量构建（wall/du）+ rp-wsw 0.1.4 终钉 sha256 复算对账 + vendor-发布物关系裁决三段词形——:229「原生构建矩阵」交付对照；判据 7/8 回显。
4. **containerfiles 双面核验**：11 族静态核验表（FROM digest / 文件集 / manifest 孪生 / 台账三处一致）+ 五族 rt7-verify 真实构建 exit 0 实测（或降级登记）+ 六族重建路径登记（gov pypi / embed·rerank·parser 模型烘焙 / pg17 编译 / zap 选项 B）——「容器工具仅在需要时构建」:229 对照；判据 9/10 回显。
5. **docs/phase0/RT-7-CI-RELEASE-VERIFICATION.md**：CI 修固清单 / build 三面 / containerfiles 双面 / RELEASE 五节对账输入面四节 + docs/phase0/RT-7-BATCH-REPORT.md（判据 1-12 对照 + 坏消息单列 + 会签声明 + 观察池）+ docs/README.md 索引两行。
6. **WSW-9 + RELEASE 前置输出**（RT-7 = RELEASE 前最后一个 RT 批——对账表输入面专节，WSW-8 §7 专节先例词形）：(a) RELEASE 五节脊柱输入面 = 一 批次汇总/提交链（RT-7 批 commit 位留白回填）+ 二 门禁数字（workspace 1337+ / clippy / deny / selftest 三套 / CI 四 job 修固后 run 数字）+ 四 已知取舍（EOF 间歇 / action 升级实录 / 构建降级面）+ 五 回滚方式（§7 词形）；(b) selftest 钉值同步清单（wsw7 三钉 + wsw4 一钉——RELEASE 立档 §钉值同步节直接引用）；(c) 发布矩阵三面数字表（RELEASE 门禁节引用）；(d) WSW-9 交接零新增声明（本批零触碰其移交面——A14 对照）。
7. **下一批输入（观察池登记，本批零实现）**：①fmt 全仓收敛独立 chore 批（D4——120 文件 + 链尾 4 交集 + 跨轨 3 交集，独立评审窗口禁夹带——RT-5/6 D3 词形延续）；②deps 层旧 fingerprint 判定批（RT-5/6 观察池③承接）；③btrfs balance 执行（RT-5/6 观察池④承接——sudo 步骤用户裁量）；④镜像族告警阈值定稿（RT-5/6 观察池⑤承接——11 族台账 + 本批核验表为输入）；⑤ZAP 镜像面复测（WSW-7 选项 B——rip-zap 64 零占位核验行承接）；⑥WSW-9 性能/可靠性验收批（WSW-8 §7.1 移交面——本批零触碰确认）；⑦action ref 升级后 CI 稳态复测（若实施时点升级未全落——deprecation 警告零残留复核）。
