# P10-2 分级确认真实浏览器实测记录

- 日期：2026-11-14
- 载体：web/tests/plugins-trust-confirm.spec.ts（离线静态服务器 visual-server.mjs + route-mock，P6-8 插件面同模式）
- 配置：web/playwright.plugins-trust.config.ts（PW_CHROMIUM 三级探测：显式环境变量 > 用户缓存 ~/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome > 系统 /usr/sbin/chromium；零系统级安装）
- 本机实测探测：系统 PATH 无 chromium / chromium-browser / google-chrome-stable（command -v 空输出）；用户缓存 chromium-1234 在位，PW_CHROMIUM 注入执行
- 运行命令：`PW_CHROMIUM=$HOME/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome npx playwright test -c playwright.plugins-trust.config.ts`，4 passed (5.9s)
- 覆盖四档确认强度（plugin-registry::trust_policy 单一策略表）：
  - official：常规确认，一次提交，无额外确认字段（D10 允许自动更新提示）
  - community：确认对话框一次同意（confirm=true）
  - local：来源核对勾选门（勾选前确认键禁用，confirm=true）
  - remote：输入插件 id 二次确认（不一致阻断 + confirm-id-mismatch 提示；一致后 confirm=true + confirm_id 与插件 id 一致，D10 需手动检查更新提示）
- 断言：零 console.error / 零 pageerror；安装写请求体 confirm/confirm_id 随档位与策略表一致
- 截图（本目录）：trust-confirm-official.png / trust-confirm-community.png / trust-confirm-local.png / trust-confirm-remote.png / trust-confirm-remote-mismatch.png
