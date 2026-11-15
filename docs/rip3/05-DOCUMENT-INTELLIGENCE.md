# RIP3 Document Intelligence（05，Phase 2 实施面）

> 状态：Phase 2 交付（2026-11-15，P2-1 批次 4139ed58 + P2-2 收口批）；
> 判据源 = R2 第四部分 + 04 号冻结物 + ADR-0003。本文为实施面事实登记
> （path:line 级），非重设计。

## 一、系统定位与分工（R2 4.1）

Rust Core 调度与领域落库（intel-db）；重量级解析在容器（rip-parser 族，
network=none）；解析任务生命周期 = workflow 定义（workflow-engine 内核，
不新建队列系统）；隔离执行复用 tool-engine T1 podman_host 面（原则 G）。

## 二、容器族（containerfiles/rip-parser/）

- 基座 `python:3.12-slim@sha256:4840d514…`（同各族）；torch 2.6.0+cpu /
  torchvision 0.21.0+cpu 成对钉版（torchvision 无上界约束曾连带 torch 升
  pypi 全量轮 2.15.0，CPU 源 `--extra-index-url` 钉版修复，首次构建实录）。
- docling==2.150.1（MIT）/ pymupdf==1.28.2（AGPL，ADR-0002 记录不淘汰）/
  jsonschema==4.26.0 / tesseract-ocr 5.5.0（eng+chi_sim apt 烘焙）；
  Docling 模型 `docling-tools models download` 烘焙
  `/opt/docling-artifacts`（2.150 起命令形变更实录：download-artifacts 移除）。
- 构建期受控拉取（pypi/pytorch-cpu/debian/hf-mirror 全 200 实测）；运行期
  network=none（manifest 强制 + 冒烟无网实测）。
- 产物 digest `sha256:9a59eef52bf80aeb2562d9ca32cada455946c775fe92a0b0fb044a527dd8bf7a`
  （版本 0.1.2；digest 变更即版本变更）台账三处同步（manifest yaml/json +
  containerfiles/README.md；provider/测试运行时读 manifest 文件）。

## 三、入口 parser_tools.py（D-TC-03 帧协议）

argv op 分派（core_tools.py/src_tools.py 惯例）：

| op | 引擎 | 输出 |
|---|---|---|
| detect | std-only | 魔法数 + OOXML/EPUB 细分 + sha256 + ZIP 成员清单与安全判定 |
| parse | Docling / PyMuPDF / native_markdown | Document IR v1（实际引擎如实记录 parser_metadata.parser） |
| ocr | Tesseract TSV（词级置信度） | Document IR v1 |

格式路由：pdf/docx/pptx/xlsx/html/image → Docling；epub → PyMuPDF
（get_toc 章节树）；markdown/txt/code → native_markdown；zip 不在 parse 面
（detect 成员清单 + 平台侧逐成员编排，R2 4.2）。

安全边界（13 号冻结物第二节）：/task/in 读限 + 512MiB 帽；ZIP 阈值
（条目 10000 / 单条 512MiB / 总量 2GiB / 压缩比 100 / 穿越拒）；stdout 守卫
（帧 = stdout 第一行，库噪声改道 stderr）；SIGTERM 协作取消。

## 四、Document IR v1（04 号冻结物实施）

- Schema 单一来源 `schemas/document-ir/ir-v1.schema.json`（全层
  additionalProperties:false + conditional required if/then + 词表封闭）。
- 双通道校验：Rust `tool_registry::jsonschema_lite::validate_instance`
  （intel-db/src/ir.rs include_str! 内嵌）+ 容器内 Python jsonschema 自检
  （parser_tools.py self_check_schema，发帧前置）。
- 平台侧树门（schema 无法表达的兜底）：parent 引用存在 / 环检测 /
  structure.root.children 与父为 root 的块一致 / 全树连通（单树）。
- 提交侧 revision 域 ID 重映射（同内容多版本不撞 document_blocks 全局 PK；
  确定性重放）。
- 版本路由：schema_version 字符串常量 "1"，v1 读取器唯一实现（04 号第四节
  演进规则）；旧 IR 文本永不改写。

## 五、Parser Registry + Provider（R2 10.3，ADR-0003 裁决 2/3）

- Trait：`DocumentParser`（detect + parse）/ `OCRProvider`（ocr），
  领域层不写死供应商；路由表 `route_parse`（10 格式 → rip.parser.docling
  单点，ocr → rip.parser.ocr，detect → rip.parser.detect）。
- `PodmanParserProvider`：tool-engine submit（D-TC-05 受理）→
  runtime-gateway Gateway::dispatch（podman_host 节点健康面）→
  `execute_container_via_engine`（隔离全套/看门狗/预算帽/FamilyLimiter/
  tool_run 审计/trace 六 ID 关联全部继承）；ops 运行库专用
  （RIP_DATA_DIR 口径，enable_wal），生产 platform*.db 零触碰。
- manifest：rip.parser.detect / rip.parser.docling / rip.parser.ocr
  （family=document，13 族词表；T1；resources.memory 4Gi 族级帽）。

## 六、解析任务生命周期 = workflow 定义（R2 4.6）

`workflows/parse-pipeline-v1.json`：九态映射（DISCOVERED=导入完成触发 run；
VALIDATING=detect 节点；QUEUED=run 执行前（FamilyLimiter 在派发层）；
PARSING/NORMALIZING/EXTRACTING=parse 节点（IR 内规范化与结构抽取合并承载）；
QUALITY_CHECK=condition 节点（overall=poor 或 POSSIBLE_PROMPT_INJECTION）；
REVIEW_PENDING=human 节点（018 awaiting_approval）；APPROVED=end；
FAILED/CANCELLED/QUARANTINED=run 终态与 quarantine 分支）。checkpoint/
断点恢复/重试继承 workflow-engine（不重写内核）。

rp-workflow 驱动扩展：podman_host manifest 分派（python_worker 逻辑零变更
抽出复用）+ run 级 `input_host_files` 工作区预置（工具 input_schema 零污染）+
manifest resources.memory 随帽 + network none 之外拒绝。

提交模式（ADR-0003 裁决 4）：run 内 Tool 节点产出 IR（节点 output_json）；
PG 落库由驱动侧提交服务在 run 终态后执行，两条通道：
1. `rp-doc parse`（直连面：容器派发 + 提交一体，幂等门 replayed=true，
   --force 强制重解析）；
2. `rp-doc parse --from-run <workflow_run_id>`（接续面：读 ops 库
   workflow_run_node.output_json 的 IR + tool_run(trace_id) 工作区资产，
   落库不经容器）。

## 七、解析质量检查（R2 4.4 + 5.3）

容器侧五检项（toc_present / page_coverage / empty_ratio / font_glyphs /
table_integrity）+ overall（poor≥2 败 / degraded 1 败 / good）+
LOW_EXTRACTION_CONFIDENCE（poor 或均置信 <0.6）+
POSSIBLE_PROMPT_INJECTION（高信号单命中或低信号 ≥3 命中，13 号冻结物第三节
形态集）；poor 或注入命中 → REVIEW_PENDING（QUARANTINED 候选进人工审核，
approve 面可追溯）；页眉/页脚/页码去噪（D2 面，丢弃计数进 self_check detail）。

## 八、落库面（PG V0003）

document_assets（as- 前缀，content_objects 引用账）+
document_revisions 增列（ir_content_hash CAS 凭证 / extraction_quality JSONB /
parse_state CHECK 12 词）+ document_blocks.structured_payload（table rows /
code source 载荷；image_ref 由 assets.source_block_id 承载）。引用计数口径
扩展：ref_count = source + revisions + document_assets 行 + ir 列（commit.rs
重放减账/加账同事务）。

## 九、CLI 面（R2 13.4 同 Application Services）

`rp-doc parse / parse-status / approve`（JSON stdout，rp-* 家族 exit 约定）；
REST 面沿用 P1-2 通道（rp-doc 子进程口径），解析面不新增第二套接口风格。

## 十、已知边界与观察面（如实登记）

1. docling 对部分 11pt 中文段落误判 heading（layout 模型分类行为，IR 如实
   记录不遮蔽；观察面）。
2. HTML 链接文本拆段为 docling HTML 后端行为。
3. 长解析 Parallel 分片：定义骨架在位，多分片调度实测随规模实测批（Phase 8）。
4. /documents 页解析状态卡随 Phase 5/6 页面族统一扩展（P1 遗留延续）。
5. CI 无 PG service：intel-db 集成测试 CI 侧 V-9 跳过（本地真容器实跑为准）。
