import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { applyTheme, getStoredTheme } from "./theme";
import { setAuthTokenProvider } from "./lib/api";
import "./styles/token.css";
import "./styles/shell.css";

// 首帧由 index.html 内联脚本先行设置 data-theme；启动时按同一映射重设一次，
// 并把归一化结果写回 storage（见 theme.ts 头注释）。
applyTheme(getStoredTheme());

// P8-4：Bearer 令牌来源接线（P5-4 会话面落地前的最小桥）。令牌存
// localStorage("rip_auth_token")，无键即无 Authorization 头（鉴权关闭的
// 开发实例读面不受影响）；写面（workflow run/保存、插件写）经 api.ts 的
// authHeaders 注入，服务端鉴权开启时校验（写端点强制 Bearer 既有纪律）。
setAuthTokenProvider(() => {
  try {
    return localStorage.getItem("rip_auth_token");
  } catch {
    return null;
  }
});

const container = document.getElementById("root");
if (!container) throw new Error("未找到 #root 挂载点");

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
