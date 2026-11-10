import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { applyTheme, getStoredTheme } from "./theme";
import "./styles/token.css";
import "./styles/shell.css";

// 首帧由 index.html 内联脚本先行设置 data-theme；启动时按同一映射重设一次，
// 并把归一化结果写回 storage（见 theme.ts 头注释）。
applyTheme(getStoredTheme());

const container = document.getElementById("root");
if (!container) throw new Error("未找到 #root 挂载点");

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
