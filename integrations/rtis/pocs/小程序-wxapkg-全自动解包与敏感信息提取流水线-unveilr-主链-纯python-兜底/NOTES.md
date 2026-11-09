# 小程序-wxapkg-全自动解包与敏感信息提取流水线-unveilr-主链-纯python-兜底

> 状态: `script_ok` — 配套脚本 `wxapkg-unpack.py`
> 本文件说明**为什么这么写**，不是使用手册。用法见脚本头 docstring。

## 为什么需要这个脚本

不依赖 npm / unveilr：纯标准库实现，解决"机器上跑不了 node 工具"的场景。

格式（逆向所得，官方无文档）：
  0x00 1 byte 0xBE ；0x05-0x08 fileLength ；0x09-0x0C dataLength ；0x0D 0xED
  之后是 data 段，index 段在文件末尾（4 字节文件数 + n 条 {nameLen,name,offset,size}）

## 判据设计

自适应的原因：header 变体有好几种（部分包在 0x0E 处多 4 字节校验）。
硬编码一个偏移会在某些包上解析出乱码。所以先试多个候选 index 起点，
用「所有文件名合法 + offset+size 不越界 + 至少有一个 .js」来验证。
最后兜底扫 0x08~0x4000 全区间找能解析的位置。

## 常见误报（先排除这些，再下结论）

① 文件名必须做合法性校验：不含控制字符、不含 ../
② 文件数超过 2 万几乎肯定是解析错了
③ offset 是相对 data 段起点，不是文件绝对偏移
④ 加密的 wxapkg（首字节不是 0xBE）无法用本脚本处理

## 验证与收口

解包后必看四类文件：
  app-service.js      主逻辑，API 全集、加密实现、密钥都在这里
  app.json            pages 列表 → 完整页面路由（爬未授权页的入口）
  project.config.json 开发者配置，偶含 appid + 测试环境地址
  __APP__.wxapkg      分包，常有业务核心
包的位置：
  Android: /data/data/com.tencent.mm/MicroMsg/<hash>/appbrand/pkg/
  PC 微信:  WeChat Files/Applet/<appid>/<version>/

## 待办

- [ ] 对真实目标跑通一次，把结果写进 `INDEX.json` 的 `battle_verified`
- [ ] 构造阴性对照：一个必然不命中的输入，确认判据不会永远返回命中
