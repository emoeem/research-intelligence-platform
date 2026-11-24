import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import { setAppLanguage, type AppLanguage } from "../src/i18n";

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async () =>
        new Response(
          JSON.stringify({
            items: [],
            total: 0,
            available: true,
            workspace_id: "default",
          }),
          { status: 200, headers: { "Content-Type": "application/json" } },
        ),
    ),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

const locales: { locale: AppLanguage; project: string; assets: string }[] = [
  { locale: "zh-CN", project: "项目目录", assets: "资产清单" },
  { locale: "en-US", project: "Project registry", assets: "Asset registry" },
  { locale: "ja-JP", project: "プロジェクト一覧", assets: "資産一覧" },
];

describe("Projects and Research Assets localization", () => {
  it.each(locales)(
    "renders translated Project and Asset workspaces in $locale",
    async ({ locale, project, assets }) => {
      await setAppLanguage(locale);
      window.history.pushState({}, "", "/projects");
      const { unmount } = render(<App />);
      expect(await screen.findByText(project)).toBeTruthy();
      unmount();

      window.history.pushState({}, "", "/research/assets");
      render(<App />);
      expect(await screen.findByText(assets)).toBeTruthy();
    },
  );
});
