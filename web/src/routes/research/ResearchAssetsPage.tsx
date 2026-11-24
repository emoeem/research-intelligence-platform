import { useState, type FormEvent } from "react";
import { Activity, Plus, Search, Server, RefreshCw, Fingerprint } from "lucide-react";
import { apiSend } from "../../lib/api";
import { useApi } from "../../lib/hooks";
import {
  EmptyPanel,
  ErrorPanel,
  PageHeader,
  SkeletonPanel,
  StatusBadge,
} from "../../components/ui";
import { translateUi } from "../../i18n";

type Asset = {
  asset_id: string;
  workspace_id: string;
  asset_type: string;
  canonical_value: string;
  display_name: string;
  description: string;
  lifecycle_status: string;
  first_seen_at: string;
  last_seen_at: string;
  created_at: string;
  updated_at: string;
};
type Observation = {
  observation_id: string;
  observation_type: string;
  source_type: string;
  source_ref: string;
  evidence_ref: string;
  observed_at: string;
  content_hash: string;
  payload: Record<string, unknown>;
};
type AssetDetail = {
  asset: Asset;
  endpoints: {
    endpoint_id: string;
    scheme: string;
    host: string;
    port: number | null;
    protocol: string;
    path: string;
  }[];
  observations: Observation[];
  technologies: {
    technology_id: string;
    observation_id: string;
    technology: string;
    version: string;
    category: string;
    confidence: number;
    evidence_ref: string;
  }[];
};
type AssetList = { items: Asset[]; total: number; available: boolean };
export function ResearchAssetsPage() {
  const assetTypeLabels: Record<string, string> = {
    domain: translateUi("域名"),
    ip: translateUi("IP 地址"),
    url: "URL",
    service: translateUi("服务"),
    repository: translateUi("代码仓库"),
    application: translateUi("应用"),
    other: translateUi("其他"),
  };
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState("all");
  const [showCreate, setShowCreate] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [assetType, setAssetType] = useState("domain");
  const [canonical, setCanonical] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [description, setDescription] = useState("");
  const [endpointHost, setEndpointHost] = useState("");
  const [endpointPort, setEndpointPort] = useState("443");
  const [endpointScheme, setEndpointScheme] = useState("https");
  const [observationNote, setObservationNote] = useState("");
  const [observationEvidence, setObservationEvidence] = useState("");
  const [technologyName, setTechnologyName] = useState("");
  const [technologyObservation, setTechnologyObservation] = useState("");
  const [technologyEvidence, setTechnologyEvidence] = useState("");
  const [writeError, setWriteError] = useState("");
  const [saving, setSaving] = useState(false);
  const listPath = `/api/research/assets?workspace_id=default&limit=100${kind === "all" ? "" : `&type=${encodeURIComponent(kind)}`}${query.trim() ? `&q=${encodeURIComponent(query.trim())}` : ""}`;
  const list = useApi<AssetList>(listPath);
  const detail = useApi<AssetDetail>(
    selectedId
      ? `/api/research/assets/${encodeURIComponent(selectedId)}?workspace_id=default`
      : null,
  );

  const createAsset = async (event: FormEvent) => {
    event.preventDefault();
    setSaving(true);
    setWriteError("");
    try {
      const created = await apiSend<Asset>("POST", "/api/research/assets", {
        workspace_id: "default",
        asset_type: assetType,
        canonical_value: canonical.trim(),
        display_name: displayName.trim(),
        description: description.trim(),
      });
      setSelectedId(created.asset_id);
      setShowCreate(false);
      setCanonical("");
      setDisplayName("");
      setDescription("");
      list.retry();
    } catch (error) {
      setWriteError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const addEndpoint = async (event: FormEvent) => {
    event.preventDefault();
    if (!selectedId || !endpointHost.trim()) return;
    setSaving(true);
    setWriteError("");
    try {
      await apiSend("POST", `/api/research/assets/${encodeURIComponent(selectedId)}/endpoints`, {
        scheme: endpointScheme,
        host: endpointHost.trim(),
        port: endpointPort.trim() ? Number(endpointPort) : null,
        protocol: endpointScheme || "unknown",
        path: "/",
      });
      setEndpointHost("");
      detail.retry();
    } catch (error) {
      setWriteError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const addObservation = async (event: FormEvent) => {
    event.preventDefault();
    if (!selectedId || !observationNote.trim()) return;
    setSaving(true);
    setWriteError("");
    try {
      const created = await apiSend<Observation>(
        "POST",
        `/api/research/assets/${encodeURIComponent(selectedId)}/observations`,
        {
          observation_type: "other",
          source_type: "manual",
          source_ref: "research-assets-ui",
          evidence_ref: observationEvidence.trim(),
          payload: { note: observationNote.trim() },
        },
      );
      setObservationNote("");
      setTechnologyObservation(created.observation_id);
      detail.retry();
    } catch (error) {
      setWriteError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const addTechnology = async (event: FormEvent) => {
    event.preventDefault();
    if (
      !selectedId ||
      !technologyName.trim() ||
      !technologyObservation ||
      !technologyEvidence.trim()
    )
      return;
    setSaving(true);
    setWriteError("");
    try {
      await apiSend("POST", `/api/research/assets/${encodeURIComponent(selectedId)}/technology`, {
        technology: technologyName.trim(),
        observation_id: technologyObservation,
        evidence_ref: technologyEvidence.trim(),
        confidence: 0.8,
      });
      setTechnologyName("");
      setTechnologyEvidence("");
      detail.retry();
    } catch (error) {
      setWriteError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  if (list.loading && !list.data)
    return (
      <div>
        <PageHeader title={translateUi("资产")} />
        <SkeletonPanel lines={6} />
      </div>
    );
  if (list.error && !list.data)
    return (
      <div>
        <PageHeader title={translateUi("资产")} />
        <ErrorPanel error={list.error} onRetry={list.retry} />
      </div>
    );

  const assets = list.data?.items ?? [];
  const selected = detail.data?.asset;

  return (
    <div className="space-y-4">
      <PageHeader
        title={translateUi("研究资产")}
        actions={
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={() => setShowCreate((v) => !v)}
          >
            <Plus className="h-4 w-4" />
            {translateUi("导入资产")}
          </button>
        }
      />
      <p className="-mt-2 text-sm text-muted">
        {translateUi("独立的研究目标身份、端点与证据来源记录。")}
      </p>

      {showCreate ? (
        <form
          onSubmit={createAsset}
          className="rounded-surface border border-edge bg-base-100 p-4 md:p-5"
        >
          <div className="mb-4 flex items-center justify-between gap-3">
            <h2 className="font-semibold">{translateUi("手动导入研究资产")}</h2>
            <button
              type="button"
              className="btn btn-ghost btn-xs"
              onClick={() => setShowCreate(false)}
            >
              {translateUi("取消")}
            </button>
          </div>
          <div className="grid gap-3 md:grid-cols-2">
            <label className="form-control gap-1">
              <span className="label-text">{translateUi("资产类型")}</span>
              <select
                className="select select-bordered select-sm"
                value={assetType}
                onChange={(e) => setAssetType(e.target.value)}
              >
                {Object.entries(assetTypeLabels).map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
            <label className="form-control gap-1">
              <span className="label-text">{translateUi("规范化目标值")}</span>
              <input
                className="input input-bordered input-sm"
                required
                maxLength={2048}
                value={canonical}
                onChange={(e) => setCanonical(e.target.value)}
                placeholder="example.org / 192.0.2.10 / https://example.org"
              />
            </label>
            <label className="form-control gap-1 md:col-span-2">
              <span className="label-text">{translateUi("显示名称")}</span>
              <input
                className="input input-bordered input-sm"
                maxLength={200}
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
              />
            </label>
            <label className="form-control gap-1 md:col-span-2">
              <span className="label-text">{translateUi("描述")}</span>
              <textarea
                className="textarea textarea-bordered min-h-16"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
              />
            </label>
          </div>
          <p className="mt-3 text-xs text-muted">
            {translateUi("手动导入会自动记录 user_import 来源 Observation；不会自动推断技术栈。")}
          </p>
          {writeError ? (
            <p role="alert" className="mt-2 text-sm text-error">
              {writeError}
            </p>
          ) : null}
          <div className="mt-4 flex justify-end">
            <button
              className="btn btn-primary btn-sm"
              type="submit"
              disabled={saving || !canonical.trim()}
            >
              {saving ? translateUi("导入中…") : translateUi("导入资产")}
            </button>
          </div>
        </form>
      ) : null}

      <div className="grid gap-4 xl:grid-cols-[minmax(0,1.6fr)_minmax(300px,0.9fr)]">
        <section className="min-w-0 rounded-surface border border-edge bg-base-100">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-edge p-4">
            <div>
              <h2 className="font-semibold">{translateUi("资产清单")}</h2>
              <p className="mt-1 text-xs text-muted">{translateUi("规范化身份 · 来源可追溯")}</p>
            </div>
            <span className="badge badge-ghost">
              {list.data?.total ?? assets.length} {translateUi("项资产")}
            </span>
          </div>
          <div className="flex flex-wrap gap-2 p-3">
            <label className="input input-bordered input-sm flex min-w-0 flex-1 items-center gap-2">
              <Search className="h-4 w-4 text-muted" />
              <input
                className="grow"
                aria-label={translateUi("搜索资产")}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={translateUi("搜索目标或显示名称")}
              />
            </label>
            <select
              className="select select-bordered select-sm"
              aria-label={translateUi("资产类型筛选")}
              value={kind}
              onChange={(e) => setKind(e.target.value)}
            >
              <option value="all">{translateUi("全部类型")}</option>
              {Object.entries(assetTypeLabels).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              aria-label={translateUi("刷新")}
              onClick={list.retry}
            >
              <RefreshCw className="h-4 w-4" />
            </button>
          </div>
          {assets.length === 0 ? (
            <div className="p-4">
              <EmptyPanel
                text={translateUi("暂无研究资产。可手动导入研究目标，或等待发现流程产生真实记录。")}
                actionLabel={translateUi("导入资产")}
                onAction={() => setShowCreate(true)}
                icon={<Server className="h-6 w-6" />}
              />
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="table table-sm">
                <thead>
                  <tr>
                    <th>{translateUi("资产")}</th>
                    <th>{translateUi("类型")}</th>
                    <th>{translateUi("状态")}</th>
                    <th>{translateUi("最近观测")}</th>
                  </tr>
                </thead>
                <tbody>
                  {assets.map((asset) => (
                    <tr
                      key={asset.asset_id}
                      className={selectedId === asset.asset_id ? "bg-primary/5" : ""}
                    >
                      <td>
                        <button
                          type="button"
                          className="text-left"
                          onClick={() => {
                            setSelectedId(asset.asset_id);
                            setWriteError("");
                          }}
                        >
                          <span className="block font-medium hover:text-primary">
                            {asset.display_name || asset.canonical_value}
                          </span>
                          <span className="text-xs text-muted">{asset.canonical_value}</span>
                        </button>
                      </td>
                      <td className="text-sm">
                        {assetTypeLabels[asset.asset_type] ?? asset.asset_type}
                      </td>
                      <td>
                        <StatusBadge
                          status={asset.lifecycle_status}
                          labels={{
                            active: translateUi("活跃"),
                            inactive: translateUi("未活跃"),
                            archived: translateUi("已归档"),
                          }}
                          tones={{
                            active: "badge-success",
                            inactive: "badge-ghost",
                            archived: "badge-ghost",
                          }}
                        />
                      </td>
                      <td className="whitespace-nowrap text-xs text-muted">
                        {new Date(asset.last_seen_at).toLocaleDateString()}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>

        <aside className="min-w-0 rounded-surface border border-edge bg-base-100">
          <div className="border-b border-edge p-4">
            <h2 className="font-semibold">{translateUi("资产详情")}</h2>
            <p className="mt-1 text-xs text-muted">{translateUi("身份、观察记录与证据引用")}</p>
          </div>
          {!selectedId ? (
            <div className="p-4">
              <EmptyPanel
                text={translateUi("选择一个资产查看其观察记录。")}
                icon={<Fingerprint className="h-6 w-6" />}
              />
            </div>
          ) : detail.loading && !selected ? (
            <div className="p-4">
              <SkeletonPanel lines={4} />
            </div>
          ) : detail.error ? (
            <div className="p-4">
              <ErrorPanel error={detail.error} onRetry={detail.retry} />
            </div>
          ) : selected ? (
            <div className="space-y-4 p-4">
              <div>
                <h3 className="break-all text-lg font-semibold">
                  {selected.display_name || selected.canonical_value}
                </h3>
                <p className="mt-1 break-all text-xs text-muted">{selected.canonical_value}</p>
                <div className="mt-2 flex flex-wrap gap-2">
                  <span className="badge badge-ghost">
                    {assetTypeLabels[selected.asset_type] ?? selected.asset_type}
                  </span>
                  <StatusBadge status={selected.lifecycle_status} />
                </div>
              </div>
              <div>
                <h4 className="text-xs font-semibold text-muted">{translateUi("描述")}</h4>
                <p className="mt-1 text-sm">{selected.description || translateUi("暂无描述")}</p>
              </div>
              <div className="grid grid-cols-2 gap-3 rounded-control bg-base-200/60 p-3 text-sm">
                <div>
                  <span className="block text-xs text-muted">{translateUi("首次发现")}</span>
                  {new Date(selected.first_seen_at).toLocaleString()}
                </div>
                <div>
                  <span className="block text-xs text-muted">{translateUi("最近观测")}</span>
                  {new Date(selected.last_seen_at).toLocaleString()}
                </div>
              </div>
              <section className="space-y-2">
                <div className="flex items-center justify-between">
                  <h4 className="text-sm font-semibold">{translateUi("端点")}</h4>
                  <span className="badge badge-ghost">{detail.data?.endpoints.length ?? 0}</span>
                </div>
                <form
                  onSubmit={addEndpoint}
                  className="grid grid-cols-[minmax(0,1fr)_auto_20_auto] gap-2"
                >
                  <input
                    className="input input-bordered input-sm min-w-0"
                    required
                    value={endpointHost}
                    onChange={(e) => setEndpointHost(e.target.value)}
                    placeholder={translateUi("端点主机名")}
                    aria-label={translateUi("端点主机名")}
                  />
                  <select
                    className="select select-bordered select-sm"
                    value={endpointScheme}
                    onChange={(e) => setEndpointScheme(e.target.value)}
                    aria-label={translateUi("协议")}
                  >
                    <option value="https">HTTPS</option>
                    <option value="http">HTTP</option>
                    <option value="tcp">TCP</option>
                  </select>
                  <input
                    className="input input-bordered input-sm w-20"
                    type="number"
                    min="1"
                    max="65535"
                    value={endpointPort}
                    onChange={(e) => setEndpointPort(e.target.value)}
                    aria-label={translateUi("端口")}
                  />
                  <button
                    type="submit"
                    className="btn btn-outline btn-sm"
                    disabled={saving || !endpointHost.trim()}
                  >
                    {translateUi("添加")}
                  </button>
                </form>
                {(detail.data?.endpoints ?? []).map((endpoint) => (
                  <div
                    key={endpoint.endpoint_id}
                    className="rounded-control border border-edge p-2 text-sm"
                  >
                    <span className="font-medium">
                      {endpoint.scheme ? endpoint.scheme + "://" : ""}
                      {endpoint.host}
                      {endpoint.port ? ":" + endpoint.port : ""}
                      {endpoint.path}
                    </span>
                    <p className="text-xs text-muted">
                      {endpoint.protocol} · {endpoint.endpoint_id}
                    </p>
                  </div>
                ))}
              </section>
              <section>
                <div className="mb-2 flex items-center justify-between">
                  <h4 className="text-sm font-semibold">{translateUi("观察记录")}</h4>
                  <span className="badge badge-ghost">{detail.data?.observations.length ?? 0}</span>
                </div>
                {(detail.data?.observations ?? []).length === 0 ? (
                  <p className="text-sm text-muted">{translateUi("暂无观察记录")}</p>
                ) : (
                  <div className="space-y-2">
                    {detail.data?.observations.map((observation) => (
                      <article
                        key={observation.observation_id}
                        className="rounded-control border border-edge p-3"
                      >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <span className="text-sm font-medium">
                            {observation.observation_type}
                          </span>
                          <span className="badge badge-success badge-sm">
                            {observation.source_type}
                          </span>
                        </div>
                        <p className="mt-1 text-xs text-muted">
                          {new Date(observation.observed_at).toLocaleString()}
                        </p>
                        <p className="mt-2 break-all font-mono text-[11px] text-muted">
                          SHA-256 {observation.content_hash}
                        </p>
                        {observation.evidence_ref ? (
                          <p className="mt-1 break-all text-xs">
                            {translateUi("证据引用")}：{observation.evidence_ref}
                          </p>
                        ) : null}
                      </article>
                    ))}
                  </div>
                )}
                <form
                  onSubmit={addObservation}
                  className="space-y-2 rounded-control border border-edge p-3"
                >
                  <h5 className="text-xs font-semibold">{translateUi("追加观察记录")}</h5>
                  <textarea
                    className="textarea textarea-bordered textarea-sm min-h-16 w-full"
                    required
                    value={observationNote}
                    onChange={(e) => setObservationNote(e.target.value)}
                    placeholder={translateUi("记录可核验的观察内容")}
                  />
                  <input
                    className="input input-bordered input-sm w-full"
                    value={observationEvidence}
                    onChange={(e) => setObservationEvidence(e.target.value)}
                    placeholder={translateUi("证据引用（可选）")}
                  />
                  <div className="flex justify-end">
                    <button
                      type="submit"
                      className="btn btn-outline btn-sm"
                      disabled={saving || !observationNote.trim()}
                    >
                      {translateUi("追加记录")}
                    </button>
                  </div>
                </form>
              </section>
              <section className="space-y-2">
                <div className="flex items-center justify-between">
                  <h4 className="text-sm font-semibold">{translateUi("技术画像")}</h4>
                  <span className="badge badge-ghost">{detail.data?.technologies.length ?? 0}</span>
                </div>
                {(detail.data?.technologies ?? []).map((technology) => (
                  <article
                    key={technology.technology_id}
                    className="rounded-control border border-edge p-3"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-medium">
                        {technology.technology} {technology.version}
                      </span>
                      <span className="badge badge-success badge-sm">
                        {Math.round(technology.confidence * 100)}%
                      </span>
                    </div>
                    <p className="mt-1 break-all text-xs text-muted">
                      {technology.evidence_ref} · {technology.observation_id}
                    </p>
                  </article>
                ))}
                <form
                  onSubmit={addTechnology}
                  className="space-y-2 rounded-control border border-edge p-3"
                >
                  <h5 className="text-xs font-semibold">{translateUi("登记技术画像")}</h5>
                  <input
                    className="input input-bordered input-sm w-full"
                    required
                    value={technologyName}
                    onChange={(e) => setTechnologyName(e.target.value)}
                    placeholder={translateUi("技术名称")}
                  />
                  <select
                    className="select select-bordered select-sm w-full"
                    required
                    value={technologyObservation}
                    onChange={(e) => setTechnologyObservation(e.target.value)}
                  >
                    <option value="">{translateUi("选择来源 Observation")}</option>
                    {(detail.data?.observations ?? []).map((observation) => (
                      <option key={observation.observation_id} value={observation.observation_id}>
                        {observation.observation_type} · {observation.observation_id}
                      </option>
                    ))}
                  </select>
                  <input
                    className="input input-bordered input-sm w-full"
                    required
                    value={technologyEvidence}
                    onChange={(e) => setTechnologyEvidence(e.target.value)}
                    placeholder={translateUi("必填证据引用")}
                  />
                  <div className="flex justify-end">
                    <button
                      type="submit"
                      className="btn btn-outline btn-sm"
                      disabled={
                        saving ||
                        !technologyName.trim() ||
                        !technologyObservation ||
                        !technologyEvidence.trim()
                      }
                    >
                      {translateUi("登记画像")}
                    </button>
                  </div>
                </form>
              </section>
              {writeError ? (
                <p role="alert" className="text-xs text-error">
                  {writeError}
                </p>
              ) : null}
              <p className="flex items-start gap-2 text-xs text-muted">
                <Activity className="mt-0.5 h-4 w-4 shrink-0" />
                {translateUi(
                  "技术画像只允许引用可追溯的 Observation 与证据，不会根据域名字符串猜测技术栈。",
                )}
              </p>
            </div>
          ) : null}
        </aside>
      </div>
    </div>
  );
}
