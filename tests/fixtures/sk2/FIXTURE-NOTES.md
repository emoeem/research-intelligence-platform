# SK-2 三源固定 Commit 最小快照 fixture（P6-10）

> 状态：生效中（SK-2 批次内落位，随批次演进）
> 日期：2026-11-12
> 基准：docs/RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md §3/§5/§8 P6-10
> 用途：离线测试（static_chain 七条校验 + runtime 摄取测试）。真实 clone 校验走
> git 来源通道（rp-plugin install 不带 --package），见 docs/phase6/SK2-SOURCES-NOTES.md。

## 目录与来源

| fixture 目录 | 上游来源 | 固定 Commit（全 40 位） | 内容 |
| --- | --- | --- | --- |
| awesome-hacking/ | github.com/Hack-with-Github/Awesome-Hacking @ master | 440486fd08b0a0d9ad9a041a0bb234dcdf65dee2 | README.md + LICENSE（CC0-1.0），字节拷贝 |
| secskills/ | github.com/DaoYiSec/SecSkills @ main | 1867eb6b71889af8632107884e9d0235510f9f78 | README.md + LICENSE（Apache-2.0），字节拷贝 |
| tongui-course-notes/ | github.com/tongui66/Cyber-Security-Course-Notes @ master | 45a12a759e82adec13c3d171a34d1cbeea1a15b3 | README.md（字节拷贝）+ catalog/articles.md（RIP 侧派生目录） |

## 纪律

1. README/LICENSE 为固定 Commit 树的字节拷贝（sha256 与 clone 实测一致，2026-11-12），
   不得手改；上游演进时按固定 Commit 重拷并同步 Manifest pinned 值。
2. tongui 课程正文（74 篇 .md 全文）故意不入 fixture：仓库无 LICENSE
   （UNLICENSED），批量持久化正文违反计划 §7 第 2 条；fixture 只保留 README
   （章节/标题元数据）与派生目录（章节/标题/URL/路径元数据）。
3. tongui catalog/articles.md 由 scripts/sk2_tongui_catalog.py 生成（输入：固定
   Commit 原稿树 + 精选站点首页快照 HTML），生成命令：

   ```bash
   python3 scripts/sk2_tongui_catalog.py \
     --repo <Cyber-Security-Course-Notes 工作树 @ 45a12a75> \
     --site-index <course-notes 首页快照 HTML> \
     --out tests/fixtures/sk2/tongui-course-notes/catalog/articles.md
   ```

   实测输出：74 篇 + 1 来源主页 = 75 条，精选合并 31/31（2026-11-12）。
4. fixture 目录非 git 工作树：以 `--package` 本地 staging 安装时 pinned 取自
   Manifest source.pinned（plugin-registry build_lock 语义）；固定 Commit 的
   存在性与 checkout 校验由 git 来源通道实跑覆盖。
