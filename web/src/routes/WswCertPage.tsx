import { translateUi } from "../i18n";
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { apiSend, ApiError } from "../lib/api";
import { ErrorPanel, PageHeader } from "../components/ui";
import { useApi } from "../lib/hooks";

/** WSW-2 证书状态页（W2-D8 最小 UI 面；03 审计 §6 页面 11 缺件项）：
 *  CA 指纹/有效期/CN、PEM 导出按钮（显式 confirm）、重新生成按钮（显式
 *  confirm + 浏览器二次确认）、各平台手动信任安装说明。红线：本页零
 *  系统代理/信任库写调用——信任安装永远由用户手动完成。 */

interface WswCaStatus {
  exists?: boolean;
  ca_dir?: string;
  cn?: string;
  not_before?: string;
  not_after?: string;
  fingerprint?: string;
  key_mode?: string;
  trust_install?: string;
  note?: string;
}

interface ExportResult {
  exported?: boolean;
  path?: string;
  fingerprint?: string;
  pem?: string;
  note?: string;
}

interface RegenResult {
  regenerated?: boolean;
  output?: string;
}

function actionError(action: string, error: unknown): string {
  if (error instanceof ApiError) {
    return `失败（HTTP ${error.status}）：${error.message}`;
  }
  return `${action} 失败：${String(error)}`;
}

export function WswCertPage() {
  const status = useApi<WswCaStatus>("/api/wsw/ca-status");
  const queryClient = useQueryClient();
  const [exported, setExported] = useState<ExportResult | null>(null);
  const [regenOutput, setRegenOutput] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function exportPem() {
    setBusy(true);
    setMessage(null);
    try {
      const result = await apiSend<ExportResult>("POST", "/api/wsw/ca/export", { confirm: true });
      setExported(result);
      setMessage(`已导出 PEM（${result.path}）；信任安装为你的显式动作。`);
    } catch (error) {
      setMessage(actionError("导出", error));
    } finally {
      setBusy(false);
    }
  }

  async function regenerate() {
    if (
      !window.confirm(
        translateUi(
          "重新生成根 CA？已信任旧 CA 的客户端（浏览器/curl）将全部失信，需要重新手动导入。",
        ),
      )
    ) {
      return;
    }
    setBusy(true);
    setMessage(null);
    try {
      const result = await apiSend<RegenResult>("POST", "/api/wsw/ca/regenerate", {
        confirm: true,
      });
      setRegenOutput(result.output ?? "CA 已重新生成");
      setExported(null);
      void queryClient.invalidateQueries();
      setMessage(translateUi("CA 已重新生成（旧信任全部失效，需重新手动导入）。"));
    } catch (error) {
      setMessage(actionError("重生成", error));
    } finally {
      setBusy(false);
    }
  }

  const data = status.data ?? {};

  return (
    <main className="page" data-testid="wsw-cert-page">
      <PageHeader
        title={translateUi("WSW 证书状态")}
        actions={
          <>
            <button
              type="button"
              className="btn btn-primary"
              disabled={busy || !data.exists}
              data-testid="wsw-ca-export"
              onClick={() => void exportPem()}
            >
              {translateUi("导出 PEM")}
            </button>
            <button
              type="button"
              className="btn"
              disabled={busy}
              data-testid="wsw-ca-regenerate"
              onClick={() => void regenerate()}
            >
              {translateUi("重新生成 CA")}
            </button>
          </>
        }
      />
      <p className="muted">
        {translateUi(
          "根 CA 私钥 0600 落 rip-data/wsw-ca/（不入 Git、不入日志、不进普通 Artifact）。导出与重生成均为显式用户动作；本工作台不静默安装根证书、不修改系统代理设置。",
        )}
      </p>
      {message && (
        <p className="badge-info" data-testid="wsw-cert-message">
          {message}
        </p>
      )}
      {status.error && <ErrorPanel error={status.error} onRetry={status.retry} />}
      {status.loading && !status.data && <p className="muted">{translateUi("加载中…")}</p>}
      {status.data && !status.data.exists && (
        <p className="muted" data-testid="wsw-ca-missing">
          {translateUi("CA 未生成：首次启动代理时自动生成（或手动执行 regenerate）。")}
        </p>
      )}
      {status.data?.exists && (
        <section className="panel" data-testid="wsw-ca-summary">
          <h2>{translateUi("根 CA")}</h2>
          <dl className="kv">
            <dt>CN</dt>
            <dd>{data.cn ?? "-"}</dd>
            <dt>{translateUi("指纹（SHA-256，证书 DER）")}</dt>
            <dd data-testid="wsw-ca-fingerprint">
              <code>{data.fingerprint ?? "-"}</code>
            </dd>
            <dt>{translateUi("有效期")}</dt>
            <dd>
              {data.not_before ?? "-"} ~ {data.not_after ?? "-"}
            </dd>
            <dt>{translateUi("私钥权限")}</dt>
            <dd>{data.key_mode ?? "-"}（应为 0o600）</dd>
            <dt>{translateUi("存储位")}</dt>
            <dd>{data.ca_dir ?? "-"}</dd>
          </dl>
        </section>
      )}
      {exported?.pem && (
        <section className="panel">
          <h2>
            {translateUi("导出的 CA PEM（已写")} {exported.path}）
          </h2>
          <pre className="code" data-testid="wsw-ca-pem">
            {exported.pem}
          </pre>
        </section>
      )}
      {regenOutput && (
        <section className="panel">
          <h2>{translateUi("重生成输出")}</h2>
          <pre className="code">{regenOutput}</pre>
        </section>
      )}
      <section className="panel">
        <h2>{translateUi("浏览器信任安装说明（手动，各平台）")}</h2>
        <ol>
          <li>
            {translateUi("导出 PEM（上方按钮，文件写 rip-data/wsw-state/exported-ca.pem）。")}
          </li>
          <li>
            {translateUi(
              "Linux（Chromium/Firefox）：设置 → 隐私与安全 → 证书 → 查看证书 → 证书颁发机构 → 导入导出的 ca.cert.pem，勾选「信任此 CA 标识网站」。",
            )}
          </li>
          <li>
            {translateUi(
              "macOS：钥匙串访问 → 系统 → 导入 ca.cert.pem → 双击 → 信任 →「始终信任」（需要管理员确认）。",
            )}
          </li>
          <li>
            {translateUi(
              "Windows：certmgr.msc → 受信任的根证书颁发机构 → 证书 → 导入 ca.cert.pem。",
            )}
          </li>
          <li>
            {translateUi(
              "命令行：curl 使用 --cacert 指向导出的 PEM 即可，无需安装；openssl s_client 用 -CAfile 同理。",
            )}
          </li>
        </ol>
        <p className="muted">{data.trust_install}</p>
      </section>
    </main>
  );
}
