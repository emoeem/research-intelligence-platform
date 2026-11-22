# RT-7 发布矩阵核验专档 · CI 修固 + build 三面 + containerfiles 双面 + RELEASE 对账输入面

- 批次：RT-7（NATIVE-FIRST §九 :229「原生构建矩阵、完整测试/Clippy 保持、容器工具仅在需要时构建」核验面交付）
- 日期：2026-11-23
- 基线：HEAD `dca07555`（WSW-8 收口在链）+ RT-5/6 `f8d38107` 先行在链；Cargo.lock 417 / reqwest=1；migrations 30
- 任务书：[RT-7-TASK.md](RT-7-TASK.md)（D1-D6 裁决 / 判据 1-12 / 八红线）；批次报告：[RT-7-BATCH-REPORT.md](RT-7-BATCH-REPORT.md)
- 本档 = D3 发布矩阵核验形状四节交付：一 CI 修固面清单 / 二 build 三面数字表 / 三 containerfiles 双面 / 四 RELEASE 五节对账输入面

---

## 一、CI 修固面清单（逐项 before/after）

工作流 = `.github/workflows/ci.yml` 单 workflow 四 job（rust / parity / python-workers / web）；diff 实测 = 12 insertions / 8 deletions，只含 timeout 行（4）+ action ref 行（8）两类，测试/clippy/lock/触发/permissions/concurrency 词形零 diff（判据 1/3 词形反证）。

### 1.1 timeout-minutes 补齐（D1①——现状 grep 实测 0 处 → 四 job 各一帽）

| job | 修固前 | 帽值（D1① 声明面） | CI 基线（RELEASE PHASE5 :18 实测） | 帽/基线倍率 |
|---|---|---|---|---|
| rust | 无帽（默认 360min） | `timeout-minutes: 45` | 3m53s | 11.6× |
| parity | 无帽 | `timeout-minutes: 20` | 1m9s | 17.4× |
| python-workers | 无帽 | `timeout-minutes: 15` | 19s | 47.4× |
| web | 无帽 | `timeout-minutes: 15` | 32s | 28.1× |

- 帽值逐 job = D1① 声明面原值（45/20/15/15）；对照基线全为宽松方向（倍率 ≥ 任务书「6-10 倍」下界词形），零窗口缩减——红线条 1「timeout 只加帽不缩窗」成立。
- 补充基线点：WSW-8 收口 CI run `45061902514` 全 run 9m32s（`gh run list` 实测 2026-11-23）——rust job 修固前无帽实态下最长观测仍远离 45min 帽。
- 只加帽不改测试面：`cargo test --offline --workspace` / clippy `-D warnings` / parity 双通道 / python lock 校验 / npm 五步逐行零 diff。

### 1.2 Rust 工具链钉定（D1②——@stable 浮动 → 版本钉）

| 面 | 修固前 | 修固后 |
|---|---|---|
| ci.yml dtolnay ref（:39/:73 两处） | `dtolnay/rust-toolchain@stable`（浮动） | `dtolnay/rust-toolchain@1.99.1` |
| `rust-toolchain.toml` | 零在位 | 新建：`channel = "1.99.1"` + `components = ["clippy"]`（镜像 ci.yml 现状词形） |
| 本地 rustc | 1.99.1 (58cf8f9c3 2026-10-13)（rustup stable active default） | 同版本串零下载零行为变化（落点前后 `rustc --version` 逐字一致实测） |

- 版本串三处一致（判据 2）：rust-toolchain.toml `channel = "1.99.1"` = ci.yml `@1.99.1` = `rustc --version` 实测 `1.99.1`。
- ref 存在性官方核验（2026-11-23）：`dtolnay/rust-toolchain` `1.99.1` ref 在位（tags 页 v1 commit "Add 1.99.1 patch release" 佐证 + tree/1.99.1 页直接核验）。
- **工具链安装路径敏感性实证（新事实，坏消息节详）**：rustup `1.99.1-x86_64-unknown-linux-gnu` 版本化安装与 `stable-x86_64-unknown-linux-gnu` 为同版本两物理目录（`rustc -vV` 逐字节一致）；目录切换触发 211 crate 部分重建 → release 二进制 sha256 漂移（228e4a9a）→ 以 stable 路径复跑 manifest 注册命令字节级复现 `a226fb56` 终钉。同工具链路径重建 = 字节可复现（WSW-8 manifest「release 重建即漂移」词形适用面收窄为跨路径/跨版本重建）。

### 1.3 actions Node 20 deprecation 升级（D1③——官方 release 页 2026-11-23 逐个核验存在性）

| action | 修固前（4 处分布） | 修固后 | 官方核验 |
|---|---|---|---|
| actions/checkout | @v4（:36/:70/:95/:121） | @v5 | v5.1.0 release 在位 |
| actions/setup-python | @v5（:98） | @v6 | v6.0.0 release 在位 |
| actions/setup-node | @v4（:124） | @v5 | v5.0.0 release 在位 |
| actions/cache | @v4（:44/:76） | 维持 v4 | v4.3.0 release 在位（D1③ 明词形不变） |

- 取更高 major（checkout v7 / setup-python v7 / setup-node v7 / cache v6 官方页在位）**不采纳**——D1③ 声明面 = 只加固不扩面（:242 保持原有范围），升级至声明面版本即收口。
- 触发（push/PR→main :15-18）/ permissions（contents:read :20-21 + 各 job :31-32 等）/ concurrency（:24-26）词形零改动实测。

### 1.4 lock 步骤零新增（D1④——绝对钉禁进 yml）

- ci.yml lock 面 = 维持 RT-2 Python lock 静态校验唯一步骤（`python -m unittest tests.test_lock_integrity`，working-directory workers/python）；**零 Cargo.lock 步骤零新增**（399 陈旧钉次日即红教训——红线条 2）。
- Cargo.lock 新口径 417 = 本批判据面（判据 12）+ selftest_wsw7 重钉面（D2），不进 yml。
- vendored-sources 一致性由 `cargo test --offline` / `cargo build --offline` 构建隐式核验（vendor 缺 crate 即 fail——本批 S1 全量 + S4 release 两次实测 exit 0）。

## 二、build 三面数字表（D3①）

### 2.1 (a) release workspace 全量构建实测

| 项 | 数字 |
|---|---|
| 命令 | `time CARGO_TARGET_DIR=/home/emo/rt3-target cargo build --release --offline --workspace` |
| exit | 0 |
| wall | **1m46.129s**（user 16m23.608s / sys 51.302s） |
| 编译面 | 211 crate 重编译（rust-toolchain.toml 工具链路径切换引起的部分 fingerprint 失效——rustc `-vV` 两目录逐字节一致，非编译器版本差异） |
| release du | **2.1G**（构建前 1.1G——WSW-8 收口态口径；rt3-target 总量 75G） |
| 零触碰反证 | `git diff Cargo.toml Cargo.lock vendor/ | wc -l` = **0**；`[profile.release]`（Cargo.toml:117-119 `strip = "debuginfo"` 唯一 profile 段）零 diff；Cargo.lock 417 / reqwest=1 恒定 |

### 2.2 (b) release 终钉对账（WSW-8 :170 词形）

| 步序 | 实测 |
|---|---|
| 构建前指纹 | `/home/emo/rt3-target/release/rp-wsw-browser` sha256 = `a226fb56c8061eb16bf739f3213705fd8e2b32d4977c11b7ee97e8274f0a564e`（= manifest 孪生 binary_sha256 精确一致——预置 PIN 态确认） |
| 全量 release 构建后 | sha256 = `228e4a9a2a4a055aa581a822d4d784499a58c85224413cacc66138d277ea5ed2`（**漂移**——工具链路径切换重编译重链接所致，manifest known_limits「release 重建即漂移」词形实况） |
| 归因闭环 | `RUSTUP_TOOLCHAIN=stable-x86_64-unknown-linux-gnu cargo build --offline --release -p wsw-runtime --bin rp-wsw-browser`（= manifest provenance 注册命令原形，9.24s）→ sha256 = **`a226fb56…` 字节级复现 = PIN_MATCH** |
| 收口态 | sha256sum 复算 = manifest json/yaml 孪生 `a226fb56…` 一致（判据 8 PASS）；manifest 零触碰（禁回钉禁重登记词形成立——登记面属 WSW-8 产物） |
| manifest 路径注 | 孪生 `runtime.binary` 注册路径 = `/home/emo/rt3-target/release/rp-wsw-browser`（任务书 §5 注释「rp-wsw 二进制路径」为宽泛词形——对账以 manifest 注册路径为准） |

### 2.3 (c) vendor 与发布物关系裁决（三段词形）

1. **vendor/ = 发布源树成分**：随 git 树入库（`git ls-files vendor/ | wc -l` = **20009 文件** / du = **387M**）；`.cargo/config.toml` vendored-sources 四行 = 离线构建唯一入口（`--offline` 构建两次实测 exit 0 = vendor↔lock 一致性隐式核验）。发布源树 = git 树整体（含 vendor），无裸 registry 外联。
2. **发布物二进制不含 vendor**：链接面零目录依赖——`rp-wsw-browser` 12MB / `rp-wsw` 24MB 单文件自含（静态链接面），运行期零 vendor 路径引用；vendor 仅为构建期源。
3. **镜像属运行时数据不入库**（containerfiles/README 头词形 + embed 族台账行 gitignore 词形）：镜像/digest 台账记录在 README 与 tool-manifest，镜像本体不入 git；登记面 digest = 建成时点实测值，本地重建不可跨主机字节复现（README 明词形——本批 rt7-verify 即删 = 零登记漂移纪律）。

## 三、containerfiles 双面核验（D3②）

### 3.1 静态核验表（11 族逐族——判据 9）

| 族 | FROM digest（逐族 grep 实测） | 文件集 | manifest 孪生 | 台账↔manifest 一致 |
|---|---|---|---|---|
| rip-tool-base | `python:3.12-slim@sha256:4840d514…a54`（docker.io/library 形） | Containerfile + smoke_tool.py + tool-manifest.json/yaml | 有 | `1dbc67084d…` 三处一致（台账 :46 ↔ tool-manifest.json:11/:112 ↔ e2e 常量 podman_host_e2e.rs:36） |
| rip-tool-coreutils | 同上 | Containerfile + core_tools.py + rip.core.*.json/yaml ×5 | 有 | `49436f71…` 一致（台账 :48 ↔ rip.core.validate.json:7/:60 等五对） |
| rip-tool-webresearch | 同上 | Containerfile + http_inspector.py + tool-manifest.json/yaml | 有 | `085b4ef3…` 一致（台账 :47 ↔ tool-manifest.json:11/:154） |
| rip-tool-source | 同上 | Containerfile + src_tools.py + rip.src.*.json/yaml ×3 | 有 | `8251ba32…` 一致（台账 :49 ↔ rip.src.*.json 三对） |
| rip-nuclei | `docker.io/projectdiscovery/nuclei@sha256:582d5546…2eb`（:19） | Containerfile + wrapper.sh | 无（wrapper 族） | 台账 :52 `9ff41ce9…`（登记面，rt7-verify 即删不触） |
| rip-embed | `python:3.12-slim@sha256:4840d514…a54`（:10） | Containerfile + embed_tools.py + rip.embed.text.json/yaml | 有 | `013dc02b…` 一致（台账 :15 ↔ rip.embed.text.json） |
| rip-gov | 同上（:9） | Containerfile + gov_tools.py + rip.gov.minhash.json/yaml | 有 | `cb043e45…` 一致（台账 :20 ↔ rip.gov.minhash.json） |
| rip-rerank | 同上（:10） | Containerfile + rerank_tools.py + rip.rerank.text.json/yaml | 有 | `768d3d12…` 一致（台账 :16 ↔ rip.rerank.text.json） |
| rip-parser | 同上（:12） | Containerfile + parser_tools.py + requirements.txt + rip.parser.*.json/yaml ×3 | 有 | `9a59eef5…` 一致（台账 :50 ↔ rip.parser.*.json 三对） |
| rip-pg17 | `docker.io/paradedb/paradedb:pg17@sha256:7cb29053…8c8` 双段（Dockerfile:18 builder / :34 runtime） | Dockerfile + third_party/ | 无（数据面族） | 台账 :51 `229189e4…`（登记面） |
| rip-zap | `ghcr.io/zaproxy/zaproxy@sha256:000…000`（:32，64 零占位） | Containerfile + wrapper.sh | 无（wrapper 族） | 无台账行——WSW-7 D1 选项 B 未实拉在案词形（判据 9 rip-zap 词形） |

- FROM 真钉 10/11 + rip-zap 64 零占位 1/11：逐族 grep 原文入上表（digest 前十六位缩写，全 64 位在 Containerfile 原文实测）。
- e2e 常量零 diff：`crates/tool-engine/tests/podman_host_e2e.rs:36` `MANIFEST_DIGEST = 1dbc67084d…` 零改动；:69 SKIP 守卫词形在案（「SKIP: 验收镜像 … 不在本地镜像库」）。

### 3.2 真实构建面（五族 rt7-verify——判据 10）

前置：df 快照（`/media/emo/containers/storage` 所在盘 477G 总量 / 279G 可用，防近满盘）；基座受控拉取一次 `podman pull python:3.12-slim@sha256:4840d514…a54` exit 0（store 计数 17→17 零漂移——digest 形态已在 `<none>` 槽位去重）；nuclei 基座在位反证（`docker.io/projectdiscovery/nuclei:latest` 935MB 本地 store）。

| 族 | 命令（原形） | exit | wall | 构建期出网面 |
|---|---|---|---|---|
| rip-tool-base | `podman build -t localhost/rip-tool-base:rt7-verify -f containerfiles/rip-tool-base/Containerfile containerfiles/rip-tool-base/` | 0 | 2.927s | 零（FROM 本地解析 + COPY + RUN mkdir） |
| rip-tool-coreutils | 同形 coreutils | 0 | 2.608s | 零（同形三步） |
| rip-tool-webresearch | 同形 webresearch | 0 | 2.644s | 零（同形三步） |
| rip-tool-source | 同形 source | 0 | 2.635s | 零（同形三步） |
| rip-nuclei | 同形 nuclei | 0 | 0.294s | 零（FROM 本地 935MB 基座 + ENV/COPY/RUN chmod/ENTRYPOINT 五步） |

- 四工具族 = README P3-1 构建口径词形成立：镜像内零包安装（工具本体 Python 纯标准库），构建全程离线（基座 digest 本地命中）。
- 清理：`podman rmi` 五验证标签 exit 0（8 层条目删除）；after `podman system df` = Images 17（10 活跃）8.198GB / Containers 5（2 活跃）11.74GB / Volumes 13（5 活跃）568MB，与 before 逐项一致；`podman images` tag 清单与 before 同列（11 条）——**零残留零登记漂移反证成立**（README「本地构建的 digest 不可跨主机字节级复现」——验证标签即删不触登记面三处 digest）。

### 3.3 其余六族重建路径登记（不重建——「容器工具仅在需要时构建」:229 对照）

| 族 | 重建路径（README 台账实录） |
|---|---|
| rip-gov | 构建期受控拉取 pypi `datasketch==2.0.0`（numpy/scipy 传递，镜像 ~250MB 级）；运行期 network=none |
| rip-embed | onnxruntime==1.32.0/tokenizers==0.23.3/numpy==2.5.4 钉版 + bge-small-zh-v1.5 模型构建期烘焙 /opt/models（GB 级模型面） |
| rip-rerank | bge-reranker-base ONNX 构建期烘焙（Xenova 社区导出，hf-mirror 308 跟随；1.1G fp32 单文件） |
| rip-parser | torch 2.6.0+cpu/torchvision 0.21.0+cpu + docling==2.150.1 等 + docling 模型烘焙（镜像 3.42 GB） |
| rip-pg17 | paradedb 基座 + pg_jieba 多阶段编译（third_party/ 内置源码零构建期网络；编译时长面） |
| rip-zap | 64 零占位 digest 未实拉（WSW-7 D1 选项 B）——上游 digest 回钉后按 Containerfile 头注命令构建 |

- 六族 = 部署面（模型烘焙/多阶段编译/上游 digest 待回钉）非核验面；本批静态核验表已全 11 族覆盖（判据 9），重建路径登记 = 未来批输入。

## 四、RELEASE 五节对账输入面（D3③ / 任务书 §8⑥——立档禁越界，本档只产输入）

RELEASE 立档 = 主会话阶段收口动作；本节 = 对账表直接引用面（docs/releases/ 34 份五节脊柱同构——A13）。

### 四.一 批次汇总/提交链

- RT-7 批（CI 修固 + 发布矩阵核验 + selftest 陈旧钉收口）：落点 7 文件（§2.1 清单：ci.yml / rust-toolchain.toml / selftest_wsw7.sh / selftest_wsw4.sh / 本档 / RT-7-BATCH-REPORT.md / docs/README.md）；**commit 位留白回填**（批零 commit——工作树留主会话审查收口）。
- 链位：HEAD `dca07555`（WSW-8 收口）+ RT-5/6 `f8d38107` 先行在链。

### 四.二 门禁数字（RELEASE 门禁节引用）

| 门 | 数字 |
|---|---|
| workspace 全量 | 1337 passed / 0 failed / 2 ignored（基线 + 收口门两次实测零红） |
| clippy | `--workspace --all-targets -- -D warnings` exit 0 |
| deny | advisories exit 0 + licenses exit 0 双检 |
| fmt | 120 文件 / 936 hunks 存量红精确持平（零扩大——D4；独立 chore 批待立） |
| selftest 三套 | wsw7 PASS=34 SKIP=4 FAIL=0；wsw4 PASS=34 FAIL=0；wsw56 PASS=42 SKIP=0 FAIL=0（零改动回归） |
| release build | exit 0 / wall 1m46s / release du 2.1G / 终钉 a226fb56 PIN_MATCH |
| 容器五族 | build exit 0 ×5 + rmi 即删 + before/after 清点一致 |
| CI 四 job 修固后 run 数字 | **待主会话 push 后实测回填**（本批禁 push；EOF 重试词形已实测成立——A5(d) 流程面） |

### 四.三 交付总账

见 RT-7-BATCH-REPORT.md 改动文件清单节（路径 + 行数）。

### 四.四 已知取舍/坏消息（RELEASE 坏消息节引用）

1. 工具链安装路径 sha 敏感性：rust-toolchain.toml（1.99.1 版本化目录）+ 全量 release 构建 → rp-wsw-browser sha 漂移（228e4a9a）；stable 路径复跑注册命令字节复现 a226fb56。**维护协议注**：同路径重建可复现，跨路径重建漂移——manifest known_limits「release 重建即漂移」词形建议后续批补注（WSW-8 产物面，本批禁改）。
2. GitHub API 间歇 EOF：`gh run list` 首击 EOF 实测，retry 循环第 2 次成功——CI watch 收口流程词形必须带重试（已实测）。
3. 生产库指纹词形勘误：任务书 §5 注释「逐文件 30」为侦察侧词形误差——实测 1/1/1/0/0（platform.db 水位 1 = 未触碰基线，selftest_wsw56 :623-628 先例词形同款）；30 属 dev-b1.db 与 migrations/*.sql 计数面。
4. action ref 升级后 CI run 未实录：本批零 push（红线 8），四 ref 升级的 CI 绿实录 = 主会话 push 后回填（观察池⑦）。

### 四.五回滚方式

RT-7-TASK.md §7 词形全文适用：文件级 revert（ci.yml ref 回 v4/@stable + 删 timeout 四行 / rust-toolchain.toml 删除即回浮动态 / selftest 两 sh checkout 回陈旧红已知态 / 两文档删档 / README 索引行删）；rt7-verify 已 rmi 零残留；受控拉取基座本地留存无删除义务；Cargo.lock/vendor/crates/migrations/web 零 diff 无回滚面。

### 四.六 selftest 钉值同步清单（RELEASE 钉值同步节直接引用）

| 脚本 | 钉位 | 旧值 | 新值 | 预授权 |
|---|---|---|---|---|
| apps/server/selftest_wsw7.sh :292 | Cargo.lock diff（依赖块行） | 3 | 0（收口态恒 0） | 观察池⑧ + WSW-8 :166 |
| apps/server/selftest_wsw7.sh :293 | Cargo.lock diff 无其它行 | 3 | 0（收口态恒 0） | 观察池⑧ + WSW-8 :166 |
| apps/server/selftest_wsw7.sh :294 | [[package]] 计数 | 399 | 417 | WSW-8 :166 明文 |
| apps/server/selftest_wsw4.sh :39 | 迁移水位 user_version | 28 | 30 | 观察池⑧ 同族词形 + RELEASE PHASE5 :63 §5 先例 |

（四钉 = WSW-8 :166「wsw7 四钉」口径：wsw7 三 ck 行 + wsw4 一行；WSW-8 收口期第四红 = :296 CROSS vendor 面，收口态实测 0 = 零触碰自愈，本批零改动。）
