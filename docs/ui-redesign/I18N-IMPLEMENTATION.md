# RIP Web 多语言实现规范

> 状态：zh-CN / en-US / ja-JP 已接入全站静态 UI 资源；静态键完整性扫描通过，浏览器全路由复测与窄屏长文案检查继续进行。
> 日期：2026-11-24
> 适用：web/src/i18n、全局 AppShell、Dashboard、Research、Knowledge、Tools、Plugins、Agents、Workflows、Runs、Runtime、Settings 和 Web Security。

## 一、支持语言

| 语言 | i18next code | 默认用途 |
| --- | --- | --- |
| 简体中文 | zh-CN | 首次访问和回退语言 |
| English | en-US | 全站英文界面 |
| 日本語 | ja-JP | 全站日文界面 |

语言选择器放在全局 Topbar，Settings 里提供相同的本地显示偏好入口。切换语言不调用服务端设置接口，不修改用户资料或服务端配置。

## 二、实现约定

- 初始化和语言列表：web/src/i18n/index.ts。
- UI 文案资源：web/src/i18n/ui-resources.ts，使用 ui namespace。
- 静态页面文案：通过 translateUi("源文案") 查找当前 locale。
- React 组件需要订阅语言变化时，使用 useUiText()。
- 语言选择写入浏览器 localStorage 的 rip_language。不支持或无效的 locale 回退至 zh-CN。
- i18next fallbackLng 为 zh-CN。语言变化时同步更新 document.documentElement.lang 和页面标题。
- 用户清除浏览器本地数据后，首次语言回到 zh-CN。

## 三、文案范围

需要翻译：

1. 工作区名称、子导航、页面标题、按钮、表格列名、筛选器、表单标签和占位提示。
2. 空态、载入态、通用错误标题、确认对话框、只读提示和操作状态。
3. Tooltip、ARIA 名称、页面可见的辅助说明。
4. Agent、Workflow、Runtime、WSW 等页面的静态动态模板片段，例如相对时间单位和计数文案。

保留原样：

1. API 返回的业务标题、知识内容、文档正文和用户输入。它们属于数据，不是 UI 字符串。
2. HTTP 状态码、错误码、SQL 字段、路由、URL、ID、CVE、Trace ID、命令、JSON key、DSL token、CLI 参数和 Rust 类型名。
3. API 返回的原始诊断详情。可以翻译通用错误标题和重试动作，但不应把后端原始错误内容改写成可能失真的本地化文本。
4. 用户明确选择的代码、请求报文、Payload 和证据原文。

简体中文页面的中文源文案可以直接作为默认值，不为相同中文内容重复创建资源项。以英文为源的 UI 短标签必须提供中文映射，避免中文界面出现孤立英文按钮。

## 四、资源覆盖检查

源码中的字面量 translateUi("...") 作为静态资源覆盖扫描入口。每次增加文案时必须同时检查 zh-CN、en-US、ja-JP：

- zh-CN 对中文源文案允许默认回退；不允许英文源 UI 短标签漏译。
- en-US 和 ja-JP 的每个 translateUi 静态 key 都必须存在明确映射。
- 动态 translateUi(value) 必须追溯 value 的来源，不能仅依赖静态字面量扫描。
- 不要把状态码、ID 或用户数据加入 UI 翻译表。

2026-11-24 最新检查结果：源码中的 translateUi/ui 静态调用有 619 个唯一字面量 key，资源表声明 770 个源 key，静态键扫描缺项为 0。中文源文案使用中文资源或原文回退；英文源 UI key 需有中文映射。Research Tasks 表头与 Research Coverage 七维标签已从共享模块改为渲染时读取当前 locale。静态键扫描不覆盖 translateUi(value) 等动态调用，仍必须以真实浏览器路由与窄屏矩阵作为最终验收。

## 五、验收

每次多语言改动至少覆盖：

1. zh-CN → en-US → ja-JP → zh-CN 连续切换。
2. 切换后浏览器刷新，语言选择仍保留。
3. document.documentElement.lang 与选择一致，页面标题正确。
4. 关键工作区路由无翻译键名泄漏，无因长文案造成的布局错位。
5. 390px 手机视口、1440px 桌面和 1768px 参考 Dashboard 视口无横向溢出。
6. 键盘焦点、ARIA 名称和 WCAG AA 对比度检查。
7. 生产构建和 Vitest 全部通过。

视觉检查不能只看 Dashboard。Research、Knowledge、Tools、Plugins、Agents、Workflows、Runs、Runtime、Settings 和 Web Security 都要抽查长标题、筛选表单、空态、错误态、详情面板。

## 六、已知限制

- 浏览器本地语言偏好只对当前浏览器配置生效，不跨设备同步。
- 数据内容、用户输入和 API 原始诊断信息不会强行翻译。
- 后端认证错误的通用标题、说明和重试按钮可翻译；底层错误码与诊断上下文保留原样。
- 某些历史业务页面的数据枚举仍按服务端原值显示。若要本地化枚举，必须先建立稳定的 enum-to-label 映射，不能直接翻译数据库值。
