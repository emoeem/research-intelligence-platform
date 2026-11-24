import { useMemo, useState, type FormEvent } from "react";
import { apiSend } from "../lib/api";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel, StatusBadge } from "../components/ui";
import { useUiText } from "../i18n";

interface FuzzJob {
  job_id: string;
  session_id: string;
  state: string;
  estimated_requests?: number;
  max_requests?: number;
  max_concurrency?: number;
  duration_ms?: number | null;
  error_code?: string | null;
  created_at: string;
  started_at?: string | null;
  completed_at?: string | null;
  case_counts?: Record<string, number>;
}

interface JobsData {
  jobs: FuzzJob[];
  limit: number;
}

interface FuzzCase {
  case_id: string;
  ordinal: number;
  state: string;
  response_status?: number | null;
  response_len?: number | null;
  duration_ms?: number | null;
  ttfb_ms?: number | null;
  diff_class?: string | null;
  similarity?: number | null;
  error_code?: string | null;
  exchange_id?: string | null;
}

interface JobDetail extends FuzzJob {
  cases: FuzzCase[];
}

interface FuzzSpec {
  request: {
    method: string;
    target: string;
    version: "http/1.0" | "http/1.1";
    headers: Array<{ name: string; value: string }>;
    body: string;
    trailers: Array<{ name: string; value: string }>;
    framing: "content_length" | "chunked";
  };
  positions: Array<Record<string, unknown>>;
  payloads: Array<{ type: "list"; values: string[] }>;
  mutators: Array<Record<string, unknown>>;
  processors: Array<Record<string, unknown>>;
  analyze: { keywords: string[]; regexes: string[] };
  mode: "single" | "cartesian" | "zip";
  budget: {
    max_requests: number;
    max_concurrency: number;
    rate_window_ms: number;
    rate_max_requests: number;
    min_request_interval_ms: number;
    max_duration_ms: number;
  };
}

interface CancelResponse {
  cancelled: boolean;
  note: string;
}

const tones: Record<string, string> = {
  COMPLETED: "badge-success",
  SUCCEEDED: "badge-success",
  FAILED: "badge-error",
  CANCELLED: "badge-warning",
  RUNNING: "badge-info",
  QUEUED: "badge-ghost",
  CREATED: "badge-ghost",
  PREPARING: "badge-info",
  CANCELLING: "badge-warning",
  PAUSED: "badge-warning",
  BUDGET_EXHAUSTED: "badge-warning",
};

const ACTIVE_STATES = new Set(["CREATED", "QUEUED", "RUNNING", "PAUSED", "CANCELLING"]);

function encodeBase64Utf8(value: string): string {
  const bytes = new TextEncoder().encode(value);
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}

function parseHeaderLines(
  value: string,
  translate: (source: string) => string,
): Array<{ name: string; value: string }> {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const separator = line.indexOf(":");
      if (separator <= 0) throw new Error(translate("请求头必须使用 Name: value 格式"));
      return {
        name: line.slice(0, separator).trim(),
        value: line.slice(separator + 1).trim(),
      };
    });
}

function countMarkers(parts: string[]): { count: number; invalid: boolean } {
  let count = 0;
  let invalid = false;
  for (const part of parts) {
    const delimiters = part.match(/§/g)?.length ?? 0;
    const matches = part.match(/§[^§]+§/g)?.length ?? 0;
    if (delimiters !== matches * 2) invalid = true;
    count += matches;
  }
  return { count, invalid };
}

function makeSimpleSpec(
  input: {
    method: string;
    target: string;
    version: "http/1.0" | "http/1.1";
    headers: string;
    body: string;
    payloadText: string;
    mode: "single" | "cartesian" | "zip";
    maxRequests: number;
    maxConcurrency: number;
    rateWindowMs: number;
    rateMaxRequests: number;
    minRequestIntervalMs: number;
    maxDurationMs: number;
  },
  translate: (source: string) => string,
): FuzzSpec {
  const headers = parseHeaderLines(input.headers, translate);
  const target = input.target.trim();
  const marker = countMarkers([target, ...headers.map((header) => header.value), input.body]);
  if (!target) throw new Error(translate("请填写目标请求路径或绝对 URL"));
  if (marker.invalid || marker.count === 0) {
    throw new Error(translate("请在请求目标、请求头或请求体中放置成对的 §payload§ 标记"));
  }
  const values = input.payloadText
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter(Boolean);
  if (values.length === 0) throw new Error(translate("至少填写一个 Payload，每行一个值"));
  if (!Number.isSafeInteger(input.maxRequests) || input.maxRequests < 1) {
    throw new Error(translate("最大请求数必须是大于 0 的整数"));
  }
  if (!Number.isSafeInteger(input.maxConcurrency) || input.maxConcurrency < 1) {
    throw new Error(translate("并发上限必须是大于 0 的整数"));
  }
  if (
    !Number.isSafeInteger(input.rateWindowMs) ||
    input.rateWindowMs < 1 ||
    !Number.isSafeInteger(input.rateMaxRequests) ||
    input.rateMaxRequests < 1
  ) {
    throw new Error(translate("速率窗口和窗口请求数都必须是大于 0 的整数"));
  }
  if (
    !Number.isSafeInteger(input.minRequestIntervalMs) ||
    input.minRequestIntervalMs < 0 ||
    !Number.isSafeInteger(input.maxDurationMs) ||
    input.maxDurationMs < 1
  ) {
    throw new Error(translate("请求最小间隔不能小于 0，最长运行时间必须大于 0"));
  }

  if (!headers.some((header) => header.name.toLowerCase() === "host")) {
    try {
      const parsed = new URL(target);
      headers.unshift({ name: "Host", value: parsed.host });
    } catch {
      throw new Error(translate("使用相对路径时必须手动填写 Host 请求头"));
    }
  }

  return {
    request: {
      method: input.method.trim().toUpperCase(),
      target,
      version: input.version,
      headers,
      body: encodeBase64Utf8(input.body),
      trailers: [],
      framing: "content_length",
    },
    positions: [],
    payloads: Array.from({ length: marker.count }, () => ({ type: "list" as const, values })),
    mutators: [],
    processors: [],
    analyze: { keywords: [], regexes: [] },
    mode: input.mode,
    budget: {
      max_requests: input.maxRequests,
      max_concurrency: input.maxConcurrency,
      rate_window_ms: input.rateWindowMs,
      rate_max_requests: input.rateMaxRequests,
      min_request_interval_ms: input.minRequestIntervalMs,
      max_duration_ms: input.maxDurationMs,
    },
  };
}

function estimatedRequests(
  mode: "single" | "cartesian" | "zip",
  markerCount: number,
  values: number,
): string {
  if (markerCount < 1 || values < 1) return "—";
  let estimate: bigint;
  if (mode === "single") estimate = BigInt(markerCount) * BigInt(values);
  else if (mode === "zip") estimate = BigInt(values);
  else estimate = BigInt(values) ** BigInt(markerCount);
  return estimate.toLocaleString();
}

export function FuzzerJobsPage() {
  const ui = useUiText();
  const jobs = useApi<JobsData>("/api/wsw/fuzz/jobs?limit=100", { refetchIntervalMs: 5000 });
  const [selected, setSelected] = useState("");
  const selectedJob = jobs.data?.jobs.find((job) => job.job_id === selected);
  const detailQuery = useApi<JobDetail>(
    selected ? `/api/wsw/fuzz/jobs/${encodeURIComponent(selected)}` : null,
    {
      refetchIntervalMs:
        selected && (!selectedJob || ACTIVE_STATES.has(selectedJob.state.toUpperCase()))
          ? 3000
          : undefined,
    },
  );
  const detail = detailQuery.data;
  const [createBusy, setCreateBusy] = useState(false);
  const [cancelBusy, setCancelBusy] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [advancedMode, setAdvancedMode] = useState(false);
  const [advancedSpec, setAdvancedSpec] = useState("");
  const [confirmedScope, setConfirmedScope] = useState(false);

  const [method, setMethod] = useState("GET");
  const [target, setTarget] = useState("");
  const [version, setVersion] = useState<"http/1.0" | "http/1.1">("http/1.1");
  const [headers, setHeaders] = useState("User-Agent: RIP Fuzzer");
  const [body, setBody] = useState("");
  const [payloadText, setPayloadText] = useState("");
  const [mode, setMode] = useState<"single" | "cartesian" | "zip">("single");
  const [maxRequests, setMaxRequests] = useState("25");
  const [maxConcurrency, setMaxConcurrency] = useState("1");
  const [rateWindowMs, setRateWindowMs] = useState("1000");
  const [rateMaxRequests, setRateMaxRequests] = useState("1");
  const [minRequestIntervalMs, setMinRequestIntervalMs] = useState("1000");
  const [maxDurationMs, setMaxDurationMs] = useState("60000");

  const headerParts = useMemo(() => headers.split(/\r?\n/).filter(Boolean), [headers]);
  const marker = useMemo(
    () => countMarkers([target, ...headerParts, body]),
    [target, headerParts, body],
  );
  const payloadCount = useMemo(
    () =>
      payloadText
        .split(/\r?\n/)
        .map((item) => item.trim())
        .filter(Boolean).length,
    [payloadText],
  );
  const previewEstimate = useMemo(
    () => estimatedRequests(mode, marker.count, payloadCount),
    [mode, marker.count, payloadCount],
  );

  async function createJob(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setNotice(null);
    if (!confirmedScope) {
      setError(ui("请先确认目标已在你获授权的测试 Scope 内。"));
      return;
    }
    setCreateBusy(true);
    try {
      let spec: FuzzSpec;
      if (advancedMode) {
        const parsed: unknown = JSON.parse(advancedSpec);
        if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
          throw new Error(ui("高级 Spec 必须是 JSON object"));
        }
        spec = parsed as FuzzSpec;
      } else {
        spec = makeSimpleSpec(
          {
            method,
            target,
            version,
            headers,
            body,
            payloadText,
            mode,
            maxRequests: Number(maxRequests),
            maxConcurrency: Number(maxConcurrency),
            rateWindowMs: Number(rateWindowMs),
            rateMaxRequests: Number(rateMaxRequests),
            minRequestIntervalMs: Number(minRequestIntervalMs),
            maxDurationMs: Number(maxDurationMs),
          },
          ui,
        );
      }
      const result = await apiSend<{ job_id: string; status_url: string }>(
        "POST",
        "/api/wsw/fuzz/jobs",
        { spec },
      );
      setNotice(`${ui("已提交 Fuzzer Job")}: ${result.job_id}`);
      setConfirmedScope(false);
      jobs.retry();
    } catch (submitError) {
      setError(submitError instanceof Error ? submitError.message : String(submitError));
    } finally {
      setCreateBusy(false);
    }
  }

  async function cancelJob(job: FuzzJob) {
    if (!ACTIVE_STATES.has(job.state.toUpperCase()) || cancelBusy) return;
    if (!window.confirm(ui("确认取消这个 Fuzzer Job？已派发的请求无法撤回。"))) return;
    setCancelBusy(job.job_id);
    setError(null);
    setNotice(null);
    try {
      const result = await apiSend<CancelResponse>(
        "POST",
        `/api/wsw/fuzz/jobs/${encodeURIComponent(job.job_id)}/cancel`,
        {},
      );
      setNotice(result.note);
      jobs.retry();
      if (selected === job.job_id) detailQuery.retry();
    } catch (cancelError) {
      setError(cancelError instanceof Error ? cancelError.message : String(cancelError));
    } finally {
      setCancelBusy("");
    }
  }

  const renderedJobs = jobs.data?.jobs ?? [];

  return (
    <div className="wsw-workspace-page space-y-4" data-testid="wsw-fuzzer-page">
      <PageHeader
        title={ui("Intruder / Fuzzer Jobs")}
        actions={<span className="badge badge-outline">{ui("WSW Rust Engine")}</span>}
      />
      <section
        className="rounded-surface border border-edge bg-base-100 p-4"
        aria-labelledby="fuzzer-create-title"
      >
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 id="fuzzer-create-title" className="text-sm font-semibold">
              {ui("新建 Intruder / Fuzzer Job")}
            </h2>
            <p className="mt-1 text-xs text-muted">
              {ui("任务仅在你明确提交后创建；目标 Scope、Payload 与预算由后端 Rust 引擎再次预检。")}
            </p>
          </div>
          <label className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={advancedMode}
              onChange={(event) => {
                setAdvancedMode(event.target.checked);
                setError(null);
              }}
            />
            {ui("高级 JSON Spec")}
          </label>
        </div>
        <form className="space-y-4" onSubmit={(event) => void createJob(event)}>
          {advancedMode ? (
            <label className="block space-y-1 text-xs font-medium">
              <span>{ui("完整 FuzzSpec JSON")}</span>
              <textarea
                className="textarea textarea-sm min-h-64 w-full font-mono text-xs"
                aria-label={ui("完整 FuzzSpec JSON")}
                value={advancedSpec}
                onChange={(event) => setAdvancedSpec(event.target.value)}
                placeholder={ui("在这里粘贴符合 WSW Rust FuzzSpec 契约的 JSON object。")}
                spellCheck={false}
              />
              <span className="text-muted font-normal">
                {ui(
                  "高级模式保留 Rust 引擎支持的七类结构化位置、Payload Source、Mutator、Processor 与分析规则。",
                )}
              </span>
            </label>
          ) : (
            <>
              <div className="grid gap-3 md:grid-cols-4">
                <label className="space-y-1 text-xs font-medium">
                  <span>{ui("HTTP 方法")}</span>
                  <select
                    className="select select-sm w-full"
                    value={method}
                    onChange={(event) => setMethod(event.target.value)}
                  >
                    {["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].map((item) => (
                      <option key={item}>{item}</option>
                    ))}
                  </select>
                </label>
                <label className="space-y-1 text-xs font-medium">
                  <span>{ui("HTTP 版本")}</span>
                  <select
                    className="select select-sm w-full"
                    value={version}
                    onChange={(event) => setVersion(event.target.value as typeof version)}
                  >
                    <option value="http/1.1">HTTP/1.1</option>
                    <option value="http/1.0">HTTP/1.0</option>
                  </select>
                </label>
                <label className="space-y-1 text-xs font-medium md:col-span-2">
                  <span>{ui("组合模式")}</span>
                  <select
                    className="select select-sm w-full"
                    value={mode}
                    onChange={(event) => setMode(event.target.value as typeof mode)}
                  >
                    <option value="single">{ui("Single / Sniper（逐位置）")}</option>
                    <option value="cartesian">{ui("Cartesian / Cluster Bomb（笛卡尔积）")}</option>
                    <option value="zip">{ui("Zip / Pitchfork（等长同步配对）")}</option>
                  </select>
                </label>
              </div>
              <label className="block space-y-1 text-xs font-medium">
                <span>{ui("目标请求路径或绝对 URL")}</span>
                <input
                  className="input input-sm w-full font-mono"
                  aria-label={ui("目标请求路径或绝对 URL")}
                  value={target}
                  onChange={(event) => setTarget(event.target.value)}
                  placeholder="https://authorized.example.test/search?q=§value§"
                  required
                />
              </label>
              <div className="grid gap-3 lg:grid-cols-2">
                <label className="block space-y-1 text-xs font-medium">
                  <span>{ui("请求头（每行 Name: value）")}</span>
                  <textarea
                    className="textarea textarea-sm min-h-28 w-full font-mono text-xs"
                    aria-label={ui("请求头（每行 Name: value）")}
                    value={headers}
                    onChange={(event) => setHeaders(event.target.value)}
                    spellCheck={false}
                  />
                </label>
                <label className="block space-y-1 text-xs font-medium">
                  <span>{ui("请求体（可选；标记使用 §payload§）")}</span>
                  <textarea
                    className="textarea textarea-sm min-h-28 w-full font-mono text-xs"
                    aria-label={ui("请求体（可选；标记使用 §payload§）")}
                    value={body}
                    onChange={(event) => setBody(event.target.value)}
                    spellCheck={false}
                  />
                </label>
              </div>
              <label className="block space-y-1 text-xs font-medium">
                <span>{ui("Payload 列表（每行一个值，所有标记位置共用）")}</span>
                <textarea
                  className="textarea textarea-sm min-h-24 w-full font-mono text-xs"
                  aria-label={ui("Payload 列表（每行一个值，所有标记位置共用）")}
                  value={payloadText}
                  onChange={(event) => setPayloadText(event.target.value)}
                  placeholder={"alpha\nbeta\ngamma"}
                  spellCheck={false}
                />
              </label>
            </>
          )}

          {!advancedMode ? (
            <div className="rounded-control border border-edge-soft p-3">
              <div className="mb-3 flex items-center justify-between gap-3">
                <h3 className="text-xs font-semibold">{ui("执行预算（必填）")}</h3>
                <span className="text-xs text-muted tabular-nums">
                  {ui("预估请求数")}: {previewEstimate}
                </span>
              </div>
              <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
                {(
                  [
                    ["maxRequests", ui("最大请求数"), maxRequests, setMaxRequests],
                    ["maxConcurrency", ui("最大并发"), maxConcurrency, setMaxConcurrency],
                    ["rateWindowMs", ui("速率窗口（毫秒）"), rateWindowMs, setRateWindowMs],
                    ["rateMaxRequests", ui("窗口内请求数"), rateMaxRequests, setRateMaxRequests],
                    [
                      "minRequestIntervalMs",
                      ui("请求最小间隔（毫秒）"),
                      minRequestIntervalMs,
                      setMinRequestIntervalMs,
                    ],
                    ["maxDurationMs", ui("最长运行时间（毫秒）"), maxDurationMs, setMaxDurationMs],
                  ] as const
                ).map(([name, label, value, setter]) => (
                  <label key={name} className="space-y-1 text-xs font-medium">
                    <span>{label}</span>
                    <input
                      className="input input-sm w-full tabular-nums"
                      type="number"
                      min="0"
                      step="1"
                      name={name}
                      value={value}
                      onChange={(event) => setter(event.target.value)}
                    />
                  </label>
                ))}
              </div>
              <p className="mt-2 text-[11px] text-muted">
                {ui(
                  "默认采用低并发和速率限制；Rust 引擎强制总量、速率、间隔和最长运行时间四道预算闸。",
                )}
              </p>
            </div>
          ) : null}

          <label className="flex items-start gap-2 rounded-control border border-warning/40 bg-warning/5 p-3 text-xs">
            <input
              type="checkbox"
              className="checkbox checkbox-sm mt-0.5"
              checked={confirmedScope}
              onChange={(event) => setConfirmedScope(event.target.checked)}
              required
            />
            <span>
              {ui(
                "我确认目标属于我有权测试的 Scope，并已核对请求上限、并发和速率预算。创建后任务会开始真实请求，已发出的请求无法撤回。",
              )}
            </span>
          </label>

          {error ? (
            <p role="alert" className="text-xs text-error">
              {error}
            </p>
          ) : null}
          {notice ? (
            <p role="status" className="text-xs text-success">
              {notice}
            </p>
          ) : null}

          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-[11px] text-muted">
              {ui("没有目标、Payload 和 Scope 确认时，不会创建或发送请求。")}
            </p>
            <button
              type="submit"
              className="btn btn-sm btn-primary"
              disabled={createBusy || !confirmedScope}
            >
              {createBusy ? ui("提交中…") : ui("确认并创建 Fuzzer Job")}
            </button>
          </div>
        </form>
      </section>

      <section className="space-y-3" aria-labelledby="fuzzer-jobs-title">
        <div className="flex items-center justify-between gap-3">
          <h2 id="fuzzer-jobs-title" className="text-sm font-semibold">
            {ui("已有 Fuzzer Jobs")}
          </h2>
          <span className="text-xs text-muted">
            {renderedJobs.length} / {jobs.data?.limit ?? 100}
          </span>
        </div>
        {jobs.loading && !jobs.data ? (
          <SkeletonPanel lines={5} />
        ) : jobs.error ? (
          <ErrorPanel error={jobs.error} onRetry={jobs.retry} />
        ) : renderedJobs.length === 0 ? (
          <EmptyPanel
            text={ui("尚无 Fuzzer Job；填写请求模板、Payload 和预算后可创建第一项任务。")}
          />
        ) : (
          <div className="grid gap-4 xl:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]">
            <div className="space-y-2">
              {renderedJobs.map((job) => (
                <article
                  key={job.job_id}
                  className={`rounded-surface border border-edge bg-base-100 p-3 ${selected === job.job_id ? "ring-1 ring-primary" : ""}`}
                >
                  <div className="flex items-start gap-2">
                    <button
                      type="button"
                      className="min-w-0 flex-1 text-left"
                      onClick={() => setSelected(job.job_id)}
                    >
                      <strong className="block truncate font-mono text-xs">{job.job_id}</strong>
                      <small className="mt-1 block text-muted">{job.created_at}</small>
                      <span className="mt-2 inline-flex">
                        <StatusBadge status={job.state} tones={tones} />
                      </span>
                      <span className="mt-2 block text-[11px] text-muted">
                        {ui("预估请求数")}: {job.estimated_requests ?? "—"} · {ui("请求上限")}:{" "}
                        {job.max_requests ?? "—"} · {ui("并发")}: {job.max_concurrency ?? "—"}
                      </span>
                    </button>
                    {ACTIVE_STATES.has(job.state.toUpperCase()) ? (
                      <button
                        type="button"
                        className="btn btn-xs btn-outline btn-warning"
                        disabled={cancelBusy === job.job_id}
                        onClick={() => void cancelJob(job)}
                      >
                        {cancelBusy === job.job_id ? ui("取消中…") : ui("取消任务")}
                      </button>
                    ) : null}
                  </div>
                </article>
              ))}
            </div>
            <section className="rounded-surface border border-edge bg-base-100 p-4">
              <h3 className="mb-3 text-sm font-semibold">{ui("Job 详情")}</h3>
              {!selected ? (
                <EmptyPanel text={ui("从左侧选择一个 Job 查看详情。")} />
              ) : detailQuery.loading && !detail ? (
                <SkeletonPanel lines={4} />
              ) : detailQuery.error ? (
                <ErrorPanel error={detailQuery.error} onRetry={detailQuery.retry} />
              ) : detail ? (
                <>
                  <dl className="grid gap-2 text-xs sm:grid-cols-2">
                    <div>
                      <dt className="text-muted">{ui("Session")}</dt>
                      <dd className="break-all">{detail.session_id}</dd>
                    </div>
                    <div>
                      <dt className="text-muted">{ui("State")}</dt>
                      <dd>
                        <StatusBadge status={detail.state} tones={tones} />
                      </dd>
                    </div>
                    <div>
                      <dt className="text-muted">{ui("Estimated requests")}</dt>
                      <dd>{detail.estimated_requests ?? "—"}</dd>
                    </div>
                    <div>
                      <dt className="text-muted">{ui("Max requests")}</dt>
                      <dd>{detail.max_requests ?? "—"}</dd>
                    </div>
                    <div>
                      <dt className="text-muted">{ui("Concurrency")}</dt>
                      <dd>{detail.max_concurrency ?? "—"}</dd>
                    </div>
                    <div>
                      <dt className="text-muted">{ui("Duration")}</dt>
                      <dd>{detail.duration_ms ?? "—"} ms</dd>
                    </div>
                  </dl>
                  <div className="mt-4 border-t border-edge pt-3">
                    <h4 className="mb-2 text-xs font-semibold">
                      {ui("Case 列表")} ({detail.cases.length})
                    </h4>
                    <div className="overflow-x-auto">
                      <table className="table table-xs">
                        <thead>
                          <tr>
                            <th>#</th>
                            <th>{ui("状态")}</th>
                            <th>HTTP</th>
                            <th>{ui("时长")}</th>
                            <th>{ui("差异")}</th>
                          </tr>
                        </thead>
                        <tbody>
                          {detail.cases.map((item) => (
                            <tr key={item.case_id}>
                              <td>{item.ordinal}</td>
                              <td>{item.state}</td>
                              <td>{item.response_status ?? "—"}</td>
                              <td>{item.duration_ms ?? "—"}</td>
                              <td>{item.diff_class ?? "—"}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </div>
                </>
              ) : (
                <EmptyPanel text={ui("选择任务后显示服务端 Job 与 Case 状态。")} />
              )}
            </section>
          </div>
        )}
      </section>
    </div>
  );
}
