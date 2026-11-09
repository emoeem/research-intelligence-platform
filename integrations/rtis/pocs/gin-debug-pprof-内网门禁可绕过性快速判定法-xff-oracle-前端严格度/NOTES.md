# gin-debug-pprof-内网门禁可绕过性快速判定法-xff-oracle-前端严格度

> 状态: `script_ok` — 配套脚本 `poc.sh`
> 本文件说明**为什么这么写**，不是使用手册。用法见脚本头 docstring。

## 为什么需要这个脚本

Go 服务用 gin.Default() 起，生产上 /debug/pprof/ 还在。很多部署在入口网关加了
"只允许内网 IP 访问 /debug"，而应用层只看 X-Forwarded-For —— 伪造头即可绕过。

判据不能是"能访问"，必须是**单变量对照**：同一路径、只改一个头、结果翻转。

## 判据设计

三态：
  OPEN      不带任何头就是 200 + pprof 特征 → 压根没门禁
  APP_TRUST 裸请求 403，加 XFF: 127.0.0.1 后 200 + pprof 特征 → 门禁在应用层，可绕
  DENY_ALL  带任何头都 403/404，且响应与随机基线一致 → 门禁在网关，绕不过

## 常见误报（先排除这些，再下结论）

① 只打一次拿到 403，无法区分"网关拦"和"路径不存在"
② pprof 首页特征不是 200，是 "Types of profiles available" / "goroutine profile"
③ 部分框架把 X-Real-IP 也当真，六种头要全试
④ 网关可能只在「连接建立时」判一次，之后不再校验

## 验证与收口

利用价值排序：
  /debug/pprof/goroutine?debug=1 → 全部 goroutine 栈，可见调用链、内部包名、DB 驱动
  /debug/pprof/cmdline           → 启动参数，常含 -config /etc/xxx.yaml
  /debug/pprof/heap              → 堆内存，可捞明文 token（体积大，先落盘再 strings）
  /debug/vars                    → 运行时变量，偶有配置项
heap 转储含真实内存数据，禁止外传，只提取能证明危害的最小片段。

## 待办

- [ ] 对真实目标跑通一次，把结果写进 `INDEX.json` 的 `battle_verified`
- [ ] 构造阴性对照：一个必然不命中的输入，确认判据不会永远返回命中
