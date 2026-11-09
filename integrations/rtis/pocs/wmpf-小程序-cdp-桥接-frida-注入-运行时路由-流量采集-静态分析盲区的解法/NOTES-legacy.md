# WMPF 小程序 CDP 桥接（工具链）
# 来源：https://github.com/xegeng/wx-miniapp-skills (wx-miniapp-wmpf)

## 架构
# 微信小程序 --Frida注入--> 62000 CDP桥 --MCP(:49999)--> AI助手
#                              |
#                        DevTools(手动调试)

## 启动顺序（铁律：桥先于小程序）
# 1. 环境变量（必须先设，否则本机代理劫持 localhost）
export no_proxy=127.0.0.1,localhost
export NO_PROXY=127.0.0.1,localhost
export PATH="/path/to/node20/bin:$PATH"      # frida_binding.node 需要 NODE_MODULE_VERSION 115

# 2. 起 CDP 桥（62000 监听）
#    WMPFDebugger / WMPFOffsetGen 取当前微信版本的偏移
# 3. 再打开小程序（反了拿不到注入）

## 连通性判据（三条都要满足）
curl -s http://127.0.0.1:9421/json/version        # 返回 JSON 含 Browser 字段
ss -lntp | grep 62000                             # 62000 在监听
grep "miniapp client connected" <桥日志>           # 关键行出现才算通
curl -s -o /dev/null -w "%{http_code}" "$MCP_URL" # 406/426 = 存活（协议头要求，非故障）

## MCP 调用序列
# connect_wmpf -> status -> select_appservice_context -> network_enable -> runtime_eval(注入)
# select_appservice_context 必须选同时含 hasWx + hasRequire + hasPages 的上下文
#   选错则后续 runtime_eval 全部无效

## 采集（静态盲区的解法）
# 路由全表：route_ops(inspect_routes) 或 runtime_eval 读 app.json
# 流量全量：get_all_requests(limit=3000)
#   注意：CDP Network 是进程级的，同机多小程序会混 -> 按 request.id 的 "pid." 前缀分离

## 崩溃处置
# 症状：get_recent_requests 超时 + runtime_eval 全 err + 窗口白屏
# 判断：WeChatAppEx 的 pid 变了
# 恢复：重起桥 -> UI 自动化重开小程序 -> 重跑 Phase0 注入 -> nav_sweep --resume
# 预防：每次 call 包 asyncio.wait_for(nav 6s / recent 20s)，每 10 页落盘

## UI 自动化（重开小程序）
# 必须 SetWindowPos(HWND_TOPMOST) -> SetForegroundWindow -> 只发键盘
# 截图验证必须用 GDI BitBlt 全屏截图（PrintWindow 看不见遮挡物，会给假象）
# DPI 125% 时截图坐标=物理像素，SendInput=虚拟坐标，差 1.25 倍 -> SetProcessDPIAware()