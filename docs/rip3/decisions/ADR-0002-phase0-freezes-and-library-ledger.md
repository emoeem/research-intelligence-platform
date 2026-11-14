# ADR-0002：Phase 0 四项冻结与选型台账（含能力优先许可裁决）

- 状态：已裁决（2026-11-14，Phase 0 产物随批）；评审入口：docs/rip3/ 四冻结物
- 关联：R2 §2.5（只冻结四项）、§2.6（任务 8）、AGENTS.md 成熟库硬约束（按用户
  裁决修订：许可证记录不做淘汰门槛，能力优先）

## 一、四项冻结清单（R2 §2.5）

| # | 冻结物 | 文档 | 冻结内容 |
|---|---|---|---|
| 1 | 统一领域模型 + RTIS 映射 | 03-DOMAIN-MODEL.md | 实体字段/关系/ID 规范/RTIS 五面映射表 v1 |
| 2 | Document IR Schema | 04-DOCUMENT-IR-SCHEMA.md | IR v1 结构全文/块词表 14 值/版本路由 |
| 3 | 存储、CAS 与迁移策略 | 12-STORAGE-AND-MIGRATION.md | 四层分层/CAS 语义/PG-0001+ refinery/SQLite 020+/备份双轨 |
| 4 | 安全边界与数据隔离 | 13-SECURITY-AND-PRIVACY.md | 继承清单/不可信文件约束/数据通道三件/权限前置过滤 |

未冻结项（按阶段 ADR 滚动，R2 §2.5/D5）：RLS 语句、KnowledgeCard 子 Schema、
索引参数、分块策略默认值、渲染器模板。

## 二、选型台账（能力优先，许可仅记录：用户 2026-11-14 裁决）

| 能力 | 选型 | 版本锚点 | 许可（记录） | 引入时机 |
|---|---|---|---|---|
| 情报核数据库 | PostgreSQL 17 | pgvector/pgvector:pg17 镜像 digest | PostgreSQL Licence | P1-1 |
| 向量 | pgvector 0.8 + pgvector (Rust crate) | 实施时钉版 | Apache-2.0 | P1-1 |
| BM25 | ParadeDB pg_search | 实施时钉版 | AGPL（记录，能力优先） | P1-1 镜像内 |
| CJK 分词 | pg_jieba | 实施时钉版 | BSD-3 | P1-1 镜像内 |
| 同步驱动/池 | postgres 0.19 + r2d2_postgres（备选 deadpool-postgres） | docs.rs 复核后钉版 | MIT/Apache-2.0 | P1-1 |
| PG 迁移 | refinery | 实施时钉版 | MIT/Apache-2.0 | P1-1 |
| 文档解析 | Docling 2.x（Granite-Docling 可选） | 镜像内钉版 | MIT | P2-1 |
| PDF 精细面 | PyMuPDF | 镜像内钉版 | AGPL（记录，能力优先） | P2-1 |
| PDF 备选 | pypdfium2 | 镜像内钉版 | Apache-2.0/BSD | P2-1 |
| OCR | Tesseract（兜底）；PaddleOCR（条件：中文实测不达标） | 镜像内 | Apache-2.0 | P2-4 |
| 近似去重 | datasketch 2.0 | 钉 2.0.x（排列方案变更） | MIT | P3-2 |
| 重排 | bge-reranker-base ONNX + onnxruntime | 模型 sha256 锚定 | Apache-2.0/MIT | P4-3 |
| Token 计数 | tokenizers crate（bge tokenizer 离线烘焙） | 钉版 | Apache-2.0 | P4-4 |
| 报告 PDF | typst 0.13 + typst-as-lib | 钉版 | Apache-2.0 | P6-2 |
| DOCX 渲染 | docx-rs 或 python-docx | P6 ADR 定夺 | MIT | P6-3 |
| 图表 | Mermaid（web）+ typst cetz/fletcher（PDF 路径） | 钉版 | MIT | P6-4 |
| 压缩/魔数 | zip crate + infer（或 file-format） | 钉版 | MIT | P1-4 |
| 换后端备选 | LanceDB / Qdrant | 不引入，基准不达标启用 | Apache-2.0 | 备用 |

## 三、裁决记录

1. 数据库绞杀者路线：用户 2026-11-14（详见 ADR-0001）。
2. 许可证不设淘汰门槛、能力优先：用户 2026-11-14；AGPL 组件在容器/服务侧使用并记录。
3. Phase 0 产物随批 commit/push：用户 2026-11-14。

## 四、后果

- 实施期每库引入走：增量 cargo vendor（Rust）或容器镜像（Python/PG），OSV 复查、
  实施时钉版本 + digest 台账三处同步（既有惯例）。
- 本台账为「成熟库优先」的 Phase 0 记录面；各阶段实施 ADR 引用本表行号级选型。
