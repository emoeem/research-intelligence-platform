# spring cloud gateway discovery locator 路由探测 api wg lt svc gt 直达后端服务

> 状态: `stub` — 只有方法论，无自动脚本。
> 本文件由目录名中的技术结论还原，未对任何真实目标验证过。

## 主题

目录名本身记录了这一步的核心结论：**spring cloud gateway discovery locator 路由探测 api wg lt svc gt 直达后端服务**

拆出的技术要点：

- `spring`
- `cloud`
- `gateway`
- `discovery`
- `locator`
- `路由探测`
- `api`
- `wg`
- `lt`
- `svc`
- `gt`
- `直达后端服务`

## 方法族: `framework`

### 执行框架

1. 确认指纹: 错误页 / 特定路径 / 响应头 / cookie 名，四者交叉确认
2. 定版本: 从错误页堆栈、静态资源 hash、特定路径的存在性反推
3. 查该版本公开问题: 只打与版本匹配的，不套用其它版本的 payload
4. 无害验证: 先用「会报错但不执行」的输入确认可达，再决定是否发真 payload
5. 记录未授权面: actuator/swagger/管理端的暴露情况单独记

### 常见误报（先排除这些，再下结论）

- 指纹误判: 有 index.php 不等于 ThinkPHP，有 /actuator 不等于 Spring Boot
- 版本误判: 打了补丁的版本仍会返回类似错误页，不能只看页面
- WAF 前置: 你测到的是 WAF 的响应，不是框架的响应
- 路径变体: 1.x 与 2.x 的默认路径不同，测不到不等于不存在

## 落地清单

- [ ] 补齐可执行脚本 `poc.sh`（参数化 HOST / ENDPOINT）
- [ ] 明确判定阈值（什么结果算命中，什么算未命中）
- [ ] 至少对一个真实目标跑通一次，把结果写进 `INDEX.json` 的 `battle_verified`
- [ ] 补「阴性对照」用例：构造一个必然不命中的输入，确认判据不会永远返回命中

## 备注

原目录名保留全部信息，未做删改。若后续补脚本，
脚本内的注释应说明「为什么用这个判据」，而不是只写用法。
