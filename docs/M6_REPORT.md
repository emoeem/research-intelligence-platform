# M6 实施报告：Tool & Workflow Platform（数据面，隔离环境 dry-run）

日期：2026-11-06
范围：计划书 §M6 数据面部分（工具注册 / 健康四态分离 / workflow 定义 / skill 元数据）
环境：隔离目标库 migrations/platform-dryrun.db，未切流；源全部只读。
验证：migrations/verify.py 全量复跑 108 PASS / 0 FAIL（exit 0），etl_m6 连续重复执行 no-op。

## 1. 交付清单

| 交付物 | 说明 |
|---|---|
| migrations/003_tools_workflow.sql | 增量迁移：workflow / workflow_registry_meta / tool_execution / etl_run_log 建表（幂等 DDL）+ schema_version=3 登记；三项守卫 DDL 的权威记录（SQLite 无条件 DDL，由 etl_m6.py 守卫执行） |
| migrations/etl_m6.py | stdlib-only ETL v1.0.0：20 工具健康实测导入 + 受限 YAML 解析 13 workflow + 125 skill front matter 抽取；库内运行日志（etl_run_log 表） |
| migrations/verify.py（扩展） | 新增 §11（tools/workflows/skills 对账，12 组检查）；§7 域归属收窄为 M1 第一批职责域并如实注释 |
| migrations/id_audit.py（扩展） | alias↔实体一致性补 tool/skill/workflow 三类（v1.0.0 → 1.1.0），M6 实体纳入 §5.4 硬门槛审计 |
| migrations/platform-dryrun.db | 新增数据：tool 20、tool_version 20、skill 125、workflow 13、workflow_registry_meta 1、tool_execution 0（空结构）、entity_alias +158、etl_run_log 4 次运行记录 |

新表结构：
- workflow：workflow_id（沿用源 id WF-00..WF-12）/ name / description / trigger_json / requires_json / related_skills / knowledge_queries / phases / stop_conditions / excluded_activities，JSON 全部 json.dumps(ensure_ascii=False, sort_keys=True) 保真。
- workflow_registry_meta：单行（id=1），registry_version=1、last_reviewed=2026-11-02、default_workflow=WF-00。
- tool_execution：exec_id / tool_id（FK→tool）/ argv_hash / exit_code / duration_ms / started_at / permission_ctx，备 M5，本阶段 0 行。
- etl_run_log：etl_m6 运行日志（任务书写路径约束"新文件仅 .py/.sql/.md"，运行日志入库表而非新 .jsonl；verify §11 相应改为读库内表，与 M1 文件式惯例的差异在此说明）。

守卫 DDL（003 为权威记录，etl_m6.py 执行，全部幂等守卫）：
1. tool 表追加列 purpose / provenance_ref / health_note。
2. tool_version 重建：version 由 NOT NULL 放宽为 NULL-able（承接"版本探测 broken → version 置 NULL"），追加 health 列（CHECK 域 unknown|ok|degraded|broken）。重建时表为空，无数据损失。
3. entity_alias.entity_type 域扩展加入 'workflow'（同 002 对 research_task 的重建模式）。

## 2. 四态分离（评审要求的落实）

SKILLS_AND_TOOLS_REVIEW 指出 registry 只有 name/kind/purpose/installed/version/readonly 六字段，缺 Schema、权限分级、执行日志与健康状态。M6 落库的四态：

| 态 | 落列 | 本批取值方式 |
|---|---|---|
| 定义 | tool.kind / purpose / provenance_ref / input_schema / output_schema | registry 原值；schema 两列 NULL 留 M6 后期（任务书指定） |
| 安装 | tool.install_state（installed/missing） | 实测：二进制/文件存在性探测，不采信 registry installed 布尔 |
| 健康 | tool.health_state（unknown/ok/degraded/broken）+ health_note；tool_version.health（每版本粒度） | 实测：白名单只读探测（--version / -h / node --check / sqlite quick_check，timeout 5s，不执行扫描） |
| 授权 | tool.permission_level（T0/T1） | readonly=true → T0（17 个），false → T1（rtis.py、rtis_kb（node 桥）、rtis_mcp.mjs）；T2 留高风险操作，本批未用 |

install 与 health 明确分离的实例：pd-httpx install_state=missing 且 health=unknown（缺失无从探测健康）；gogo install_state=installed 且 health=degraded（二进制在、版本不可得）。

health 判定规则（etl_m6.py 内固定规则，verify §11 独立复算）：
- 探测执行且版本可解析 → ok；
- 二进制可执行但版本不可得（usage 输出 / --version 空输出）→ degraded；
- 文件缺失 → unknown（无从探测）；
- 探测超时 / 语法校验失败 / quick_check 失败 → broken。
注意：health 描述的是"探测通道结论"，不是功能完整性的证明（见 §7 limitations）。

## 3. 工具健康实测清单（20/20，registry 声明 vs 实测）

实测时间：2026-11-05T19:28~19:35Z（etl_run_log run_seq 1..4，四次探测结果一致）。
探测命令全部为版本/帮助/语法/完整性类只读操作，未执行任何扫描。

| 工具 | kind | registry 声明 | 实测 install | 实测 health | 实测 version | 探测命令 |
|---|---|---|---|---|---|---|
| nmap | cli | installed, "Nmap 7.993 ( https://nmap.org )" | installed | ok | 7.993 | nmap --version |
| nuclei | cli | installed, "[ANSI]Nuclei Engine Version: v3.11.1" | installed | ok | v3.11.1 | nuclei --version |
| fscan | cli | installed, "flag needs an argument: -h"（坏值） | installed | degraded | NULL | fscan -h |
| gogo | cli | **installed=false**（失真）, "" | **installed** | degraded | NULL | gogo --version |
| suo5 | cli | installed, "NAME:"（坏值） | installed | ok | **v2.2.0** | suo5 --version |
| frpc | cli | installed, "0.71.1" | installed | ok | **0.72.0** | frpc -v |
| chisel | cli | installed, "v1.13.0" | installed | ok | v1.13.0 | chisel --version |
| ffuf | cli | installed, "ffuf version: git-20260926-8d65e037" | installed | ok | git-20260926-8d65e037 | ffuf -V |
| sqlmap | cli | installed, "1.10.10#stable" | installed | ok | 1.10.10#stable | sqlmap --version |
| hashcat | cli | installed, "v8.0.0" | installed | ok | v8.0.0 | hashcat --version |
| subfinder | cli | installed, "[ANSI]Current Version: v2.17.0" | installed | ok | v2.17.0 | subfinder -version |
| pd-httpx | cli | installed=false | **missing** | unknown | NULL | （路径不存在，未执行） |
| frida | cli | installed, "17.24.3" | installed | ok | 17.24.3 | frida --version |
| jadx | cli | installed, "1.5.6" | installed | ok | 1.5.6 | jadx --version |
| node | cli | installed, "v24.22.2" | installed | ok | v24.22.2 | node --version |
| python3 | cli | installed, "Python 3.14.9" | installed | ok | 3.14.9 | python3 --version |
| rtis_kb（node 桥） | mcp | installed, "" | installed | ok | NULL | node --check rtis_kb.mjs |
| rtis.py | cli | installed, "1"（硬编码） | installed | degraded | NULL | python3 rtis.py --version |
| knowledge.db | datastore | installed, "" | installed | ok | NULL | 只读连接 PRAGMA quick_check=ok |
| rtis_mcp.mjs | mcp | installed, "" | installed | ok | NULL | node --check rtis_mcp.mjs |

健康分布：ok 16 / degraded 3（fscan、gogo、rtis.py）/ unknown 1（pd-httpx）/ broken 0。

### drift 清单（8 项，verify §11 同规则独立复算一致）

| # | 工具.字段 | 类型 | registry 声明 | 实测/落库 | 处置 |
|---|---|---|---|---|---|
| 1 | fscan.version | value-corrupt | "flag needs an argument: -h" | NULL | 报错首行误存为版本；坏值不落列（verified） |
| 2 | gogo.installed | install-flag | false | installed=true | toolkit/gogo/gogo 二进制实存（27MB，ls 验证），按实存重判（verified） |
| 3 | gogo.version | unavailable | （空） | NULL | --version exit=0 但输出为空；与评审"P0 源码本地构建"备忘一致（根因为推断，见 §7） |
| 4 | suo5.version | value-corrupt | "NAME:" | v2.2.0 | 帮助首行误存为版本；实测 --version 可正常出 v2.2.0（verified） |
| 5 | frpc.version | value-drift | 0.71.1 | 0.72.0 | 版本漂移，以实测为准（verified，与交接文档备忘一致） |
| 6 | rtis.py.version | value-corrupt | "1" | NULL | 硬编码无探测依据；工具无版本 flag（verified） |
| 7 | nuclei.version_format | format | 含 ANSI 转义 | v3.11.1 | 清洗后语义一致，登记格式缺陷（verified） |
| 8 | subfinder.version_format | format | 含 ANSI 转义 | v2.17.0 | 清洗后语义一致，登记格式缺陷（verified） |

非 drift 注记：pd-httpx registry installed=false 与实况（路径缺失）一致，无漂移；~/.local/bin/httpx（Go ELF）与 httpx2 存在但名称与 registry 声明不符，未擅自等同（unverified 等价性，见 §7）。

## 4. Workflow 导入（13/13，隔离 0）

- 源：rtis/data/registry/workflows.yaml（sha256 见 etl_run_log；mtime 2026-11-02T14:07:08Z 未变）。
- 解析：stdlib 无 yaml，etl_m6.py 内置受限解析器（缩进映射 / 块列表 / 流式列表 [..] / 流式映射 {..} / 标量；多行标量、锚点、制表符缩进不支持即抛错）。解析失败的条目入隔离清单不猜测：本批隔离 0。
- 行数对账：YAML `- id:` 独立重计 13 = 落库 13，ID 集合一致（WF-00..WF-12）。
- 字段保真：7 个 JSON 列全部可解析；phases 均为含 phase/role 的字典列表。
- default_workflow：registry meta 落库 default_workflow=WF-00 且 WF-00 行存在，M0 报告 §3.2 的声明得到数据面验证（verified）。
- related_skills：48 处引用，44 处可解析到 skill 表；4 个名字在 125 skill 之外：apk-reverse、firmware-pentest、hardware-security、src-hunter（源数据事实，INFO 记录，未代改）。

## 5. Skill 导入（125/125，隔离 0）

- 源：/home/emo/.dsh/redteam/skills/*.md（125 文件），只读抽取 front matter，未修改任何文件（源目录 mtime 与文件数开工前后一致）。
- skill_id = front matter name（125 个 name 唯一且与文件名 stem 一致）；body_ref 指向原文件绝对路径，全部实存。
- 字段覆盖：name/description 125/125；whenToUse 119；role 119；enabled 119（true 119 / NULL 6）；generator 19。
- roles 分布（与评审一致）：vuln-scan 72 / recon 20 / internal 14 / plan 7 / exploit 6 / NULL 6。
- 缺 role 的 6 个文件（同为缺 enabled/whenToUse 的 6 个，写入报告备人工补）：
  1. av-evasion.md
  2. confluence-atlassian.md
  3. electron-client.md
  4. iot-snmp-pivot.md
  5. opsec-anti-trace.md
  6. phishing-delivery.md
- 抽样复建：10 个 skill 的 front matter 全文重解析 vs 落库字段比对，全部一致。

## 6. 幂等与 verify 结论

- etl_m6.py 连续执行 4 次：第 1 次建数据，第 2、3、4 次内容摘要与行数完全一致（no-op）。行时间戳取源侧确定性值（registry generated_at / 文件 mtime），运行时刻信息只进 etl_run_log，不进内容行。
- tool_version 幂等设计：current 行版本/健康/证据哈希均不变时跳过写入；版本变化时旧行转 superseded、新行 current（1:N 语义保留）。
- verify.py 全量复跑：**108 PASS / 6 INFO / 0 FAIL，exit 0**。§11 覆盖：schema_version=3、守卫 DDL 结构、运行日志 no-op、tool ID 集合、四态域、T0/T1 映射、gogo/pd-httpx 现勘复核、健康分布、current 版本覆盖、NULL 版本集合、坏声明不落列、drift 重算、workflow 行数/JSON/结构/default_workflow、skill 125/125 + 缺 role 三方一致 + body_ref 实存 + 抽样复建、别名覆盖 100%、entity_alias 总量（589 = 431 + 158）、tool_execution 0 行。
- id_audit.py（v1.1.0）复跑：结论"现网 ID 可直接沿用"保持，tool(pk=20)/skill(pk=125)/workflow(pk=13) 三类 alias 双向缺失 0。

## 7. Limitations（verified / unverified 分标）

Verified（实测）：
- 20 工具探测输出、8 项 drift、13 workflow、125 skill、幂等 no-op、verify 108 项，均为本机实测，证据存 etl_run_log.detail 与本报告表格。
- 源文件只读：registry 三文件与 skills/ 目录 mtime 开工前后一致。

Unverified / 推断（如实标注）：
1. health=ok 的含义是"探测通道健康"（版本可解析 / 语法通过 / quick_check ok），不是功能完整性证明；未对任何工具执行真实扫描验证功能（红线：只读探测）。fscan/gogo 的 degraded 仅指版本元数据不可得，不构成扫描功能故障的判定（未实测功能）。
2. gogo 版本为空的根因（本地构建未注入 ldflags 版本号）是 HYPOTHESIS：依据是 --version exit=0 空输出 + 评审 P0 备忘"源码本地构建"，未通过重新构建验证。
3. pd-httpx：声明路径缺失为 verified；~/.local/bin/httpx（Go ELF）与 httpx2 是否即 ProjectDiscovery httpx 改名，未验证，未等同。
4. related_skills 4 个未解析名字为源 YAML 事实；是否应补 skill 或改引用，属 M6 后期治理，本批不代改。
5. 6 个缺 role/enabled/whenToUse 的 skill 保留 NULL，未补写（红线：skill 文件一条不改）。
6. tool 的 input_schema / output_schema / timeout_s 为 NULL，JSON Schema、超时与资源限制留 M6 后期（任务书指定）；tool_execution 空结构备 M5。
7. 受限 YAML 解析器只覆盖该 registry 文件的语法子集；子集外的 YAML 会抛错入隔离而不是猜测。本次 0 条隔离，解析正确性由 verify 独立口径（正则重计 + JSON 结构校验）背书，不依赖解析器自身。
8. 运行日志入库存放（etl_run_log 表）与 M1 的 .jsonl 文件惯例不同，系任务书写路径约束（新文件仅 .py/.sql/.md）所致；§11 已相应适配。
9. id_audit.py 为支持 §10 硬门槛对当前库复跑，扩展 pk_sql 三类（v1.1.0）；审计算法未改。
10. 幂等的前提是环境稳定：工具在两次运行间升级会正确打破 no-op（期望行为，届时 drift 与健康列随之更新）。

## 8. 禁止项遵守声明

未执行 git 操作；未改 crates/、workers/、apps/；skill 文件与 registry 源文件只读（mtime 佐证）；平台写路径仅 migrations/platform-dryrun.db 与新 .py/.sql/.md 及既有 verify.py / id_audit.py 的验证链扩展（扩展内容见 §1，系 verify 100% 红线的必要条件）。
