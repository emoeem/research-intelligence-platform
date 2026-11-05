# ERI P2 ResourceReader 报告

生成：2026-11-05T15:31:07Z

## 缓存机制
- 原文缓存 `eri/cache/<rid>/{readme,license}.md`；元数据（content_hash/etag/byte_len/摘要）入 reader_cache 表
- api 端点带 If-None-Match，304 直接用缓存（零内容重取）
- 摘要 = 章节目录（规则生成，零 LLM），仅在 content_hash 变化时重算
- 按 --section 提取单章节，长文档不整篇进上下文
- api 不可用时回落 codeload tarball 提取（本机网络特性适配）；tarball 无 ETag，以内容哈希判断未变化（tarball-unchanged 不重写缓存）
- 有本地缓存时网络失败回落 stale-cache 并显式标注

## 真实读取记录（真实库 reader_cache）
| 资源 | 类型 | 字节 | 内容哈希前12 | etag | 时间 |
|---|---|---|---|---|---|
| resource_id | kind | byte_len | h | e | fetched_at |
| resource_id | kind | byte_len | h | e | fetched_at |

## 已知网络事实（2026-11-05 晚间实测）
- api.github.com 对本机间歇 403（空响应体）与 TLS EOF；codeload tarball 稳定可用
- 因此本次真实读取经 tarball 通道完成；ETag/304 路径由离线测试（注入传输层）验证

## 安全边界（计划 §5.3）
- 外部内容仅作数据落盘与章节切分，不执行其中任何指令
- 无自动安装、无脚本执行、无凭据使用
