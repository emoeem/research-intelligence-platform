import { useState, type FormEvent } from "react";
import { FolderKanban, Plus, Search, ArrowUpRight, Link2, RefreshCw } from "lucide-react";
import { apiSend } from "../lib/api";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel, StatusBadge } from "../components/ui";
import { translateUi } from "../i18n";

type Project = {
  project_id: string;
  workspace_id: string;
  name: string;
  slug: string;
  summary: string;
  objective: string;
  status: "draft" | "active" | "paused" | "completed" | "archived";
  priority: "low" | "normal" | "high" | "critical";
  scope: Record<string, unknown>;
  tags: string[];
  owner_id: string;
  created_at: string;
  updated_at: string;
};
type ProjectList = { items: Project[]; total: number; available: boolean };
type ProjectDetail = {
  project: Project;
  tasks: { task_id: string; title?: string | null; state: string; relation_role: string }[];
  assets: { asset_id: string; canonical_value: string; display_name: string; asset_type: string }[];
};
const statusTones: Record<string, string> = {
  draft: "badge-ghost",
  active: "badge-success",
  paused: "badge-warning",
  completed: "badge-info",
  archived: "badge-ghost",
};

export function ProjectsPage() {
  const statusLabels: Record<string, string> = {
    draft: translateUi("草稿"),
    active: translateUi("进行中"),
    paused: translateUi("已暂停"),
    completed: translateUi("已完成"),
    archived: translateUi("已归档"),
  };
  const priorityLabels: Record<string, string> = {
    low: translateUi("低"),
    normal: translateUi("普通"),
    high: translateUi("高"),
    critical: translateUi("紧急"),
  };
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("all");
  const [showCreate, setShowCreate] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [objective, setObjective] = useState("");
  const [summary, setSummary] = useState("");
  const [priority, setPriority] = useState("normal");
  const [taskId, setTaskId] = useState("");
  const [assetId, setAssetId] = useState("");
  const [writeError, setWriteError] = useState("");
  const [saving, setSaving] = useState(false);
  const listPath = `/api/projects?workspace_id=default&limit=100${status === "all" ? "" : `&status=${encodeURIComponent(status)}`}${query.trim() ? `&q=${encodeURIComponent(query.trim())}` : ""}`;
  const list = useApi<ProjectList>(listPath);
  const detail = useApi<ProjectDetail>(
    selectedId ? `/api/projects/${encodeURIComponent(selectedId)}?workspace_id=default` : null,
  );

  const submitProject = async (event: FormEvent) => {
    event.preventDefault();
    setSaving(true);
    setWriteError("");
    try {
      const created = await apiSend<Project>("POST", "/api/projects", {
        workspace_id: "default",
        name: name.trim(),
        slug: slug.trim().toLowerCase().replace(/\s+/g, "-"),
        objective: objective.trim(),
        summary: summary.trim(),
        priority,
      });
      setSelectedId(created.project_id);
      setShowCreate(false);
      setName("");
      setSlug("");
      setObjective("");
      setSummary("");
      setPriority("normal");
      list.retry();
    } catch (error) {
      setWriteError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const linkTask = async (event: FormEvent) => {
    event.preventDefault();
    if (!selectedId || !taskId.trim()) return;
    setSaving(true);
    setWriteError("");
    try {
      await apiSend("POST", `/api/projects/${encodeURIComponent(selectedId)}/tasks`, {
        task_id: taskId.trim(),
        role: "research",
      });
      setTaskId("");
      detail.retry();
      setWriteError(translateUi("任务关联已提交"));
    } catch (error) {
      setWriteError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const linkAsset = async (event: FormEvent) => {
    event.preventDefault();
    if (!selectedId || !assetId.trim()) return;
    setSaving(true);
    setWriteError("");
    try {
      await apiSend("POST", `/api/projects/${encodeURIComponent(selectedId)}/assets`, {
        asset_id: assetId.trim(),
        role: "in_scope",
      });
      setAssetId("");
      detail.retry();
      setWriteError(translateUi("资产关联已提交"));
    } catch (error) {
      setWriteError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  if (list.loading && !list.data)
    return (
      <div>
        <PageHeader title={translateUi("项目")} />
        <SkeletonPanel lines={6} />
      </div>
    );
  if (list.error && !list.data)
    return (
      <div>
        <PageHeader title={translateUi("项目")} />
        <ErrorPanel error={list.error} onRetry={list.retry} />
      </div>
    );

  const projects = list.data?.items ?? [];
  const selected = detail.data?.project;

  return (
    <div className="space-y-4">
      <PageHeader
        title={translateUi("项目")}
        actions={
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={() => setShowCreate((v) => !v)}
          >
            <Plus className="h-4 w-4" aria-hidden="true" /> {translateUi("新建项目")}
          </button>
        }
      />
      <p className="-mt-2 text-sm text-muted">
        {translateUi("独立的研究项目空间，用于组织目标、任务和研究资产。")}
      </p>

      {showCreate ? (
        <form
          onSubmit={submitProject}
          className="rounded-surface border border-edge bg-base-100 p-4 md:p-5"
        >
          <div className="mb-4 flex items-center justify-between gap-3">
            <h2 className="font-semibold">{translateUi("创建研究项目")}</h2>
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
              <span className="label-text">{translateUi("项目名称")}</span>
              <input
                className="input input-bordered input-sm w-full"
                required
                maxLength={160}
                value={name}
                onChange={(e) => {
                  setName(e.target.value);
                  if (!slug)
                    setSlug(
                      e.target.value
                        .toLowerCase()
                        .trim()
                        .replace(/[^a-z0-9]+/g, "-")
                        .replace(/^-|-$/g, ""),
                    );
                }}
                placeholder={translateUi("例如：Linux 内核攻击面审查")}
              />
            </label>
            <label className="form-control gap-1">
              <span className="label-text">{translateUi("唯一标识")}</span>
              <input
                className="input input-bordered input-sm w-full"
                required
                pattern="[a-z0-9-]+"
                maxLength={180}
                value={slug}
                onChange={(e) => setSlug(e.target.value)}
                placeholder="linux-kernel-review"
              />
            </label>
            <label className="form-control gap-1">
              <span className="label-text">{translateUi("优先级")}</span>
              <select
                className="select select-bordered select-sm w-full"
                value={priority}
                onChange={(e) => setPriority(e.target.value)}
              >
                <option value="low">{translateUi("低")}</option>
                <option value="normal">{translateUi("普通")}</option>
                <option value="high">{translateUi("高")}</option>
                <option value="critical">{translateUi("紧急")}</option>
              </select>
            </label>
            <label className="form-control gap-1 md:col-span-2">
              <span className="label-text">{translateUi("研究目标")}</span>
              <textarea
                className="textarea textarea-bordered min-h-20"
                value={objective}
                onChange={(e) => setObjective(e.target.value)}
                placeholder={translateUi("描述项目需要回答的核心研究问题")}
              />
            </label>
            <label className="form-control gap-1 md:col-span-2">
              <span className="label-text">{translateUi("项目摘要")}</span>
              <textarea
                className="textarea textarea-bordered min-h-16"
                value={summary}
                onChange={(e) => setSummary(e.target.value)}
              />
            </label>
          </div>
          {writeError ? (
            <p role="alert" className="mt-3 text-sm text-error">
              {writeError}
            </p>
          ) : null}
          <div className="mt-4 flex justify-end">
            <button
              className="btn btn-primary btn-sm"
              type="submit"
              disabled={saving || !name.trim() || !slug.trim()}
            >
              {saving ? translateUi("保存中…") : translateUi("创建项目")}
            </button>
          </div>
        </form>
      ) : null}

      <div className="grid gap-4 xl:grid-cols-[minmax(0,1.6fr)_minmax(300px,0.9fr)]">
        <section className="min-w-0 rounded-surface border border-edge bg-base-100">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-edge p-4">
            <div>
              <h2 className="font-semibold">{translateUi("项目目录")}</h2>
              <p className="mt-1 text-xs text-muted">
                {translateUi("独立项目实体 · 工作区")} default
              </p>
            </div>
            <span className="badge badge-ghost">
              {list.data?.total ?? projects.length} {translateUi("个项目")}
            </span>
          </div>
          <div className="flex flex-wrap gap-2 p-3">
            <label className="input input-bordered input-sm flex min-w-0 flex-1 items-center gap-2">
              <Search className="h-4 w-4 text-muted" aria-hidden="true" />
              <input
                aria-label={translateUi("搜索项目")}
                className="grow"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={translateUi("搜索名称或标识")}
              />
            </label>
            <select
              className="select select-bordered select-sm"
              aria-label={translateUi("项目状态")}
              value={status}
              onChange={(e) => setStatus(e.target.value)}
            >
              <option value="all">{translateUi("全部状态")}</option>
              <option value="draft">{statusLabels.draft}</option>
              <option value="active">{statusLabels.active}</option>
              <option value="paused">{statusLabels.paused}</option>
              <option value="completed">{statusLabels.completed}</option>
              <option value="archived">{statusLabels.archived}</option>
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
          {projects.length === 0 ? (
            <div className="p-4">
              <EmptyPanel
                text={translateUi("还没有研究项目。创建一个项目以组织任务、资产和研究目标。")}
                actionLabel={translateUi("新建项目")}
                onAction={() => setShowCreate(true)}
                icon={<FolderKanban className="h-6 w-6" />}
              />
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="table table-sm">
                <thead>
                  <tr>
                    <th>{translateUi("项目")}</th>
                    <th>{translateUi("状态")}</th>
                    <th>{translateUi("优先级")}</th>
                    <th>{translateUi("最近更新")}</th>
                  </tr>
                </thead>
                <tbody>
                  {projects.map((project) => (
                    <tr
                      key={project.project_id}
                      className={selectedId === project.project_id ? "bg-primary/5" : ""}
                    >
                      <td>
                        <button
                          type="button"
                          className="text-left"
                          onClick={() => {
                            setSelectedId(project.project_id);
                            setWriteError("");
                          }}
                        >
                          <span className="block font-medium hover:text-primary">
                            {project.name}
                          </span>
                          <span className="text-xs text-muted">{project.slug}</span>
                        </button>
                      </td>
                      <td>
                        <StatusBadge
                          status={project.status}
                          labels={statusLabels}
                          tones={statusTones}
                        />
                      </td>
                      <td className="text-sm">
                        {priorityLabels[project.priority] ?? project.priority}
                      </td>
                      <td className="whitespace-nowrap text-xs text-muted">
                        {new Date(project.updated_at).toLocaleDateString()}
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
            <h2 className="font-semibold">{translateUi("项目详情")}</h2>
            <p className="mt-1 text-xs text-muted">{translateUi("目标、范围与关联关系")}</p>
          </div>
          {!selectedId ? (
            <div className="p-4">
              <EmptyPanel
                text={translateUi("选择一个项目查看详情。")}
                icon={<FolderKanban className="h-6 w-6" />}
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
                <div className="flex flex-wrap items-center gap-2">
                  <h3 className="text-lg font-semibold">{selected.name}</h3>
                  <StatusBadge status={selected.status} labels={statusLabels} tones={statusTones} />
                </div>
                <p className="mt-1 text-xs text-muted">
                  {selected.project_id} · {selected.slug}
                </p>
              </div>
              <div>
                <h4 className="text-xs font-semibold text-muted">{translateUi("研究目标")}</h4>
                <p className="mt-1 text-sm">
                  {selected.objective || translateUi("尚未填写研究目标")}
                </p>
              </div>
              <div>
                <h4 className="text-xs font-semibold text-muted">{translateUi("项目摘要")}</h4>
                <p className="mt-1 text-sm">{selected.summary || translateUi("暂无摘要")}</p>
              </div>
              <div className="grid grid-cols-2 gap-3 rounded-control bg-base-200/60 p-3 text-sm">
                <div>
                  <span className="block text-xs text-muted">{translateUi("优先级")}</span>
                  {priorityLabels[selected.priority] ?? selected.priority}
                </div>
                <div>
                  <span className="block text-xs text-muted">{translateUi("创建时间")}</span>
                  {new Date(selected.created_at).toLocaleDateString()}
                </div>
              </div>
              <div>
                <h4 className="mb-2 text-xs font-semibold text-muted">
                  {translateUi("关联研究任务")} · {detail.data?.tasks.length ?? 0}
                </h4>
                <form onSubmit={linkTask} className="flex gap-2">
                  <input
                    className="input input-bordered input-sm min-w-0 flex-1"
                    value={taskId}
                    onChange={(e) => setTaskId(e.target.value)}
                    placeholder={translateUi("输入 RTIS task_id")}
                    aria-label={translateUi("RTIS task ID")}
                  />
                  <button
                    type="submit"
                    className="btn btn-outline btn-sm"
                    disabled={saving || !taskId.trim()}
                  >
                    <Link2 className="h-4 w-4" />
                    {translateUi("关联")}
                  </button>
                </form>
                <p className="mt-1 text-xs text-muted">
                  {translateUi("关联不会修改 RTIS 任务本身。")}
                </p>
                {(detail.data?.tasks ?? []).map((task) => (
                  <div key={task.task_id} className="mt-2 rounded-control border border-edge p-2">
                    <span className="block text-sm font-medium">{task.title || task.task_id}</span>
                    <span className="text-xs text-muted">
                      {task.state} · {task.relation_role}
                    </span>
                  </div>
                ))}
              </div>
              <div>
                <h4 className="mb-2 text-xs font-semibold text-muted">
                  {translateUi("关联研究资产")} · {detail.data?.assets.length ?? 0}
                </h4>
                <form onSubmit={linkAsset} className="flex gap-2">
                  <input
                    className="input input-bordered input-sm min-w-0 flex-1"
                    value={assetId}
                    onChange={(e) => setAssetId(e.target.value)}
                    placeholder={translateUi("输入 asset_id")}
                    aria-label={translateUi("Asset ID")}
                  />
                  <button
                    type="submit"
                    className="btn btn-outline btn-sm"
                    disabled={saving || !assetId.trim()}
                  >
                    <Link2 className="h-4 w-4" />
                    {translateUi("关联")}
                  </button>
                </form>
                {(detail.data?.assets ?? []).map((asset) => (
                  <div key={asset.asset_id} className="mt-2 rounded-control border border-edge p-2">
                    <span className="block text-sm font-medium">
                      {asset.display_name || asset.canonical_value}
                    </span>
                    <span className="break-all text-xs text-muted">{asset.asset_id}</span>
                  </div>
                ))}
              </div>
              {writeError ? (
                <p role="status" className="text-xs text-muted">
                  {writeError}
                </p>
              ) : null}
              <a
                className="inline-flex items-center gap-1 text-sm text-primary hover:underline"
                href="/research/tasks"
              >
                <ArrowUpRight className="h-4 w-4" />
                {translateUi("查看研究任务")}
              </a>
            </div>
          ) : null}
        </aside>
      </div>
    </div>
  );
}
