import { useState, type FormEvent } from "react";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { formatDateTime } from "../lib/format";
import {
  apiSend,
  fetchDocumentDetail,
  ApiError,
  type DocumentDetail,
  type DocumentList,
  type DocumentSummary,
  type DocumentRevisionRow,
} from "../lib/api";

/** 文档中心（RIP3 P1-2/P1-5，R2 §12 Knowledge Center Documents 页）：
 *  1. 上传表单（base64 → POST /api/documents/import，rp-doc 子进程：
 *     magic bytes + CAS + 单事务落对象/来源/文档/版本）；
 *  2. 文档列表（workspace 过滤；权限前置谓词在 PG SQL 内）；
 *  3. 版本历史（点击行取 /api/documents/<id>，SUPERSEDED 历史可访问）。
 *  内容不重复存储：同内容跨通道导入时 content_object_created=false 并如实标注。 */

const VISIBILITY_LABEL: Record<string, string> = {
  private: "私有",
  shared: "共享",
  public: "公开",
};

function DocumentRow({ doc, onOpen }: { doc: DocumentSummary; onOpen: (id: string) => void }) {
  return (
    <tr
      data-testid={`doc-card-${doc.document_id}`}
      className="hover:bg-base-200/60 cursor-pointer"
      onClick={() => onOpen(doc.document_id)}
    >
      <td className="font-mono text-xs" data-testid="doc-id">
        {doc.document_id}
      </td>
      <td className="max-w-48 truncate" title={doc.title}>
        {doc.title}
      </td>
      <td>
        <span className="badge badge-ghost badge-sm">{doc.document_type}</span>
      </td>
      <td className="text-xs">{(doc.file_size / 1024).toFixed(1)} KiB</td>
      <td>
        <span className="badge badge-sm badge-ghost">
          {VISIBILITY_LABEL[doc.visibility] ?? doc.visibility}
        </span>
      </td>
      <td className="text-xs" data-testid="doc-source-count">
        {doc.source_count}
      </td>
      <td className="text-xs">{doc.revision_count}</td>
      <td className="text-xs">{formatDateTime(doc.created_at)}</td>
    </tr>
  );
}

function RevisionsPanel({ detail }: { detail: DocumentDetail }) {
  return (
    <section
      className="rounded-surface border border-edge bg-base-100 p-5"
      data-testid="doc-revisions"
    >
      <h3 className="mb-3 text-sm font-semibold">版本历史：{detail.document_id}</h3>
      <table className="table table-sm">
        <thead>
          <tr>
            <th>版本</th>
            <th>状态</th>
            <th>content_hash</th>
            <th>解析器</th>
            <th>创建时间</th>
          </tr>
        </thead>
        <tbody>
          {detail.revisions.map((r: DocumentRevisionRow) => (
            <tr key={r.revision_id}>
              <td className="font-mono text-xs">{r.revision_number}</td>
              <td>
                <span
                  className={`badge badge-sm ${r.revision_status === "CURRENT" ? "badge-success" : "badge-ghost"}`}
                >
                  {r.revision_status}
                </span>
              </td>
              <td className="font-mono text-[10px] max-w-40 truncate" title={r.content_hash}>
                {r.content_hash.slice(0, 16)}…
              </td>
              <td className="text-xs">{r.parser_version ?? "未解析"}</td>
              <td className="text-xs">{formatDateTime(r.created_at)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

export function DocumentsPage() {
  const [workspace, setWorkspace] = useState("default");
  const listState = useApi<DocumentList>(
    `/api/documents?workspace=${encodeURIComponent(workspace)}`,
  );
  const { retry: refetchList } = listState;

  // 上传表单状态
  const [title, setTitle] = useState("");
  const [visibility, setVisibility] = useState("private");
  const [sourceUrl, setSourceUrl] = useState("");
  const [file, setFile] = useState<File | null>(null);
  const [uploading, setUploading] = useState(false);
  const [uploadResult, setUploadResult] = useState<string | null>(null);
  const [uploadError, setUploadError] = useState<string | null>(null);

  // 版本历史面板
  const [openDoc, setOpenDoc] = useState<string | null>(null);
  const [detail, setDetail] = useState<DocumentDetail | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);

  async function refresh() {
    await refetchList();
  }

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setUploadError(null);
    setUploadResult(null);
    if (!file) {
      setUploadError("请选择文件");
      return;
    }
    setUploading(true);
    try {
      const buffer = await file.arrayBuffer();
      // 分块编码避免大文件 call stack 爆栈（String.fromCharCode 摊平限制）。
      const bytes = new Uint8Array(buffer);
      let binary = "";
      const chunk = 0x8000;
      for (let i = 0; i < bytes.length; i += chunk) {
        binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
      }
      const contentBase64 = btoa(binary);
      const outcome = await apiSend<{
        document_id: string;
        content_object_created: boolean;
        content_hash: string;
      }>("POST", "/api/documents/import", {
        title,
        filename: file.name,
        content_base64: contentBase64,
        workspace,
        visibility,
        source_type: "upload",
        source_url: sourceUrl,
      });
      setUploadResult(
        `${outcome.document_id} 已入库（对象${outcome.content_object_created ? "新建" : "复用，内容不重复存储"}；sha256 ${outcome.content_hash.slice(0, 16)}…）`,
      );
      setTitle("");
      setSourceUrl("");
      setFile(null);
      await refresh();
    } catch (error) {
      setUploadError(error instanceof ApiError ? error.message : String(error));
    } finally {
      setUploading(false);
    }
  }

  async function openRevisions(id: string) {
    setOpenDoc(id);
    setDetail(null);
    setDetailError(null);
    try {
      setDetail(await fetchDocumentDetail(id));
    } catch (error) {
      setDetailError(error instanceof ApiError ? error.message : String(error));
    }
  }

  const { data, error, loading } = listState;
  const items: DocumentSummary[] = data?.items ?? [];

  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="文档中心" />

      <form className="panel" data-testid="doc-upload-form" onSubmit={onSubmit}>
        <h3 className="panel-title">导入文档</h3>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
          <label className="form-control">
            <span className="label-text">文件</span>
            <input
              type="file"
              className="file-input file-input-bordered file-input-sm w-full"
              data-testid="doc-file"
              onChange={(e) => setFile(e.target.files?.[0] ?? null)}
            />
          </label>
          <label className="form-control">
            <span className="label-text">标题</span>
            <input
              className="input input-bordered input-sm"
              data-testid="doc-title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="文档标题"
            />
          </label>
          <label className="form-control">
            <span className="label-text">可见性</span>
            <select
              className="select select-bordered select-sm"
              data-testid="doc-visibility"
              value={visibility}
              onChange={(e) => setVisibility(e.target.value)}
            >
              <option value="private">私有</option>
              <option value="shared">共享</option>
              <option value="public">公开</option>
            </select>
          </label>
          <label className="form-control">
            <span className="label-text">工作区</span>
            <input
              className="input input-bordered input-sm"
              data-testid="doc-workspace"
              value={workspace}
              onChange={(e) => setWorkspace(e.target.value)}
              placeholder="default"
            />
          </label>
          <label className="form-control md:col-span-2">
            <span className="label-text">来源 URL（可空）</span>
            <input
              className="input input-bordered input-sm"
              data-testid="doc-source-url"
              value={sourceUrl}
              onChange={(e) => setSourceUrl(e.target.value)}
              placeholder="https://…"
            />
          </label>
          <div className="flex items-end">
            <button
              type="submit"
              className="btn btn-primary btn-sm"
              data-testid="doc-upload-submit"
              disabled={uploading}
            >
              {uploading ? "导入中…" : "导入"}
            </button>
          </div>
        </div>
        {uploadResult && (
          <p className="mt-2 text-xs text-success" data-testid="doc-upload-result">
            {uploadResult}
          </p>
        )}
        {uploadError && (
          <p className="mt-2 text-xs text-error" data-testid="doc-upload-error">
            {uploadError}
          </p>
        )}
      </form>

      {error && <ErrorPanel error={error} onRetry={refresh} />}
      {loading && <SkeletonPanel lines={4} />}
      {!loading && !error && items.length === 0 && (
        <EmptyPanel text="暂无文档（上传后经 rp-doc 落 PostgreSQL 情报核）" />
      )}
      {!loading && !error && items.length > 0 && (
        <section className="rounded-surface border border-edge bg-base-100 p-5 overflow-x-auto">
          <h3 className="mb-3 text-sm font-semibold">文档列表（workspace: {workspace}）</h3>
          <table className="table table-sm">
            <thead>
              <tr>
                <th>ID</th>
                <th>标题</th>
                <th>类型</th>
                <th>大小</th>
                <th>可见性</th>
                <th>来源数</th>
                <th>版本数</th>
                <th>入库时间</th>
              </tr>
            </thead>
            <tbody>
              {items.map((doc) => (
                <DocumentRow key={doc.document_id} doc={doc} onOpen={openRevisions} />
              ))}
            </tbody>
          </table>
        </section>
      )}

      {openDoc && detailError && (
        <ErrorPanel error={new ApiError(0, detailError)} onRetry={() => openRevisions(openDoc)} />
      )}
      {openDoc && detail && <RevisionsPanel detail={detail} />}
    </div>
  );
}
