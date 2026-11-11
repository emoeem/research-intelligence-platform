# RTIS 红队知识插件落位备注（P6-5）

| 项     | 值                                                                                       |
| ------ | ---------------------------------------------------------------------------------------- |
| 状态   | 生效中（P6-5 批次内落稿，随批次演进）                                                     |
| 日期   | 2026-11-11                                                                               |
| 基准   | ROADMAP §7.3 P6-5、§7.5.2/§7.5.6、PLUGIN-CONTRACT §2 原则 2 / §3.2 / §5.4、待验证项 V-12/V-16 |
| 相关   | docs/phase0/PHASE-ROADMAP.md、docs/phase0/PLUGIN-CONTRACT-DRAFT.md、docs/phase6/PHASE6-PLAN.md |

## 一、license 裁决（ROADMAP §7.5.2 / V-16）

**裁决结果：维持 UNLICENSED，依据如下。**

1. 许可证选择属维护者治理决策：仓库已公开（SESSION-HANDOFF §十口径），但顶层 LICENSE 文件选型（MIT / Apache-2.0 / 其他）未拍板，实施侧不得代为选择。
2. UNLICENSED 是 SPDX 保留标识，语义为「未选择许可 / 非自由许可」，与本仓现状（公开但未定许可）相符；plugin-registry rule 4（P6-1）只拒 `license=unknown`，UNLICENSED 通过校验，不构成阻塞。
3. 决策点移交维护者：未来补顶层 LICENSE 文件时，同步更新两份示例 Manifest 的 license 字段与本节裁决记录（变更走批次台账）。

## 二、knowledge.db 条数口径收敛（V-12）

实测（2026-11-11，`sqlite3 file:integrations/rtis/knowledge.db?mode=ro` 只读 COUNT）：

| 来源                 | 声称    | 实查    | 结论                                       |
| -------------------- | ------- | ------- | ------------------------------------------ |
| integrations/rtis/README.md | 14717 | 14717 | 与实查一致，为当前权威口径                  |
| rtis.py 头注（第 6 行）     | 14664 | 14717 | 陈旧快照（2026-11-02 时点），头注未更新     |

- **权威口径：poc 表 14717 行**（另：k_meta 28 行、k_event 数未入账要求，头注的 15/225 同为陈旧快照）。
- rtis.py 头注不改：rtis.py 零 diff 是 P6-5 红线（§7.5.6），口径收敛以 README 与本批实测记录为准，头注修正归 RTIS kit 自己的维护批次。
- RTIS-RESOLVER、bootstrap.sh、backups/ 回滚机制全程未触碰（红线核查：knowledge.db 与 rtis.py sha256 摄取前后逐字节一致，见 §四）。

## 三、摄取映射与规模（本批实测）

| content_kind | 来源                       | 映射规则                                  | 实测条数 |
| ------------ | -------------------------- | ----------------------------------------- | -------- |
| poc          | knowledge.db `poc` 表      | 逐行；body = content，缺省回退 usage/description；meta_json 收 kind/category/cve/component/versions/severity/language/tags/verified | 14717    |
| skill_md     | `skills/*.md`              | 文件全文；title = 文件名 stem             | 125      |
| method_case  | `pocs/**` 全部文件         | 递归正文；title = 包内相对路径            | 313      |

合计 15155 行、6286781 字节正文；幂等重跑 skipped=15155（UNIQUE(plugin_id, plugin_version, content_kind, content_sha256) 幂等键，M6 ETL Replay 口径）。

## 四、RTIS 生产零变化红线核查（P6-5 批次内实跑）

| 核查项                            | 方法                                   | 结果                                     |
| --------------------------------- | -------------------------------------- | ---------------------------------------- |
| knowledge.db 主文件零改动         | 摄取前后 sha256 对比                   | f8580feb...（前后一致，逐字节不变）      |
| rtis.py 零 diff                   | `git diff integrations/rtis/rtis.py`   | 空                                       |
| git status integrations/rtis 干净 | `git status --porcelain integrations/rtis/` | 0 行                                |
| RTIS_RESOLVER_MODE 三态机制不动   | grep rtis.py:658/664 实查              | 机制原文在位，未触碰                     |
| 只读通道证明                      | 摄取源 = installed 副本内 knowledge.db（mode=ro URI），非生产路径 | 生产文件哈希全程不变 |

WAL 边车说明：SQLite 只读连接打开 WAL 模式库会生成 0 字节 `-wal`/`-shm` 边车（本批 05:10 出现），批次结束已移除复位（0 字节无内容，安全）；knowledge.db 主文件哈希全程不变。

## 五、capability entry 对齐修订（P6-5）

RTIS 真实树无 `capabilities/` 目录（P6-1 示例为理想化假设）：两 capability 的 entry 从理想化路径改为包内真实文件 `README.md`（rule 7 语义 = 入口描述文件存在于包内），yaml/json 双份同步，示例头注已声明。pinned commit 升为全量 sha（git rev-parse HEAD，批次时点 6217ca807c68974ad1541d8017ed7d9e76b58e3c）：版本固定语义下，后续仓库提交需同步更新 pinned 值。

## 六、遗留与移交

- FTS/向量索引对接（摄取行进检索面）属 Phase 4 search-engine 轴向，本批只落数据面行（PLUGIN-CONTRACT §9 附加规则 4：索引构建为 INSTALLED 后台步骤）。
- `rtis.py` 头注条数修正、顶层 LICENSE 文件：移交维护者决策（见 §一、§二）。
