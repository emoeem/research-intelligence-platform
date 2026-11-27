import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

/** P5-6 Knowledge 页冒烟：列表渲染、q= 检索（防抖后带关键词请求）、无命中
 *  真实空态（服务端 message）、Loading/Error 三态、详情侧栏（knowledge_id
 *  全字段拉取 + 来源/元数据/正文摘要呈现）。 */

const LIST_STUB = {
  available: true,
  source: "main",
  total: 2,
  returned: 2,
  items: [
    {
      knowledge_id: "k-redirect",
      lifecycle: "VERIFIED",
      title: "站群/落地页开放重定向专项测试法",
      component: "landing",
      severity: "high",
      verified: 1,
      hit_count: 7,
      updated_at: "2026-11-09T10:00:00Z",
    },
    {
      knowledge_id: "k-hex2bin",
      lifecycle: "CANDIDATE",
      title: "hex2bin 解码技巧在参数矩阵中的应用",
      component: null,
      severity: null,
      verified: 0,
      hit_count: 0,
      updated_at: "2026-11-08T09:00:00Z",
    },
  ],
};

const DETAIL_STUB = {
  available: true,
  source: "main",
  item: {
    knowledge_id: "k-redirect",
    record_kind: "own",
    lifecycle: "VERIFIED",
    title: "站群/落地页开放重定向专项测试法",
    component: "landing",
    cve: null,
    severity: "high",
    language: null,
    evidence_level: "K3",
    verified: 1,
    verified_note: "3 站实测成立",
    hit_count: 7,
    description: "覆盖 h2 陷阱与路径面。",
    usage: "示例用法",
    content: "正文第一段\n第二段：参数矩阵。",
    source: "web",
    source_url: "https://example.org/note",
    tags: ["open-redirect", "landing"],
    updated_at: "2026-11-09T10:00:00Z",
  },
};

function okFetch(): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      let body: unknown = {};
      if (url.includes("knowledge_id=")) body = DETAIL_STUB;
      else if (url.includes("q="))
        body = { ...LIST_STUB, total: 1, returned: 1, items: [LIST_STUB.items[0]] };
      else if (
        url.startsWith("http://localhost:5173/api/knowledge") ||
        url.includes("/api/knowledge")
      )
        body = LIST_STUB;
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json; charset=utf-8" },
      });
    }),
  );
}

beforeEach(() => {
  okFetch();
  window.history.pushState({}, "", "/knowledge");
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("Knowledge 页（P5-6）", () => {
  it("POC 治理视图展示 M3 独立状态与只读复测历史", async () => {
    const governanceItem = {
      poc_id: 42,
      title: "Fixture POC governance",
      evidence_level: "K0",
      lifecycle: "Candidate",
      verification_result: "candidate_not_verified",
      review_gate: "Recover raw request/response evidence",
      missing_evidence: ["raw_request_response"],
      evidence_refs: ["local://evidence/fixture"],
      related_relations: [
        {
          direction: "outgoing" as const,
          target: "fixture-case",
          target_title: "Fixture case group",
          type: "supports",
          confidence: 0.8,
          note: "review-only relationship fixture",
        },
      ],
      assessment: {
        evidence_completeness: "not_assessed",
        artifact_validity: "not_assessed",
        remediation_status: "unknown",
        license_status: "unknown",
        review_status: "candidate",
      },
    };
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.includes("/api/curated-pocs/governance?poc_id=42")
        ? {
            available: true,
            item: governanceItem,
            retest_history: {
              rtis_poc_id: 42,
              attempts: [
                {
                  item_id: "plan-item-42",
                  provider: "nuclei",
                  provider_version: "3.4.0",
                  result_status: "NOT_TESTED",
                  error_category: "tool_unavailable",
                  transitions: [{ from_status: "PENDING", to_status: "NOT_TESTED" }],
                  evidence: [
                    {
                      evidence_id: "evidence-1",
                      evidence_type: "note",
                      content_hash: "a".repeat(64),
                    },
                  ],
                },
              ],
            },
          }
        : url.includes("/api/curated-pocs/governance")
          ? {
              available: true,
              summary: { total: 1, assessed: 0, not_assessed: 1, evidence_gaps: 1 },
              items: [governanceItem],
            }
          : LIST_STUB;
      return new Response(JSON.stringify(body), { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    fireEvent.click(await screen.findByTestId("knowledge-poc-governance-tab"));
    expect(await screen.findByText("Fixture POC governance")).toBeTruthy();
    expect(screen.getByTestId("poc-governance-m3-summary").textContent).toContain("1");
    fireEvent.click(screen.getByRole("button", { name: /Fixture POC governance/ }));
    expect(await screen.findByText("NOT_TESTED")).toBeTruthy();
    expect(screen.getByTestId("poc-governance-detail").textContent).toContain("not_assessed");
    expect(screen.getByTestId("poc-governance-detail").textContent).toContain("unknown");
    expect(screen.getByTestId("poc-governance-detail").textContent).toContain("Fixture case group");
    expect(screen.getByTestId("poc-governance-detail").textContent).toContain(
      "review-only relationship fixture",
    );
    expect(screen.getByTestId("poc-governance-detail").textContent).toContain("a".repeat(64));
    expect(screen.getByTestId("poc-governance-detail").textContent).not.toContain("payload");
    expect(fetchMock.mock.calls.some((call) => String(call[0]).includes("poc_id=42"))).toBe(true);
  });
  it("自有 POC 证据工作台：默认待补证队列、治理元数据与结构化搜索", async () => {
    const queueHit = {
      resource_id: "rtis:poc:900033",
      name: "Zeai 图形验证码待补证候选",
      resource_type: "rtis_poc",
      evidence_level: "K0",
      knowledge_lifecycle: "Candidate",
      poc_category: "auth-bypass",
      poc_severity: "high",
      source_path: "integrations/rtis/pocs/zeai-captcha",
      source_commit: "022a71718740a7fad74ca194a10e9cc9269501c4",
      related_poc_ids: ["supports:900031"],
      poc_source_integrity: "missing",
      poc_verification_result: "candidate_not_verified",
      poc_dedup_status: "independent",
      poc_independent_knowledge_status: "independent",
      poc_missing_evidence: ["raw_challenge_response", "negative_control"],
      poc_evidence_refs: ["integrations/rtis/pocs/zeai-captcha/poc.py"],
      poc_source_urls: ["https://example.org/zeai-reference"],
      poc_review_gate: "补齐合成挑战响应与阴性对照记录。",
      description: null,
    };
    const pocHit = {
      ...queueHit,
      resource_id: "rtis:poc:900031",
      name: "Zeai 会员照片访问控制检查",
      evidence_level: "K2",
      knowledge_lifecycle: "Researched",
      poc_category: "unauthorized",
      poc_source_integrity: "partial",
      poc_verification_result: "historical_evidence_supported",
      poc_missing_evidence: [],
      poc_evidence_refs: ["integrations/rtis/pocs/zeai-photo-blur-bypass/SUMMARY.md"],
      poc_source_urls: ["https://example.org/zeai-reference"],
      poc_review_gate: null,
    };
    const summary = {
      available: true,
      total_curated: 49,
      by_evidence_level: { K0: 29, K1: 9, K2: 10, K3: 1 },
      by_lifecycle: { Candidate: 29, Researched: 19, "Lab-validated": 1 },
      by_source_integrity: { missing: 21, partial: 28 },
      by_verification_result: {
        candidate_not_verified: 30,
        historical_evidence_supported: 11,
        not_applicable: 8,
      },
      missing_evidence_entries: 33,
      missing_evidence_items: 94,
      entries_with_source_path: 42,
      entries_without_source_path: 7,
      entries_with_source_commit: 39,
      entries_without_source_commit: 10,
      relation_count: 25,
      external_entity_count: 2,
      review_queue_count: 33,
      metadata_incomplete_entries: 0,
    };
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.includes("/api/curated-pocs/summary")
        ? summary
        : url.includes("/api/curated-pocs/search")
          ? url.includes("q=zeai")
            ? {
                available: true,
                query: "zeai",
                total: 1,
                returned: 1,
                hits: [pocHit],
                filters: { resource_type: "rtis_poc", evidence_level: "K2" },
              }
            : {
                available: true,
                query: "",
                total: 1,
                returned: 1,
                hits: [queueHit],
                filters: { resource_type: "rtis_poc", poc_missing_evidence: "true" },
              }
          : LIST_STUB;
      return new Response(JSON.stringify(body), { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    fireEvent.click(await screen.findByTestId("knowledge-poc-tab"));
    expect(await screen.findByText("Zeai 图形验证码待补证候选")).toBeTruthy();
    expect(screen.getByTestId("poc-governance-summary").textContent).toContain("49");
    expect(screen.getByTestId("poc-governance-summary").textContent).toContain("33");
    const queueResults = screen.getByTestId("curated-poc-results");
    expect(queueResults.textContent).toContain("raw_challenge_response");
    expect(queueResults.textContent).toContain("negative_control");
    expect(queueResults.textContent).toContain("补齐合成挑战响应与阴性对照记录。");
    expect(screen.getByText("candidate_not_verified")).toBeTruthy();
    fireEvent.change(screen.getByRole("combobox", { name: "缺失证据筛选" }), {
      target: { value: "" },
    });
    fireEvent.change(screen.getByRole("searchbox", { name: "搜索自有 POC" }), {
      target: { value: "zeai" },
    });
    fireEvent.change(screen.getByRole("combobox", { name: "证据等级筛选" }), {
      target: { value: "K2" },
    });
    expect(await screen.findByText("Zeai 会员照片访问控制检查")).toBeTruthy();
    expect(screen.getByTestId("poc-search-total").textContent).toContain("1");
    const resultText = screen.getByTestId("curated-poc-results").textContent ?? "";
    expect(resultText).toContain("integrations/rtis/pocs/zeai-photo-blur-bypass/SUMMARY.md");
    expect(resultText).toContain("022a71718740");
    expect(resultText).toContain("https://example.org/zeai-reference");
    await waitFor(() => {
      const called = fetchMock.mock.calls.some((call) => {
        const url = String(call[0]);
        return (
          url.includes("/api/curated-pocs/search") &&
          url.includes("q=zeai") &&
          url.includes("evidence_level=K2")
        );
      });
      if (!called) throw new Error("POC search filters were not sent");
    });
    expect(screen.queryByText("源材料正文不应出现在检索结果中")).toBeNull();
  });

  it("渲染列表：标题、生命周期徽标、验证状态与总数", async () => {
    render(<App />);
    expect(await screen.findByText("站群/落地页开放重定向专项测试法")).toBeTruthy();
    expect(screen.getByText("hex2bin 解码技巧在参数矩阵中的应用")).toBeTruthy();
    expect(screen.getByText("共 2 条（显示 2）")).toBeTruthy();
    expect(screen.getAllByText("VERIFIED").length).toBeGreaterThan(0);
    expect(screen.getAllByText("已验证").length).toBeGreaterThan(0);
    expect(screen.getAllByText("未验证").length).toBeGreaterThan(0);
  });

  it("检索走 q= 参数且防抖生效：输入后 300ms 才发带关键词的请求", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.includes("q=")
        ? { ...LIST_STUB, total: 1, returned: 1, items: [LIST_STUB.items[0]] }
        : LIST_STUB;
      return new Response(JSON.stringify(body), { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await screen.findByText("共 2 条（显示 2）");
    const callsBefore = fetchMock.mock.calls.filter((call) =>
      String(call[0]).includes("q="),
    ).length;
    const input = screen.getByRole("textbox", { name: "检索知识条目" });
    fireEvent.change(input, { target: { value: "重定向" } });
    // 防抖窗口内（<300ms）：还没有带 q= 的请求
    await new Promise((resolve) => setTimeout(resolve, 120));
    const callsDuringDebounce = fetchMock.mock.calls.filter((call) =>
      String(call[0]).includes("q="),
    ).length;
    expect(callsDuringDebounce).toBe(callsBefore);
    // 防抖结束：请求带 q=（encodeURIComponent 后的「重定向」）且列表更新
    await waitFor(
      () => {
        const withQ = fetchMock.mock.calls.filter((call) => {
          const url = String(call[0]);
          return url.includes("q=") && decodeURIComponent(url).includes("重定向");
        });
        if (withQ.length === 0) throw new Error("带 q= 的检索请求未发出");
      },
      { timeout: 1500 },
    );
    expect(await screen.findByText("检索「重定向」：共 1 条（显示 1）")).toBeTruthy();
  });

  it("无命中显示服务端真实空态文案", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        const body = url.includes("q=")
          ? {
              available: true,
              total: 0,
              returned: 0,
              items: [],
              message: "没有匹配的知识条目，调整关键词或生命周期后重试",
            }
          : LIST_STUB;
        return new Response(JSON.stringify(body), { status: 200 });
      }),
    );
    render(<App />);
    await screen.findByText("共 2 条（显示 2）");
    fireEvent.change(screen.getByRole("textbox", { name: "检索知识条目" }), {
      target: { value: "zzz-不存在" },
    });
    expect(await screen.findByText("没有匹配的知识条目，调整关键词或生命周期后重试")).toBeTruthy();
  });

  it("Loading 态渲染骨架，Error 态渲染 ErrorPanel 且可重试", async () => {
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      if (String(input).includes("/api/knowledge") && !String(input).includes("q=")) {
        return new Promise<Response>(() => {});
      }
      return Promise.resolve(new Response(JSON.stringify(LIST_STUB), { status: 200 }));
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    expect(document.querySelector(".skeleton")).toBeTruthy();
    // 切 Error
    cleanup();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify({ message: "内部错误" }), { status: 500 })),
    );
    render(<App />);
    expect(await screen.findByText("加载失败")).toBeTruthy();
    expect(screen.getByRole("button", { name: /重试/ })).toBeTruthy();
  });

  it("点击列表项：详情侧栏按 knowledge_id= 拉取并呈现来源/元数据/正文摘要", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.includes("knowledge_id=")
        ? DETAIL_STUB
        : url.includes("q=")
          ? { ...LIST_STUB, total: 1, returned: 1, items: [LIST_STUB.items[0]] }
          : LIST_STUB;
      return new Response(JSON.stringify(body), { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    fireEvent.click(await screen.findByText("站群/落地页开放重定向专项测试法"));
    const aside = await screen.findByRole("complementary", { name: "条目详情" });
    await waitFor(() => {
      const called = fetchMock.mock.calls.some((call) =>
        String(call[0]).includes("knowledge_id=k-redirect"),
      );
      if (!called) throw new Error("详情请求未发出");
    });
    // 来源
    expect(aside.textContent).toContain("来源");
    expect(aside.textContent).toContain("https://example.org/note");
    // 元数据
    expect(aside.textContent).toContain("landing");
    expect(aside.textContent).toContain("K3");
    expect(aside.textContent).toContain("3 站实测成立");
    expect(aside.textContent).toContain("open-redirect");
    // 正文摘要
    expect(aside.textContent).toContain("正文摘要");
    expect(aside.textContent).toContain("第二段：参数矩阵。");
  });

  it("深链契约：?knowledge_id= 直接打开详情侧栏并同步选中态", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.includes("knowledge_id=")
        ? DETAIL_STUB
        : url.includes("q=")
          ? { ...LIST_STUB, total: 1, returned: 1, items: [LIST_STUB.items[0]] }
          : LIST_STUB;
      return new Response(JSON.stringify(body), { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    // ⌘K 面板知识命中的落点形态：/knowledge?knowledge_id=k-redirect
    window.history.pushState({}, "", "/knowledge?knowledge_id=k-redirect");
    render(<App />);
    const aside = await screen.findByRole("complementary", { name: "条目详情" });
    await waitFor(() => {
      const called = fetchMock.mock.calls.some((call) =>
        String(call[0]).includes("knowledge_id=k-redirect"),
      );
      if (!called) throw new Error("深链详情请求未发出");
    });
    // 详情侧栏呈现深链条目
    expect(aside.textContent).toContain("站群/落地页开放重定向专项测试法");
    // 等列表渲染完成后再断言选中态与 URL 同步（高亮属性在行按钮上）
    await screen.findByText("共 2 条（显示 2）");
    const activeRow = document.querySelector('button[aria-current="true"]');
    expect(activeRow?.textContent).toContain("站群/落地页开放重定向专项测试法");
    // 点击另一行：URL 参数同步更新（replace 语义）
    fireEvent.click(screen.getByText("hex2bin 解码技巧在参数矩阵中的应用"));
    await waitFor(() => {
      if (!window.location.search.includes("knowledge_id=k-hex2bin")) {
        throw new Error("URL 未随选中同步");
      }
    });
    expect(window.location.search).toContain("knowledge_id=k-hex2bin");
  });

  it("Nuclei 模板可见结构化元数据，并支持显式批量审核与审计历史", async () => {
    let reviewStatus = "DISCOVERED";
    let reviewHistory: Array<Record<string, unknown>> = [];
    const pluginId = "knowledge.rtis-redteam";
    const itemKey = "http/exposed-panels/test-review-fixture.yaml";
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = init?.method ?? "GET";
      if (url.includes("/api/plugins/") && url.endsWith("/review") && method === "POST") {
        const body = JSON.parse(String(init?.body ?? "{}")) as {
          status: string;
          item_keys: string[];
        };
        reviewStatus = body.status;
        reviewHistory = [
          {
            event_id: 1,
            ts: "2026-11-26T12:00:00Z",
            actor: "test-reviewer",
            plugin_id: pluginId,
            status: body.status,
            item_keys: body.item_keys,
            updated: body.item_keys.length,
          },
        ];
        return new Response(
          JSON.stringify({
            ok: true,
            action: "review",
            plugin_id: pluginId,
            result: { updated: body.item_keys.length },
          }),
          { status: 200 },
        );
      }
      if (url.includes("/api/knowledge_sources")) {
        return new Response(
          JSON.stringify({
            available: true,
            total: 1,
            returned: 1,
            plugins: [
              {
                plugin_id: pluginId,
                name: "RTIS Red Team",
                version: "0.2.6",
                state: "ENABLED",
                license: "MIT",
              },
            ],
            summary: [
              {
                plugin_id: pluginId,
                entry_kind: "nuclei_template",
                review_status: reviewStatus,
                n: 1,
              },
            ],
            items: [
              {
                plugin_id: pluginId,
                plugin_version: "0.2.6",
                corpus_id: "nuclei-templates",
                entry_kind: "nuclei_template",
                item_key: itemKey,
                title: "Test exposed panel",
                description: "Metadata-only review fixture",
                category_path: ["http", "exposed-panels"],
                license_status: "MIT",
                rights_policy: "approved_only",
                review_status: reviewStatus,
                source_commit: "a346d8f00c61bf95bbd30365f12cf923be51ba3d",
                source_url: "https://github.com/projectdiscovery/nuclei-templates",
                source_location: { path: itemKey },
                body_bytes: 128,
                last_seen_at: "2026-11-26T12:00:00Z",
                nuclei_template: {
                  template_id: "test-exposed-panel",
                  severity: "high",
                  parse_status: "parsed",
                  protocols: ["http"],
                  tags: ["panel", "exposure"],
                  classification: {
                    "cve-id": ["CVE-2026-12345"],
                    "cwe-id": ["CWE-200"],
                    "cvss-score": 9.8,
                    "epss-score": 0.90338,
                    "epss-percentile": 0.99802,
                  },
                },
                pocindex_record: {
                  cve_id: "CVE-2026-12345",
                  cisa_date_added: "2026-11-01",
                  pocindex_kev_present: true,
                  epss_score: 0.90338,
                  epss_percentile: 0.99802,
                  cvss_assessments: [
                    {
                      version: "3.1",
                      score: 9.8,
                      severity: "CRITICAL",
                      vector: "CVSS:3.1/AV:N",
                      source: "nvd@nist.gov",
                      assessment_type: "Primary",
                    },
                  ],
                  advisories: [{ url: "https://example.com/advisory", tags: ["Vendor Advisory"] }],
                  nuclei_metadata: { severity: "critical", cvss: 9.8, cwe: "CWE-78" },
                  snapshot: {
                    repository_commit: "e9076fea96095e014fc107c02dd23ee8dc21476d",
                    retrieved_at: "2026-11-26T07:30:41Z",
                  },
                },
                cve_record: {
                  schema: "cisa-kev-v1",
                  cve_id: "CVE-2026-12345",
                  source_authority: "CISA",
                  known_exploited: true,
                  vendor_project: "Synthetic Vendor",
                  product: "Synthetic Product",
                  date_added: "2026-11-01",
                  due_date: "2026-11-22",
                  known_ransomware_campaign_use: "Unknown",
                  forensic_triage: "Yes",
                  cwes: ["CWE-200"],
                  cvss_score: null,
                  epss_score: null,
                  poc_status: "not_assessed",
                  current_target_status: "not_tested",
                  required_action: "Apply the vendor update.",
                },
              },
            ],
            review_history: reviewHistory,
          }),
          { status: 200 },
        );
      }
      return new Response(JSON.stringify({}), { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "知识来源" }));
    expect(await screen.findByText("Test exposed panel")).toBeTruthy();
    expect(screen.getByTestId("nuclei-template-metadata").textContent).toContain("high");
    expect(screen.getByTestId("nuclei-template-metadata").textContent).toContain("CVE-2026-12345");
    expect(screen.getByTestId("nuclei-template-metadata").textContent).toContain("CWE-200");
    expect(screen.getByTestId("nuclei-template-metadata").textContent).toContain(
      "Nuclei CVSS: 9.8",
    );
    expect(screen.getByTestId("nuclei-template-metadata").textContent).toContain(
      "Nuclei EPSS: 0.90338",
    );
    expect(screen.getByTestId("pocindex-record-metadata").textContent).toContain("0.90338");
    expect(screen.getByTestId("pocindex-record-metadata").textContent).toContain("9.8 CRITICAL");
    expect(screen.getByTestId("pocindex-record-metadata").textContent).toContain("Vendor Advisory");
    expect(screen.getByTestId("cve-record-metadata").textContent).toContain("CVE-2026-12345");
    expect(screen.getByTestId("cve-record-metadata").textContent).toContain("CISA KEV");
    expect(screen.getByTestId("cve-record-metadata").textContent).toContain(
      "CVSS/EPSS 未由 KEV 提供",
    );
    expect(screen.getByTestId("cve-record-metadata").textContent).toContain("PoC 未评估");
    expect(screen.getByTestId("cve-record-metadata").textContent).toContain("取证分诊: Yes");
    expect(screen.getByTestId("source-item-key").textContent).toContain(itemKey);

    fireEvent.click(screen.getByRole("checkbox", { name: `选择条目 ${itemKey}` }));
    fireEvent.change(screen.getByRole("textbox", { name: "审核理由" }), {
      target: { value: "Fixture metadata and source provenance reviewed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "批准所选" }));
    await waitFor(() => expect(reviewStatus).toBe("APPROVED"));
    expect(await screen.findByTestId("knowledge-source-review-history")).toBeTruthy();
    expect(screen.getByTestId("knowledge-source-review-history").textContent).toContain(
      "test-reviewer",
    );
    expect(screen.getByTestId("knowledge-source-review-history").textContent).toContain("APPROVED");
    const reviewCall = fetchMock.mock.calls.find(
      (call) =>
        String(call[0]).includes("/api/plugins/knowledge.rtis-redteam/review") &&
        call[1]?.method === "POST",
    );
    expect(reviewCall).toBeTruthy();
    expect(String(reviewCall?.[1]?.body)).toContain('"version":"0.2.6"');
    expect(String(reviewCall?.[1]?.body)).toContain(
      '"reason":"Fixture metadata and source provenance reviewed"',
    );
  });
});
