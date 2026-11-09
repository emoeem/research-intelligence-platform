# 多租户-saas-的-ssrf-白名单字典-schema-存在性-oracle-yz_sys_dict-error-1146-按租户取-token

> 状态: `script_ok` — 配套脚本 `poc.sh`
> 本文件说明**为什么这么写**，不是使用手册。用法见脚本头 docstring。

## 为什么需要这个脚本

若依 / pig4cloud 一类多租户框架，隔离靠"一个租户一个 MySQL schema"，连接时把 schema
名拼进 SQL。报错信息会泄露 schema 存在性：
  schema 不存在 → MySQL error 1146 (Table 'xxx.sys_dict' doesn't exist)
  schema 存在   → 正常返回数据或空列表
这个差异就是 oracle，不需要任何权限。

## 判据设计

必须先证明 oracle 有判别力：
  负样本（必然不存在的 schema 名）返回 200 + 数据 → 参数根本没参与 schema 拼接 → oracle 无效，退出
  负样本返回 1146/1049 → 有判别力，继续枚举

## 常见误报（先排除这些，再下结论）

① 1146 是"表不存在"（schema 推断成功），1049 是"库不存在"，两者含义不同
② 有些实现把异常吞掉统一返回 500，那就没有 oracle
③ tenant 标识可能不在 query 上：Header（X-Tenant-Id）、路径前缀、子域、JWT 里的 tenant 字段
④ 只返回空列表 ≠ 越权，只说明 schema 存在

## 验证与收口

命中后的正确验证顺序（风险从低到高）：
  A. 用命中的 tenant 值重新请求，看是否返回**别的租户的数据** —— 这才叫越权
  B. 拿 A 租户的 token，配 B 租户的 tenant 参数（token/tenant 交叉）
  C. 找租户注册接口，注册新租户，看初始数据是否含模板数据（推断 schema 命名规则）
禁止用 oracle 批量跑 10 万级 schema 名 —— 这是扫描行为，会打爆日志。

## 待办

- [ ] 对真实目标跑通一次，把结果写进 `INDEX.json` 的 `battle_verified`
- [ ] 构造阴性对照：一个必然不命中的输入，确认判据不会永远返回命中
