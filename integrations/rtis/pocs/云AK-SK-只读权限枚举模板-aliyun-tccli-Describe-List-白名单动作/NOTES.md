# 云AK-SK-只读权限枚举模板-aliyun-tccli-Describe-List-白名单动作

> 状态: `script_ok` — 配套脚本 `cloud-ak-enum.py`
> 本文件说明**为什么这么写**，不是使用手册。用法见脚本头 docstring。

## 为什么需要这个脚本

拿到 AK 后最省事的做法是跑一键枚举工具，但那些会发上百个请求、包含写动作，
在没有明确授权范围时是不可逆的。本脚本只回答一个最小问题：
**这组 AK 有效吗？它是哪个身份？它至少能读什么？**

## 判据设计

白名单动作，硬约束不是提示：
  只允许 Describe / List / Get / Query / Check / Head 开头
  24 个写动词黑名单（create/delete/put/run/invoke/attach/grant/...）命中即拒绝

## 常见误报（先排除这些，再下结论）

① AWS 的 GetCallerIdentity 不需要任何权限就能调 —— 它只证明 AK 有效，不证明有权
② 阿里云/腾讯云签名复杂（HMAC-SHA1 / TC3），自己实现容易错，错的签名给出假阴性
   → 所以走官方 CLI，没有 CLI 就明确说"无法判定"而不是"无效"
③ 主账号 AK 与子账号 AK 区别必须写：主账号几乎无限制
④ InvalidClientTokenId = AK 无效；AccessDenied = AK 有效但无该权限（两者完全不同）

## 验证与收口

报告口径：
  · 只写"AK 有效 + 能 Describe 到什么"，不写"可控制整个云账号"
  · 能 List 用户/策略 ≠ 能提权，除非验证了具体提权路径
  · **绝不**执行写动作来"证明危害"，一条 Describe 的返回就够
修复建议：立即禁用轮换；若来自前端 bundle 改用后端签发临时 STS（≤1h）；
若来自代码仓库要清理 git 历史（filter-repo），不只是删文件。

## 待办

- [ ] 对真实目标跑通一次，把结果写进 `INDEX.json` 的 `battle_verified`
- [ ] 构造阴性对照：一个必然不命中的输入，确认判据不会永远返回命中
