# SK-3 fixture 纪律（tests/fixtures/sk3/）

本目录全部是**拒绝矩阵反例与映射 fixture**，六个反例类一一对应批次验收
（安全知识源计划 §8 P6-11：无效 License、浮动分支、未锁依赖、可疑安装钩子、
越权 Shell、私网 URL、危险 MCP Schema 均有静态拒绝/告警测试）。

硬纪律（与 sk2 FIXTURE-NOTES 同源，逐字继承）：

1. **任何 fixture 都不允许被执行**。它们只作为文本进入静态审查
   （rip.security.extension.audit）或作为解析输入进入 catalog.extract；
   install-hook/install.sh、overreach-shell/SKILL.md 里的命令行永远只被
   读为文本。审查工具本身零执行能力（AST 级断言锁定 import 面）。
2. **不触网络**。fixture 内 URL（含 evil.example / 169.254.169.254）只做
   字面断言；document.fetch 的真实出网冒烟走独立冒烟域（example.com），
   与本目录无关。
3. **反例全部登记**：floating-branch（浮动分支 Manifest，validate rule 5 +
   sync pin 前置闸双闸拒绝）、no-license（license unknown → rule 5 语义）、
   install-hook（curl|sh 安装钩子）、overreach-shell（越权 Shell + Prompt
   Injection 迹象）、private-url（loopback/RFC1918/metadata 端点）、
   mcp-dangerous（无约束执行类 MCP Schema + 缺 Schema 反例，另含 1 个
   受约束对照 tool）。
4. **skill-pack 是正向映射 fixture**：两个静态分析/依赖检查类候选 +
   local_dir 来源，供 resource-engine promote_knowledge_candidates 与
   source.sync --map-candidates 使用（首批 Tool Pack 只收本地静态分析/
   代码审计/依赖检查类的选择政策落在本类候选上）。
5. fixture 只进测试环境；任何运行（install/sync）都在临时目录副本上进行
   （sk2 测试同款 copy_tree），install 落 checksums/plugin.lock 不污染本目录。
