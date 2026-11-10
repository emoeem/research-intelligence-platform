/// <reference types="vitest/config" />
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv } from "vite";

export default defineConfig(({ mode }) => {
  // /api 代理目标默认 apps/server rp_server.py（127.0.0.1:7800）；本机 7800 被
  // 其他实例占用时可用 RP_API_TARGET 覆盖（如 dev 模式无鉴权实例的 7801）。
  const env = loadEnv(mode, ".", "");
  const apiTarget = env.RP_API_TARGET || "http://127.0.0.1:7800";
  return {
    plugins: [react(), tailwindcss()],
    server: {
      proxy: {
        "/api": {
          target: apiTarget,
          changeOrigin: false,
        },
      },
    },
    build: {
      outDir: "dist",
      emptyOutDir: true,
      // Vite 8 走 Rolldown，vendor 分组用 codeSplitting.groups（模式沿用
      // 参照实现 admin vite 配置的实测写法；RIP 是单入口应用，
      // react-router 可以安全并入 react 组，不存在其双入口 170KB 多下载的教训）。
      // echarts 等重型库不进 vendor 组，仍按动态 import 按需加载（计划 §10.6）。
      rolldownOptions: {
        output: {
          codeSplitting: {
            groups: [
              {
                name: "vendor-react",
                test: /node_modules[\\/](react|react-dom|scheduler|react-router)[\\/]/,
                priority: 30,
              },
              {
                name: "vendor-query",
                test: /node_modules[\\/]@tanstack[\\/]/,
                priority: 20,
              },
              {
                name: "vendor-icons",
                test: /node_modules[\\/]lucide-react[\\/]/,
                priority: 20,
              },
              {
                name: "vendor-echarts",
                test: /node_modules[\\/](echarts|zrender)[\\/]/,
                priority: 20,
              },
              {
                name: "vendor-zod",
                test: /node_modules[\\/]zod[\\/]/,
                priority: 20,
              },
            ],
          },
        },
      },
    },
    test: {
      environment: "jsdom",
      globals: true,
    },
  };
});
