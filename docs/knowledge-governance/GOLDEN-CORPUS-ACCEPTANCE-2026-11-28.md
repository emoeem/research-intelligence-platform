# PB-001 Golden Corpus 多格式真实摄取验收报告（2026-11-28）

## 结论

**PASS。** 41 份真实（非夹具）文档经宿主优先解析管线（`rp-doc import → parse`）
全部摄取并解析成功，8 种格式全覆盖；重解析幂等实证（`replayed=true`、状态不变）；
内容对象 CAS 去重实证。执行于隔离 PostgreSQL 容器（digest 钉死），生产
`migrations/platform*.db` 零接触。

## 执行面

| 项 | 值 |
|---|---|
| 摄取脚本 | `scripts/rip_golden_corpus_ingest.py --execute`（既有自动化，逐文件 import+parse） |
| 语料构建 | `scripts/rip_golden_corpus_build.py`（本批新增，见下节） |
| 语料目录 | `/tmp/golden-corpus-pb001`（manifest.json 含逐项 sha256/bytes/origin） |
| 隔离 PG | `localhost/rip-pg17:latest@sha256:229189e4c66a2eb8…`，端口 25432，独立数据卷 |
| 解析器 | 宿主优先路由（Poppler/Tesseract/LibreOffice/Python Worker），零容器依赖 |
| 红线 | 生产 `platform*.db` / RTIS `knowledge.db` 零写；无网络请求；零个人文件（~/Downloads 未使用） |

## 语料构成（41 份，全部真实内容）

| 格式 | 份数 | 来源 |
|---|---|---|
| pdf | 16 | /usr/share/doc 系统真实 PDF（单份 ≤2 MiB，见"已知边界"） |
| md | 15 | 本仓库 docs/ 真实工程文档 |
| html | 5 | /usr/share/doc 系统真实 HTML |
| txt | 3 | /usr/share/doc 系统真实 TXT |
| docx/pptx/xlsx | 各 1 | 真实仓库文档内容经宿主 LibreOffice 转换（内容真实 + OOXML 真实封装） |
| png | 1 | 真实仓库文档文本渲染（宿主 Tesseract OCR 路由） |

`personal_sources_used=false`（构建器硬约束，源仅 /usr/share/doc 与仓库 docs/）。

## 实测结果

```
status: pass plan: 41 ok: 41 / 41
failures: 0
```

逐文件 import（magic bytes 检测 + CAS put + 单事务落对象/来源/文档/版本）与
parse（detect 路由 → 宿主解析 → IR 校验 → blocks/assets 落库 + parse_state 回填）
全部成功，`parse_state=APPROVED`（无 REVIEW_PENDING 触发项）。

## 幂等与去重实证

- **重解析幂等**：对 3 份已解析文档重跑 `rp-doc parse`，全部 `replayed=true`、
  `parse_state=APPROVED` 不变（"已提交版本幂等跳过"闸实测生效）。
- **内容对象 CAS 去重**：重导入 3 份同内容文件后 documents 41→44，但内容对象
  去重持有：44 documents / 40 distinct content sha256（重导入复用同一 CAS 对象，
  不复制正文；文档行按登记面语义各自存在，去重走关系层 dedup-scan，与
  R2 5.2「语义相似不得自动合并」一致）。

## 已知边界（如实记录，不阻断本闸）

1. **宿主解析 stdout 读帽 1 MiB**（tool-engine `NATIVE_OUTPUT_CAP_EXCEEDED`）：
   单份超大 PDF（实测 Ghostscript.pdf 4.7 MiB）全文抽取输出超帽即拒。语料构建器
   对 PDF 设 2 MiB 上限避开；帽值是否随真实大文档场景调整属 tool-engine 策略
   决策，不在本闸内改。
2. 修复记录（本批落码）：
   - 宿主解析 run 库 manifest 重复注册（首份注册后同版本重注册撞
     `VersionComparable`，批跑必现）→ `parser.rs` 幂等注册（已登记同版本跳过）；
   - txt 路由 `document_type` 直用 format 撞 IR schema enum（无 `txt` 值）→
     与 detect 面一致映射 `other`；
   - 空文本文档（LibreOffice 转换出空文本页）块装配伪造空 content 撞
     `minLength 1` → 空文本不产内容块，由 `overall=poor` + REVIEW_PENDING
     质量闸承载。

## 复现

```bash
cd /media/emo/code/pensting/research-platform
podman run -d --name rip-p17-pb001 -e POSTGRES_PASSWORD=pgselftest \
  -p 127.0.0.1:25432:5432 localhost/rip-pg17:latest@sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45
export RP_PG_DSN='postgres://postgres:pgselftest@127.0.0.1:25432/postgres'
python3 -I scripts/rip_golden_corpus_build.py --target /tmp/golden-corpus-pb001
python3 -I scripts/rip_golden_corpus_ingest.py --corpus /tmp/golden-corpus-pb001/raw \
  --execute --report /tmp/pb001-run/ingest-report.json
# 预期: status=pass, ok=41/41
```
