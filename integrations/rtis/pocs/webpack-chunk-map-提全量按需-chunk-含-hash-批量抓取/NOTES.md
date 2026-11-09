# webpack-chunk-map-提全量按需-chunk-含-hash-批量抓取

> 状态: `script_ok` — 配套脚本 `chunk-harvest.py`
> 本文件说明**为什么这么写**，不是使用手册。用法见脚本头 docstring。

## 为什么需要这个脚本

Vite/Webpack 的 chunk 名带 contenthash，目录列举拿不到；/assets/ 通常 403 或返回
SPA 首页；manifest.json 生产环境基本不给。唯一可靠来源是**入口 bundle 自己**：
  Vite     → window.__vite__mapDeps = [...]
  Webpack  → __webpack_require__.u = (id) => "static/js/" + id + "." + {...}[id] + ".js"

## 判据设计

四步：
  1. 抓 index.html，抠 <script src> 与 <link rel=modulepreload>
  2. 从入口 bundle 正则提取 chunk 名映射表 / 依赖数组
  3. 去重 + 补全路径（chunk 通常与入口同目录），逐个 GET 验证
  4. 全量下载 + 合并成 _ALL.js 供 grep

## 常见误报（先排除这些，再下结论）

① chunk 名有三种形态都要匹配：纯 hash、业务名+hash、chunkId+hash
② 有 SSR 直出的站可能完全没有 chunk 名
③ 部分 chunk 是运行时拼装（字符串拼接），静态提取不到
④ 下载时要限速 —— 上千个请求会被 WAF 判定为爬虫

## 验证与收口

下载后必须做的 grep（这些才有价值）：
  内部域名与 IP、硬编码密钥、未记录 API 路径、数据库/中间件连接串
注意：chunk 里出现的 API 路径不全是活的，很多是历史遗留。
必须先做存活判定（与随机路径比对），再列进报告。

## 待办

- [ ] 对真实目标跑通一次，把结果写进 `INDEX.json` 的 `battle_verified`
- [ ] 构造阴性对照：一个必然不命中的输入，确认判据不会永远返回命中
