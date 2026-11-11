# SK-2 三源知识来源接入备注（P6-10）

| 项     | 值                                                                                     |
| ------ | -------------------------------------------------------------------------------------- |
| 状态   | 生效中（SK-2 批次内落稿，随批次演进）                                                    |
| 日期   | 2026-11-12                                                                              |
| 基准   | docs/RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md §3/§4.2/§5/§7/§8 P6-10、docs/phase6/SK1-INGEST-ADR.md |
| 相关   | docs/phase6/RTIS-REDTEAM-PLUGIN-NOTES.md（体例参照）、docs/phase0/PHASE-ROADMAP.md、docs/phase6/PHASE6-PLAN.md |

## 一、三源登记与许可裁决

三份独立 Manifest 落位 `manifests/plugins/`（yaml/json 双份，经 schemas/plugin-manifest/v1.json 全文校验 + validate rule 3 per-kind 声明），source.type=git + pinned.kind=commit，三个固定 Commit 在 `git ls-remote` 实查与 clone HEAD 逐字节一致（2026-11-12）：

| Plugin ID | 仓库与分支 | 固定 Commit | license 裁决 | content_kinds |
| --- | --- | --- | --- | --- |
| knowledge.awesome-hacking | github.com/Hack-with-Github/Awesome-Hacking @ master | 440486fd08b0a0d9ad9a041a0bb234dcdf65dee2 | CC0-1.0（顶层 LICENSE 实查为 CC0 1.0 Universal），catalog_only | [catalog_entry] |
| knowledge.secskills | github.com/DaoYiSec/SecSkills @ main | 1867eb6b71889af8632107884e9d0235510f9f78 | Apache-2.0（顶层 LICENSE 实查），catalog 条目 catalog_only + 候选 metadata_only | [catalog_entry, skill_candidate] |
| knowledge.tongui-course-notes | github.com/tongui66/Cyber-Security-Course-Notes @ master | 45a12a759e82adec13c3d171a34d1cbeea1a15b3 | UNLICENSED（顶层无 LICENSE 文件，计划 §3.3 核实值 2026-11-12 复核成立），metadata_only | [catalog_entry] |

许可纪律落地：

1. 目录许可不传递（计划 §7 第 1 条）：awesome-hacking 的 CC0 与 secskills 的 Apache-2.0 只覆盖各自仓库自身资料，外链项目不 clone、不安装、不执行，只登记条目元数据与原始 URL。
2. tongui 课程正文（74 篇 .md 全文）不批量持久化、不批量镜像：数据库面 document_markdown 恒 0（§五断言 1），RIP 仓库 fixture 也不含正文（tests/fixtures/sk2/FIXTURE-NOTES.md 纪律 2）。全文持久化需先取得作者许可。
3. 推广与娱乐条目不过滤删除、只挡在检索面外：mseep 徽章、中转站裸链（ai.daoyivip.com，非 Markdown 链接语法，解析器天然不提取）、secskills「娱乐」分类 12 条等全部以 DISCOVERED/可 REJECTED 形态入库待审，本批全量 APPROVED 属于对账演示口径，治理面可随时翻转（knowledge_review 受控写面）。

## 二、摄取形态与条数对账

摄取管线零改动复用 SK-1 半成品（ingest.rs 的 markdown_links 解析 + knowledge_source_entry 写面），三源对账结果：

| 源 | 解析入口 | 实测条数（counts.catalog_entry） | 过滤计数 | 对账基准 |
| --- | --- | --- | --- | --- |
| awesome-hacking | README.md | 85 | no_scheme=2（contributing.md 两处相对链接） | 分类表 47+34=81 行 + 仓库自链/徽章图/Twitter/Facebook 4 条 = 85，与独立 python 逐行模拟（镜像 knowledge_parse.rs 语义）85 唯一键一致 |
| secskills | README.md | 102 | no_scheme=17（分类目录锚点 17 条） | README「共计 102 个」实数：102 表行中 skill-dfyx_code_security_review 两条同 URL 合并为 1 唯一键 = 101，加 mseep 徽章 1 = 102 唯一键 |
| tongui | catalog/articles.md（派生目录） | 75 | 0 | 74 篇课程 + 1 来源主页；精选 31 篇合并（§三） |

对账口径说明：secskills README 第 98/99 行是上游原文的两条同 URL 重复条目，同趟摄取第二条命中 UNIQUE(plugin_id, plugin_version, entry_kind, item_key) 计 skipped=1（首跑报告实值），这是规范化 canonical URL 去重生效的实测形态，不是幂等异常；二跑 skipped=103（102 落表行 + 1 源内重复）changed=0。

条数对账的独立校验：python 模拟器逐行复刻 extract_markdown_links 语义（围栏、图片前缀、首括号收口、scheme 白名单），对三份源文件算得 85/103 raw（102 唯一）/75，与 Rust dry-run 报告逐数一致；模拟脚本过程记录在批次会话（本节表格即其输出固化）。

## 三、tongui 派生目录与站点/仓库合并（批次偏差登记）

**偏差：tongui 的 catalog_entry 不来自上游树直摄，来自 RIP 侧派生目录文件 catalog/articles.md（scripts/sk2_tongui_catalog.py 生成）。** 原因（全部实测）：

1. 上游原稿树整仓只有 1 个绝对 URL（README 的博客首页链接）；README 的 46+ 条文章链接全部是仓库内相对路径，markdown_links 解析器按 no_scheme 过滤（计划 §6.2A「不从文字猜造 URL」的红线语义，解析器行为正确）。
2. 精选站点（tongui66.github.io/course-notes，Hexo）的文章 slug（如 chapter1-web-dev/23-Nginx-基础与-HTTP-协议）与 Git 路径（渗透测试全套/04-Web架构与开发基础/23-Nginx基础与HTTP协议.md）互不可推导，映射键是文章 frontmatter 的「编号」字段。
3. 因此「74 篇的章节/标题/URL/路径元数据登记」在固定 Commit 的上游树上不可原样解析得出，需要一个显式、可复核、可重现的派生步骤。

派生与合并语义（生成脚本实跑输出：74 篇 + 1 来源主页 = 75 条，精选合并 31/31）：

- 精选 31 篇：canonical_url = 站点 URL（第一 source_ref），Git 路径与 GitHub blob URL 落在条目描述（第二 source_ref）；同文章只占一个逻辑资源。
- 未精选 43 篇：canonical_url = blob URL；Git 路径同样在描述。
- Git 路径是正文权威来源标注（「以 Git 路径为正文来源」，计划 §4.2），正文本体仍不落库不入仓（UNLICENSED）。
- 配对闭环校验在脚本内强制：站点链接数 != 31、编号配对不满、仓库文章数 != 74 任一不满足即非零退出不产出。

上游树直摄的实跑证据（git 通道安装后的包直摄）：counts 为空（0 条目），filtered 空，exit 0。该实跑证明解析器对未派生树的行为是诚实的零，而不是报错或编造条目；也即：上游若在未来版本自带绝对 URL 目录，本插件无需改 Manifest 即可切换为直摄。

## 四、CLI 实跑链（RIP_DATA_DIR=/tmp/sk2-data，--db /tmp/sk2.db）

| 步骤 | 命令（均 exit 0） | 关键回显 |
| --- | --- | --- |
| install（git 通道） | rp-plugin --db /tmp/sk2.db install --manifest manifests/plugins/knowledge.awesome-hacking.yaml --trust community | plugin.lock pinned={commit, 440486fd...全 40 位}，negotiation.resolved_commit 与 pinned 相等；secskills/tongui 同形 |
| ingest --dry-run | rp-plugin --db /tmp/sk2.db ingest <id> --dry-run | ah: counts 85/no_scheme 2；secs: 102/17/skipped 1；tg: 75/0；dry_run=true 零落盘 |
| ingest 实跑 | 同上去 --dry-run | 与 dry-run 计数逐数一致 |
| 幂等二跑 | 同上 | ah skipped=85、secs skipped=103、tg skipped=75，changed 全 0 |
| enable | rp-plugin --db /tmp/sk2.db enable <id>（三源） | REGISTERED → ENABLED（检索双闸之可见闸） |
| review | rp-plugin --db /tmp/sk2.db review <id> --keys <全量键 CSV> --status APPROVED | updated=85/102/75 |
| 索引重建 | rp-search-index build --platform-db /tmp/sk2.db --catalog-db /tmp/sk2.db --index-db /tmp/sk2-index.db --rebuild | doc_count=262=85+102+75（三源全量入检索面） |
| 检索命中 | rp-search-index query --keywords Nmap / 越权 / strix | 三源条目均命中（kse:plugin@version:catalog_entry:<url> 词形） |
| 禁用撤出 | rp-plugin disable knowledge.secskills → rebuild | doc_count=160=85+75，strix 命中 0；复 enable 恢复 262 |

tongui 的 git 通道验证在一次性环境完成（RIP_DATA_DIR=/tmp/sk2-verify-tongui --db /tmp/sk2-verify.db）：install exit 0 + pinned/resolved 一致 + 上游树直摄 0 条目（§三）；随后主环境的安装用 --package tests/fixtures/sk2/tongui-course-notes 本地 staging（plugin.lock pinned 仍取 Manifest source.pinned 全 40 位）。

## 五、只读断言与红线核查（python3，file:ro）

| 断言 | 实查 | 结果 |
| --- | --- | --- |
| tongui document_markdown 条数 | count WHERE plugin_id LIKE 'knowledge.tongui%' AND entry_kind='document_markdown' | 0，PASS（无许可正文未批量持久化） |
| license_status/rights_policy 分组 | GROUP BY 1,2 | (Apache-2.0, catalog_only, 102) / (CC0-1.0, catalog_only, 85) / (UNLICENSED, metadata_only, 75)，PASS |
| tongui 逻辑资源 | description LIKE '%Git路径=渗透测试全套/%' | 74，PASS；来源主页恰 1 条 |
| 精选双 source_ref | 站点 URL canonical + Git 路径 + Blob 描述 | 31，PASS |
| source_commit | 三源全行 = 固定 Commit 且 40 位 | 违规 0，PASS |
| 审核双闸 | review 分组 + plugin state | APPROVED 262 / 其余 0；三插件 ENABLED |
| 唯一键 | count vs count(DISTINCT item_key) | 85/102/75 全等，PASS |
| RTIS 生产零触碰 | sha256 integrations/rtis/knowledge.db | f8580feb...（与 P6-5 基线前缀一致，全程未变）；git status --porcelain -- integrations/rtis migrations/platform.db 零输出 |

## 六、已知取舍与移交

1. upstream_missing 自动标记未实现：011 已建列，ingest 写面固定 0；「上游删除转 DEPRECATED」的自动翻转需要新快照对比触发，本批三源各只有一个固定 Commit，无触发场景。物理删已由测试面锁定为不发生（sk2_source_ingest::sk2_changed_counts_preserve_history_and_no_physical_delete：删除目录行后重摄，历史行保留）；自动翻转归 rip.knowledge.source.sync（计划 §6.2D，P6-11）。
2. secskills skill_candidate 本快照计数 0：上游只打包 README/LICENSE/图片（计划 §3.2 仓库结构核实值），skills/ 目录不存在，Manifest 声明与 metadata_only 策略已就位，候选以 catalog_entry 形态登记（Agent/代码审计/MCP 分类）。
3. 解析器已知形态（不改 knowledge_parse.rs 的批次约束下如实登记）：awesome-hacking 表格行首列是链接，分类路径含 README H1 的原始 Markdown（含徽章链接），Web 分类展示可见该原文；secskills 条目描述列（链接左侧的表格单元格）不在 markdown_links 的「链接后缀描述」语义内，条目描述为表格分隔符「|」，标题/分类/URL 三元不受影响。
4. 50 URL 抽样全过（§七），但 awesome-hacking 外链项目的长期存活属时效治理（计划 §9），后续周检按 upstream_missing/DEPRECATED 语义处理。
5. Web 视觉门禁（web/qa/visual playwright）本机无法运行：系统无 chromium 二进制（与 SK-1 podman_host_egress 同类环境缺口），vitest 73 项全绿 + browser-use 真实浏览器 E2E（§八）为本批的浏览器级证据。

## 七、50 URL 抽样记录（链接可达 + 分类对账）

方法：APPROVED 全量 262 条中分层抽样 50（ah 17 / secs 17 / tongui 16，random.seed(20261112)）；每条两项校验：

1. URL 可达：GET canonical_url（UA=RIP-SK2-audit/0.1，15s 超时，<400 为过；CJK blob URL 客户端侧百分号编码后请求）。
2. 分类对账：ah/secs 按 source_location 行号在 fixture README 重算标题路径与 category_path_json 全等；tongui 校验 category_path 与描述内 Git 路径在固定 Commit clone 树中真实存在。

结果：50/50 URL 可达，50/50 分类对账，0 失败。逐条明细：

| # | 判定 | HTTP | 来源 | 条目标题 | URL（截断） |
| - | ---- | ---- | ---- | -------- | ----------- |
| 1 | OK | 200 | awesome-hack | Node.js Security | https://github.com/lirantal/awesome-nodejs-security |
| 2 | OK | 200 | awesome-hack | Infosec Getting Started | https://github.com/gradiuscypher/infosec_getting_started |
| 3 | OK | 200 | awesome-hack | HackwithGithub | https://www.facebook.com/HackwithGithub |
| 4 | OK | 200 | awesome-hack | YARA | https://github.com/InQuest/awesome-yara |
| 5 | OK | 200 | awesome-hack | Pentest | https://github.com/enaqx/awesome-pentest |
| 6 | OK | 200 | awesome-hack | Shell | https://github.com/alebcay/awesome-shell |
| 7 | OK | 200 | awesome-hack | Drone Hacking | https://github.com/nicholasaleks/Awesome-Drone-Hacking |
| 8 | OK | 200 | awesome-hack | Tor | https://github.com/polycarbohydrate/awesome-tor |
| 9 | OK | 200 | awesome-hack | Cryptography | https://github.com/sobolevn/awesome-cryptography |
| 10 | OK | 200 | awesome-hack | Vehicle Security | https://github.com/jaredthecoder/awesome-vehicle-security |
| 11 | OK | 200 | awesome-hack | IoT and Hardware Security | https://github.com/kayranfatih/awesome-iot-and-hardware-secu… |
| 12 | OK | 200 | awesome-hack | Forensics | https://github.com/Cugu/awesome-forensics |
| 13 | OK | 200 | awesome-hack | Payloads | https://github.com/foospidy/payloads |
| 14 | OK | 200 | awesome-hack | Web Security | https://github.com/qazbnm456/awesome-web-security |
| 15 | OK | 200 | awesome-hack | Password Cracking | https://github.com/n0kovo/awesome-password-cracking |
| 16 | OK | 200 | awesome-hack | Hacker101 | https://github.com/Hacker0x01/hacker101 |
| 17 | OK | 200 | awesome-hack | Security Cheatsheets | https://github.com/OWASP/CheatSheetSeries |
| 18 | OK | 200 | secskills | Security Audit Toolkit | https://clawhub.ai/gitgoodordietrying/security-audit-toolkit |
| 19 | OK | 200 | secskills | vxcontrol/pentagi | https://github.com/vxcontrol/pentagi |
| 20 | OK | 200 | secskills | pentest-skills | https://github.com/Jumbo-WJB/pentest-skills |
| 21 | OK | 200 | secskills | oritera/Cairn | https://github.com/oritera/Cairn |
| 22 | OK | 200 | secskills | Pentest Api Attacker | https://clawhub.ai/0x-professor/pentest-api-attacker |
| 23 | OK | 200 | secskills | SO Analyzer MCP | https://github.com/1600822305/so-analyzer-mcp |
| 24 | OK | 200 | secskills | transilienceai/communitytools | https://github.com/transilienceai/communitytools |
| 25 | OK | 200 | secskills | dfyx_skills_lab | https://github.com/EastSword/dfyx_skills_lab/tree/main/secur… |
| 26 | OK | 200 | secskills | CyberStrikeus/CyberStrike | https://github.com/CyberStrikeus/CyberStrike |
| 27 | OK | 200 | secskills | andreashappe/cochise | https://github.com/andreashappe/cochise |
| 28 | OK | 200 | secskills | 老板.skill | https://github.com/vogtsw/boss-skills |
| 29 | OK | 200 | secskills | Nmap Pentest Scans | https://clawhub.ai/0x-professor/nmap-pentest-scans |
| 30 | OK | 200 | secskills | KeygraphHQ/shannon | https://github.com/KeygraphHQ/shannon |
| 31 | OK | 200 | secskills | Armur-Ai/Pentest-Swarm-AI | https://github.com/Armur-Ai/Pentest-Swarm-AI |
| 32 | OK | 200 | secskills | LinuxGun-skill | https://github.com/sun977/LinuxGun-skill |
| 33 | OK | 200 | secskills | NYU-LLM-CTF/nyuctf_agents | https://github.com/NYU-LLM-CTF/nyuctf_agents |
| 34 | OK | 200 | tongui-cours | 24 Nginx 进阶配置 | https://tongui66.github.io/course-notes/chapter1-web-dev/24-… |
| 35 | OK | 200 | tongui-cours | 52 SSRF与XXE漏洞 | https://tongui66.github.io/course-notes/chapter2-web-vulns/5… |
| 36 | OK | 200 | tongui-cours | 48 任意文件上传与下载漏洞 | https://tongui66.github.io/course-notes/chapter2-web-vulns/4… |
| 37 | OK | 200 | tongui-cours | 37 PHP 数据库与会话操作 | https://tongui66.github.io/course-notes/chapter1-web-dev/37-… |
| 38 | OK | 200 | tongui-cours | 06 Windows 进阶操作 | https://github.com/tongui66/Cyber-Security-Course-Notes/blob… |
| 39 | OK | 200 | tongui-cours | 70 跨域与渗透测试流程 | https://tongui66.github.io/course-notes/chapter4-intranet/70… |
| 40 | OK | 200 | tongui-cours | 73 安全开发基础(Python入门) | https://github.com/tongui66/Cyber-Security-Course-Notes/blob… |
| 41 | OK | 200 | tongui-cours | 35 PHP 时间日期与文件操作 | https://github.com/tongui66/Cyber-Security-Course-Notes/blob… |
| 42 | OK | 200 | tongui-cours | 21 静态路由与端口映射 | https://github.com/tongui66/Cyber-Security-Course-Notes/blob… |
| 43 | OK | 200 | tongui-cours | 55 逻辑漏洞、挂马与钓鱼 | https://tongui66.github.io/course-notes/chapter2-web-vulns/5… |
| 44 | OK | 200 | tongui-cours | 08 Linux 基本命令与 Vi 编辑器 | https://github.com/tongui66/Cyber-Security-Course-Notes/blob… |
| 45 | OK | 200 | tongui-cours | 25 Nginx 常用变量与动态网站 | https://github.com/tongui66/Cyber-Security-Course-Notes/blob… |
| 46 | OK | 200 | tongui-cours | 42 信息收集之域名与子域名 | https://github.com/tongui66/Cyber-Security-Course-Notes/blob… |
| 47 | OK | 200 | tongui-cours | 62 Windows病毒查杀与Linux入侵排查 | https://tongui66.github.io/course-notes/chapter3-incident/62… |
| 48 | OK | 200 | tongui-cours | 67 内网渗透 | https://tongui66.github.io/course-notes/chapter4-intranet/67… |
| 49 | OK | 200 | tongui-cours | 54 越权、旁注、CDN绕过与登录页面漏洞 | https://github.com/tongui66/Cyber-Security-Course-Notes/blob… |

## 八、Web 展示与浏览器 E2E

服务端（apps/server/rp_server.py，增量字段不破坏 P6-8 契约）：/api/plugins 列表行增 pinned 与 knowledge_stats；/api/plugins/<id> L2 增 homepage/pinned/knowledge_provides（content_kinds + per-kind license_policy）/knowledge_stats；新增 /api/knowledge_sources（RIP_PLUGIN_DB 只读直查，缺表守卫降级，过滤 plugin_id/entry_kind/review_status/q）。

Web（web/src）：PluginsPage 卡片增许可证徽章、快照 Commit 徽章、来源条数与 APPROVED 徽章；PluginDetailPage 元数据区增来源主页链接、快照 Commit、内容类型与许可策略、审核状态计数；KnowledgePage 增「知识来源」视图（平台知识面行为不变），展示审核状态/条目类型/许可证/持久化策略/快照 Commit/分类/原始 URL。

浏览器 E2E（browser-use 真实浏览器 1280x800，vite dev /api 代理 rp_server 7823，RIP_PLUGIN_DB=/tmp/sk2.db，截图入 docs/phase6/browser-e2e/）：

| 文件 | 内容 |
| --- | --- |
| sk2-01-plugins-list-source-badges.png | 插件中心三卡片：许可徽章（CC0-1.0/Apache-2.0/UNLICENSED）、快照短 Commit（440486f/1867eb6/45a12a7）、来源 85/102/75 条 · APPROVED |
| sk2-02-plugin-detail-commit-homepage-review.png | tongui 详情 L2：来源主页链接、快照 Commit 全 40 位、内容类型 catalog_entry·metadata_only、审核状态 APPROVED: 75、审计事件含 plugin.knowledge.ingest |
| sk2-03-knowledge-sources-view.png | 知识库页「知识来源」视图：共 262 条，APPROVED/目录条目/CC0-1.0/仅目录元数据/短 Commit 徽章与分类行 |
| sk2-04-knowledge-sources-tongui-filter.png | tongui 过滤：75 条，UNLICENSED + 仅元数据与哈希 + 45a12a7，分类「渗透测试全套（74 篇，13 章）」，blob URL 可点 |

