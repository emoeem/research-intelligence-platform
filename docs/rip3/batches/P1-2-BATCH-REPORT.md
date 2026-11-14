# RIP3 批次报告：P1-2 收口（REST 通道 + 文档中心 web 面 + 浏览器实测 + 备份双轨演练，§17.6 格式）

## A 完成内容

1. **REST 通道**（rp_server.py）：GET /api/documents（workspace 过滤 + all 软删开关）、
   GET /api/documents/<id>（版本历史动态路径）、POST /api/documents/import（base64 →
   临时文件 0600 用完即删 → rp-doc 子进程 → magic bytes + CAS + 单事务落库）。
   体帽 RP_DOC_IMPORT_MAX_BYTES（缺省 64MiB，_read_json_body 参数化放宽，base64 膨胀
   4/3 余量）；鉴权/限频/审计三件自动继承（require_auth + check_rate_limit +
   audit_api_access）；python 侧零 PG 驱动依赖（进程边界即数据边界）。
2. **web 面**：DocumentsPage（/documents，挂 Knowledge 工作区第二 tab 4.2）：上传表单
   （文件/标题/工作区/可见性/来源 URL，分块 base64 编码防大文件爆栈）+ 文档列表 +
   版本历史面板（SUPERSEDED 在列）；lib/api.ts 三取数一写入函数；App 路由 +
   workspaces.ts tab（十一工作区不变，路由总数 11 到 12）。
3. **浏览器实测**（web/scripts/rip3_p1_browser_e2e.mjs，chromium 用户缓存口径，
   1440x900）：7/7 PASS：列表就绪（真 PG 数据 rows=2）；真实文件上传（setInputFiles
   走真实 rp-doc 子进程与 PG 落库，对象新建 doc-18e7f60a）；新文档 refetch 在列
   （2 到 3）；版本历史面板（CURRENT 词在列）；无文件提交前端拦截（请选择文件）；
   零 pageerror；零 console.error。截图三张 + summary.json 入库
   docs/rip3/batches/browser-e2e/。
4. **备份双轨演练**（P1 验收项）：PG 轨 pg_dump custom 格式，全新容器实例
   pg_restore（TOC 过滤镜像基础设施面 pg_cron/tiger/paradedb 后全量应用），四表逐行
   对账 documents 19 到 19 / content_objects 10 到 10 / document_revisions 23 到 23 /
   source_records 19 到 19，三扩展在恢复库在位；SQLite 轨 rp-admin backup/restore
   （selftest_phase1_pg.sh 16/16 已证）。恢复实例用后即删。

## B 架构变化

- 进程边界口径定型：python（rp_server）零 PG 驱动，文档读写一律经 rp-doc 子进程
  JSON stdout（沿 rp-workflow/rp-plugin 既有模式，写面强制鉴权）。
- web 面权限前置行为面贯通：list 请求携带 workspace，visibility 谓词在 PG SQL 内。

## C 文件变化

新增：web/src/routes/DocumentsPage.tsx、web/tests/documents-smoke.test.tsx、
web/scripts/rip3_p1_browser_e2e.mjs、docs/rip3/batches/browser-e2e/（3 截图 + summary）。
修改：apps/server/rp_server.py（DOC_BIN 配置 + _rp_doc_json + 三 handler + 动态路径 +
do_POST 白名单 + _read_json_body 参数化）、web/src/lib/api.ts（四函数 + 五类型）、
web/src/App.tsx（路由）、web/src/workspaces.ts（knowledge tab 4.2）。

## D 数据库变化

零迁移新增（PG-0001/0002 既有链；SQLite 020 仍空闲）。备份双轨演练的数据面见 A4。

## E 测试结果（全部实际执行）

| 门 | 命令 | 结果 |
| --- | --- | --- |
| web 单测 | cd web && npm run test | Test Files 19 passed，Tests 124 passed（基线 120 + documents-smoke 4） |
| web 构建/格式 | npm run build + prettier --check | exit 0；All matched files use Prettier code style! |
| REST live smoke | curl 全链（import/list/detail/401 无令牌拒绝） | 全过（跨通道 CAS 复用：CLI 与 REST 各导同内容，第二次 content_object_created=false、hash 一致） |
| 浏览器实测 | node web/scripts/rip3_p1_browser_e2e.mjs | 7/7 PASS（截图三张留证） |
| 备份双轨 | pg_dump/pg_restore roundtrip + rp-admin（既有 selftest） | 四表逐行一致 + rp-admin 16/16 |
| 静态检查 | python3 ast 语法核 + clippy --workspace -D warnings | exit 0（P1-1 批已录，本批 python 面无 Rust 改动） |

## E2 CI 结论

- P1-1 批次（c4c76a9）CI #63 **failure**：vendor 嵌套 .gitignore 挡 git add
  （pgvector-0.4.2/.gitignore 的 Cargo.lock 规则使 checksum 清单文件未入库，离线
  构建 checksum 校验失败）。
- 修复 commit 41bf3359（清理 28 个新增 vendor 目录嵌套 .gitignore + checksum 同步 +
  三方一致性核对 checksum/磁盘/git 零缺失）CI **success 2m59s**（本批 web 面随该提交
  一并入库验证）。
- docs 回填 15ff4063 已推送（SSH 断连重试即通惯例）。

## F 性能结果

REST import 端到端秒级（64KiB 样本）；规模数字属 Phase 8 基准，不在本批。

## G 遗留问题

1. 浏览器实测用 setInputFiles（playwright 能力）覆盖真实文件上传；IAB 通道的
   file chooser 不支持属工具面限制，真实用户浏览器的 file chooser 流由表单实现
   （input[type=file]）承载，行为一致。
2. CI 无 PG service：intel-db 集成测试 CI 侧跳过口径维持（本地实跑为准），PG service
   job 评估随 Phase 4 批。
3. /documents 页未纳入视觉门矩阵（44 张基线为 11 页 × 双主题 × 双视口）；视觉覆盖
   随 Phase 5/6 页面族批次统一扩展。

## H 下一阶段依赖与顺序

Phase 1 阶段收口（本文件 + RELEASE + 全量门禁复跑 + HANDOVER/SESSION-HANDOFF 登记）
→ Phase 2 开批（rip-parser 容器族：Docling/PyMuPDF/Tesseract 模型烘焙离线 +
Parser Registry + 解析管线 workflow 定义 + Document IR 落库）。
