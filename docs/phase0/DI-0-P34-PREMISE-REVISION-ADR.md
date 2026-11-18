# DI-0 ADR：P3-4 前提修订（Document Pack planned 裁定的前提被 NATIVE-FIRST 策略与 RT-0.5 实测推翻）

- 日期：2026-11-19
- 状态：已定稿（DI-0 前提修订 ADR；DI 批次链首个文档，DI-1/2/3 入口指针见 §5）
- 性质：对 docs/phase3/PHASE3-DESIGN.md §2.3 与 docs/phase0/PHASE-ROADMAP.md §4.3 行 P3-4 的**前提修订**，不推翻其历史事实登记（Phase 3 收口时点的裁定按当时输入成立）；修订生效面为 DI 批次链及其后的 Document Pack 规划
- 依据（引用两文件原文行号 + RT-0.5 五文档实测）：§2/§3 逐条给出；本 ADR 写作过程只新增本文档与 docs/README.md 登记行，未改任何源文件

## 1. 旧裁定原文（带行号）

**PHASE-ROADMAP.md:260（P3-4 行原文，逐字）**：

> | P3-4 | Document Pack 补强：PDF/Office/OCR 进容器（现 parsing_worker 仅 md/txt/html 纯文本，TOOL-INVENTORY §1.2） | PDF 解析工具一个 run 端到端；第三方解析库全部在容器内 | 【并行】依赖 P3-1（自研可走 P2-5，引入第三方库建议等 P2-6 后走 VM，按信任级裁定） |

**docs/phase3/PHASE3-DESIGN.md 承接原文**：

- §1:18（裁定依据 2）：「ADR §3.2 过渡期纪律原文：……Document 与 Mobile 两包的重依赖按 ROADMAP P3-4/P3-6 原文建议属 VM 通道信任级，VM 挂起期间只能 planned。」
- §2.3:53（planned 表 Document 行）：「Document | PDF、Office、Markdown、OCR/文本抽取 | planned | 第三方解析库按 ROADMAP P3-4 原文建议按信任级走 VM 通道；VM 面挂起中（§3.1）」
- §2.3:46（存根落盘）：「document.json（P3-4，第三方解析库等 VM 通道）」，存根 `manifests/packs/planned/document.json` trust_ruling.rationale 引用同一原文（「ROADMAP P3-4 原行建议『引入第三方库建议等 P2-6 后走 VM，按信任级裁定』」），exit_conditions 两条：「S1 到 S4 解锁」+「第三方解析库许可证核实通过并入 provenance」。

旧裁定的两条**前提**：

- **前提 A**：第三方解析库（PDF/Office/OCR 解析能力）只能由容器承载，宿主无可信解析面，「第三方解析库全部在容器内」（ROADMAP:260 判据原文）。
- **前提 B**：按信任级，第三方解析依赖必须走 VM 通道，VM 未解锁则只能 planned（PHASE3-DESIGN §1:18、§2.3:53）。

## 2. 推翻论证

### 2.1 前提 A 被推翻：策略裁决变更在先，RT-0.5 实测在后

**策略层**（NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md，2026-11-18 定稿，晚于 PHASE3-DESIGN 的 2026-11-10）：

- §1:11 决策摘要：「RIP 从『工具执行默认容器化』调整为 **Native-first、Isolation-when-needed（原生优先、按风险隔离）**」，默认容器化（前提 A 的载体）已不是生效策略。
- §3.1:64-76 原生执行准入六条件：来源可信且审核（:68）、Manifest 固定身份与参数 Schema（:69）、受控子进程全要素（:70）、凭据不继承（:71）、网络策略（:72）、统一 Workspace/Artifact/Evidence 路径（:73），第三方宿主 CLI 在满足六条件时**允许**受限原生。
- §12:255-262 Host Software First：「RIP 应优先复用宿主机已经安装、来源可信、版本与能力满足要求的成熟软件，而不是为了『统一环境』重复打包」（:255）；「对媒体、OCR、Office、PDF、图像、归档与元数据处理，Rust 负责路由、策略、预算……成熟软件负责专业解析/转换」（:258）。

**实测层**（RT-0.5，2026-11-18 五份只读取证）：

- RT-0.5-C §4:249-250：16 项能力 **15 available + 1 missing**。与 Document Pack 直接相关：pdf.pdftotext/pdf.pdfinfo/pdf.pdftoppm（poppler 26.11.0，C §3.6-3.8）、document.libreoffice（26.8.2.2 headless 可用，C §3.4:79）、ocr.tesseract（5.5.4，语言包 chi_sim/eng/osd，C §3.3:66）、convert.pandoc（3.11，C §3.5）。
- 唯一 missing 是 pdf.mutool（C §3.9:139-148），且其替代已裁决为 pdftotext/pdfinfo/pdftoppm + gs（RT-0.5-D §4.1:67-74「成熟替代已裁决」），缺失项不构成「只能容器」的理由。
- RT-0.5-D:24 现状裁决：rip.doc.parse「**保持** python_worker → RT-2 项目级 venv；PDF/Office/媒体解析不并入本工具，由 RT-1 新增 native CLI 工具（见 §3 能力映射）承接」，同文档 :46：「现有 19+1 个 Manifest **无一因宿主能力目录而改变 runtime**；宿主能力目录的映射价值在 §3，为文档/媒体处理提供 RT-1 新工具的能力解析输入」。

结论：宿主解析能力面实测成立（15 项 available），且策略已把「默认容器」改为「满足准入即原生」。前提 A（「第三方解析库全部在容器内」）作为判据被推翻；ROADMAP:260 判据 1「PDF 解析工具一个 run 端到端」保留为 DI-1 判据。

### 2.2 前提 B 被修订：VM 从唯一信任通道变为高风险档位

- RT-0.5-E §1:14-17 三档路由（判定输入 = 来源信任 × 格式解析器风险，:11）：可信低风险 → 「受限宿主原生子进程（RT-1 Native CLI Adapter：固定绝对路径、参数 Schema、环境白名单）」（:15）；来源不明 → rootless Podman（:16）；高风险/对抗性 → VM（:17，podman_vm 未就绪时**拒绝派发**）。
- RT-0.5-E §3:52-63 文档处理路由图把 Document 处理收敛为「capability lookup（RT-0.5-F Provider）→ Runtime Policy decision（四通道分派）→ parser/OCR/converter → 统一 Artifact/Evidence/Trace」，VM 只是四通道之一，且由输入风险档位触发，不是「第三方解析库」的固有属性。
- 信任事实更新：旧裁定把第三方解析依赖整体标为「third-party-license-unverified」（document.json trust_ruling）；RT-0.5 实测逐条落了 package_provenance（pacman 包归属：poppler 26.11.0-1.1、libreoffice-fresh 26.8.2-3、tesseract 5.5.4-1.1，C §3.6/3.4/3.3），发行版包来源可核实，「许可证未核」前提对发行版包路径不再成立。

结论：前提 B 修订为「按输入风险档位路由」：第三方解析 CLI 处理**可信低风险**输入时走受限原生（RT-1 Adapter 准入面）；处理**不明来源**输入时走 Podman（E §1:16）；**高风险**（PS/EPS、宏 Office 等）才落 VM 档（E §1:17，未就绪即拒绝）。域特化裁决不因此修订而放松：magick policy.xml 空壳 → 不可信图像/PDF 禁宿主直连（C §3.10:159-160）；soffice 禁并发共享 profile、运行时必须隔离临时 profile（C §3.4:81）；tesseract chi_tra 缺失显式报告禁静默降级（C §3.3:68）；归档路径穿越/炸弹防护独立实现（C §3.11:173）。

### 2.3 修订边界（不改的部分）

- PHASE3-DESIGN §2.3:54 Mobile Analysis planned 行（P3-6，Java 生态重依赖）**不在本修订面**，其前提是依赖重量而非解析库容器化，另行评估。
- planned 存根的登记纪律不变：planned 工具不进 tool 表、不建容器定义（PHASE3-DESIGN §2.3:44 校验九条第 4 条理由仍成立）。
- 本 ADR 只改登记文档；`manifests/packs/planned/document.json` 存根的 exit_conditions/trust_raining 修订随 DI-1 S0 执行（§4.3），本批不动存根 JSON。

## 3. rip.doc.parse + parsing_worker 现状冻结盘点表（md/txt/html 之外零覆盖实证）

冻结口径：rip.doc.parse 保持 python_worker 不迁不扩（RT-0.5-D:24 裁决）；PDF/Office/OCR 由 DI 链新工具承接，本工具零改动。

| 维度 | 实证 | 出处 |
|---|---|---|
| format 枚举 | `md`/`txt`/`html` 三值，`additionalProperties:false`，text/path 二选一 | manifests/tools/rip.doc.parse.json:24-58（input_schema）；:46-58（format enum） |
| Worker 支持面 | `SUPPORTED_FORMATS = ("md", "txt", "html")`；「支持 .md/.txt/.html；html 用标准库 html.parser 剥标签」 | workers/python/parsing_worker.py:29、:11 |
| PDF/Office/OCR 覆盖 | **零**：无 pdf/docx/odt/图像任何解析分支；扩展名推断表仅 .md/.markdown/.html/.htm/.txt/.text（parsing_worker.py:430-431） | parsing_worker.py:430-431；全文件无 pdf/office 关键字分支 |
| runtime 形态 | python_worker pooled，Worker Protocol v1，`python3 -I workers/python/parsing_worker.py` | rip.doc.parse.json:9-19 |
| 资源帽 | 输入 10 MiB（text maxLength）、输出 1 MiB、超时 60s、cpu 1、memory 256Mi | rip.doc.parse.json:40-42、118-123 |
| known_limits | 「输入硬上限 10 MiB」「html 行号不可用（剥标签后无行映射）」，不含任何 PDF/Office 限制声明 | rip.doc.parse.json:145-149 |
| 测试面 | workers/python/tests 125 项全绿（md/txt/html 面） | rip.doc.parse.json:149 |
| SK-1 扩展 | extract_links（md 专属）已入 schema，纯解析不访问链接 | rip.doc.parse.json:57、101-115 |

零覆盖实证结论：Document Pack 的 planned 内容（PDF/Office/OCR，ROADMAP:260）与 rip.doc.parse 现状之间的缺口 = **全部** PDF/Office/OCR 面；该缺口由 DI 链以新 Manifest（native CLI 工具，allowlist 钉值）承接，不改 rip.doc.parse。

## 4. 修订后的裁定（冻结）

1. **P3-4 planned 状态修订为「解锁条件改写」**：Document Pack 解锁不再绑定 VM S1-S4；改绑 DI 批次链（§5）+ RT-0.5-E §1 档位路由逐工具裁决。ROADMAP P3-4 行判据 1（PDF 解析工具一个 run 端到端）保留为 DI-1 判据；判据 2（「第三方解析库全部在容器内」）废止，替换为「第三方解析 CLI 按 RT-1-ADR D3 allowlist 钉值 + RT-0.5-E §1 档位路由执行；不明来源输入走隔离」。
2. **planned 存根 document.json 修订（随 DI-1 S0 执行，本批不改文件）**：exit_conditions 追加「DI-1 完成（PDF 文本抽取 native 工具端到端）即解锁 PDF 抽取面；Office/OCR 面按 DI-2/DI-3 逐面解锁」；trust_ruling.level 修订为「distro-package-verifiable + input-risk-routed」（发行版包 pacman 可核实 + 输入风险档位路由），rationale 引本 ADR §2.1/§2.2。
3. **原始裁定保留为历史事实**：PHASE3-DESIGN §2.3 与 ROADMAP:260 原文不改写；本 ADR 为 superseded-by 标注的承载文档（docs/README.md 登记）。若后续批改写原文，须在原行位以行内标注引用本 ADR，不得静默改写历史裁定。
4. **不扩大面**：本修订不授权任何「一律原生化」批量迁移（NATIVE-FIRST §10:240 验收原文「Manifest 分类有证据，不进行『一律容器化』或『一律原生化』的批量迁移」）；逐工具裁决在 DI 批次内以 allowlist + 档位路由证据逐项落盘。

## 5. DI-1/2/3 入口指针与依赖

```text
DI-0（本 ADR：P3-4 前提修订，冻结现状盘点）
  └─ DI-1 Document Pack 原生首工具（PDF 文本抽取）
       消费：RT-1-TASK.md 的 NativeRustAdapter + RT-1-ADR D3 allowlist 形状
             + RT-0.5-F 契约（capability_refs → pdf.pdftotext/pdf.pdfinfo，
             RT-0.5-C §3.6/3.7 实测条目为种子）
       前置：RT-1 收口（八项判据全绿）；manifests/packs/planned/document.json
             存根修订（本 ADR §4.2）随 DI-1 S0 执行
       判据：ROADMAP:260 判据 1 保留版，PDF 解析工具一个 run 端到端
             （可信低风险档受限原生；不明来源输入档拒绝或路由隔离）
  └─ DI-2 Office/OCR 链路
       消费：document.libreoffice（隔离临时 profile，C §3.4:81 约束）
             + ocr.tesseract（chi_tra 缺失显式报告，C §3.3:68）
             + pdf.pdftoppm（渲染像素/页数硬限额，C §3.8:133）
       前置：DI-1（capability_refs 接线复用 + 工作区形态复用）
       判据：Office→文本与 PDF 页→OCR 两条 run 端到端；chi_tra 缺失显式告警
             落信封（不静默降级）
  └─ DI-3 归档/元数据 + 路由整合
       消费：archive.7z 系（C §3.11-3.13）+ meta.exiftool（C §3.16，
             非受信前缀固定绝对路径）+ Document Router（RT-0.5-E §3:52
             路由图最小实现，档位判定与 RT-3 Policy Resolver 的前置承接）
       前置：DI-2
       判据：Router 按 E §1 三档路由分派一条 PDF/归档混合输入链路；
             RT-3 接入点登记
```

依赖要点：

- DI-1 是 RT-0.5-F 契约（F §8:259-265 消费清单）与本批 RT-1 adapter 的**首个消费批**：capability_refs 可选字段解析、Provider get/指纹校验、allowlist 钉值登记三条在 RT-1 已具备最小面，DI-1 只做工具 Manifest 登记与端到端 run，不回改 adapter。
- DI-2/DI-3 不依赖 VM 解锁：podman_vm 保持设计挂起（NATIVE-FIRST §8.1:205）；高风险档行为恒为「拒绝派发并报告 VM 未就绪」（RT-0.5-E §1:17）。
- RT-3（Runtime Policy Resolver）不阻塞 DI 链：DI-3 的 Router 最小档位判定承接其前置职责，RT-3 落地后 Router 判定面移交（依赖方向：DI-3 → RT-3 输入，非反向）。

## 6. 对既有文档的影响清单（本批零改写，回填标注属后续批）

| 文档 | 影响 | 处置批 |
|---|---|---|
| docs/phase0/PHASE-ROADMAP.md §4.3 P3-4 行 | 判据 2 废止 + DI 链标注（superseded-by 本 ADR） | DI-1（回填批，不得静默改写历史裁定，本 ADR §4.3） |
| docs/phase3/PHASE3-DESIGN.md §2.3:53 Document 行 | planned 理由改写指引 | DI-1 |
| manifests/packs/planned/document.json | exit_conditions/trust_ruling 修订（§4.2 全文已冻结） | DI-1 S0 |
| docs/README.md | 本 ADR + RT-1 两份文档登记 | 本批（已执行） |
| manifests/tools/rip.doc.parse.json、workers/python/parsing_worker.py | 零改动（冻结盘点表 §3 口径） | 无（不迁不扩） |
