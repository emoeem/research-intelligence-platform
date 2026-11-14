# npm 依赖许可证逐包清单（web 面，P10-4）

> 快照性质：结论反映 2026-11-14 批次工作树（未 commit）的 web/package.json 与 web/node_modules 实装状态。
> 口径沿 telegram-bot `admin/DEPENDENCIES.md`（2026-08-22 基线）：逐包读取
> `node_modules/<pkg>/package.json` 的 `version` 与 `license` 字段（非声明值，为实际安装值）。
> 相关：docs/phase10/DEPENDENCIES-RUST.md（Rust 面）、docs/phase10/P10-4-TASK.md。

## 一、方法与范围

1. 脚本只读扫描 `web/node_modules` 全量 144 个实装包（含 scope 包与嵌套实装，顶层实装路径
   去重），逐包读实装 package.json；直接依赖 22 个（7 运行时 + 15 开发/测试）的声明版本与
   实装版本逐一核对一致（零偏差）。
2. 直方图（144 实装包）：MIT 109、ISC 12、Apache-2.0 10、MPL-2.0 5、BSD-3-Clause 4、
   BSD-2-Clause 2、0BSD 1、BlueOak-1.0.0 1。
3. 结论：全部为许可型（permissive）许可证，零强著作权（无 GPL/AGPL/LGPL）、零未声明许可。
   MPL-2.0 为弱著作权文件级 copyleft，仅 axe-core（4 包：@axe-core/playwright 与 axe-core）
   与 lightningcss（3 包，构建期二进制）使用，均为测试面与构建面，不进服务端。
4. 维护约定：npm 包只进 web/ 项目目录（红线）；后续新增依赖沿用本清单口径补审计。

## 二、直接依赖逐包清单（22 个，node_modules 实装值）

| 包 | node_modules 实装版本 | License（实装值） | 用途 |
| --- | --- | --- | --- |
| @tanstack/react-query | 5.107.1 | MIT | 数据请求与缓存（React Query） |
| @xyflow/react | 12.12.1 | MIT | Workflow Studio 画布（React Flow，P8-4） |
| echarts | 6.1.0 | Apache-2.0 | 图表（Overview 生命周期分布） |
| lucide-react | 1.70.0 | ISC | 图标库 |
| react | 19.3.1 | MIT | UI 框架 |
| react-dom | 19.3.1 | MIT | UI 框架渲染入口 |
| react-router | 8.5.1 | MIT | 路由（BrowserRouter） |
| @axe-core/playwright | 4.14.0 | MPL-2.0 | axe 无障碍扫描（视觉门禁） |
| @playwright/test | 1.65.0 | Apache-2.0 | 浏览器端到端与视觉门禁 |
| @tailwindcss/vite | 4.3.5 | MIT | Tailwind 构建插件 |
| @testing-library/dom | 10.4.2 | MIT | 组件测试工具 |
| @testing-library/react | 16.3.3 | MIT | React 组件测试 |
| @types/react | 19.3.0 | MIT | React 类型 |
| @types/react-dom | 19.3.0 | MIT | ReactDOM 类型 |
| @vitejs/plugin-react | 6.3.0 | MIT | Vite React 插件 |
| daisyui | 5.7.50 | MIT | daisyUI 组件样式（自定义双主题） |
| jsdom | 31.0.0 | MIT | vitest DOM 环境 |
| prettier | 3.9.10 | MIT | 代码格式化 |
| tailwindcss | 4.3.5 | MIT | 样式框架 |
| typescript | 7.0.2 | Apache-2.0 | 类型检查与构建 |
| vite | 8.3.7 | MIT | 构建工具 |
| vitest | 5.1.0 | MIT | 单元测试 |

## 三、非 MIT/ISC/Apache-2.0 的其余实装包（12 个，全部许可型）

| 包 | 实装版本 | License | 引入链 |
| --- | --- | --- | --- |
| @axe-core/playwright | 4.14.0 | MPL-2.0 | 直接 devDependency（视觉门禁 axe 扫描） |
| axe-core | 4.14.0 | MPL-2.0 | @axe-core/playwright 传递依赖 |
| lightningcss | 1.33.0 | MPL-2.0 | tailwindcss 4 构建链（vite 插件） |
| lightningcss-linux-x64-gnu | 1.33.0 | MPL-2.0 | lightningcss 平台二进制 |
| lightningcss-linux-x64-musl | 1.33.0 | MPL-2.0 | lightningcss 平台二进制 |
| d3-ease | 3.0.2 | BSD-3-Clause | zrender（echarts 动效）传递依赖 |
| zrender | 6.1.0 | BSD-3-Clause | echarts 渲染引擎 |
| source-map-js | 1.2.2 | BSD-3-Clause | 构建链传递依赖 |
| tough-cookie | 6.1.0 | BSD-3-Clause | jsdom 传递依赖 |
| entities | 9.0.0 | BSD-2-Clause | jsdom/html 解析链传递依赖 |
| webidl-conversions | 8.0.1 | BSD-2-Clause | jsdom 传递依赖 |
| lru-cache | 11.5.3 | BlueOak-1.0.0 | 构建链传递依赖 |
| tslib | 2.3.0 | 0BSD | TypeScript helper |

## 四、验收命令与输出

~~~bash
$ python3 全量扫描 web/node_modules/**/package.json（只读，见 P10-4-TASK.md 第二节）
# unique packages: 144；declared 7 + 15 与实装版本逐一一致（mismatches: []）
# license histogram: MIT 109 / ISC 12 / Apache-2.0 10 / MPL-2.0 5 / BSD-3-Clause 4
#   / BSD-2-Clause 2 / 0BSD 1 / BlueOak-1.0.0 1
# non-permissive / missing license: 0
~~~
