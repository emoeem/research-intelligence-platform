import { useEffect, useState } from "react";
import { Modal } from "./ui";
import { type PluginConfirmFields, type TrustPolicyTier } from "../lib/api";

/** P10-2 信任级分级确认对话框（PLUGIN-CONTRACT §6：信任级驱动安装确认
 *  提示强度；策略档由 plugin-registry::trust_policy 单一来源下发）：
 *  - single（official）：一次点击确认；
 *  - confirm（community）：明确同意一次；
 *  - confirm_with_provenance（local）：展示来源核对信息后确认；
 *  - double_confirm_typed（remote）：输入插件 id 才能继续。
 *  策略不可得时 resolveTrustTier 已 fail-closed 到最强档。 */

export function TrustConfirmDialog({
  open,
  onClose,
  tier,
  pluginId,
  confirmText,
  onConfirm,
  busy,
  contextNote,
}: {
  open: boolean;
  onClose: () => void;
  tier: TrustPolicyTier;
  /** 展示与 remote 输入比对的插件 id（manifest-inspect 预检结果）。 */
  pluginId: string;
  confirmText: string;
  onConfirm: (fields: PluginConfirmFields) => void;
  busy?: boolean;
  /** 来源核对展示（confirm_with_provenance 档要求）。 */
  contextNote?: string;
}) {
  const [typedId, setTypedId] = useState("");
  const [checkedProvenance, setCheckedProvenance] = useState(false);
  useEffect(() => {
    if (!open) {
      setTypedId("");
      setCheckedProvenance(false);
    }
  }, [open]);

  const typedReady = !tier.requires_typed_id || (pluginId.length > 0 && typedId === pluginId);
  const provenanceReady = !tier.requires_provenance || checkedProvenance;
  const ready = typedReady && provenanceReady && !busy;

  const fields: PluginConfirmFields = tier.required_confirm_fields.includes("confirm")
    ? {
        confirm: true,
        ...(tier.required_confirm_fields.includes("confirm_id") ? { confirm_id: pluginId } : {}),
      }
    : {};

  return (
    <Modal open={open} onClose={onClose} title={tier.confirmation_label}>
      <div className="space-y-3 text-sm" data-testid="trust-confirm-dialog">
        <p>
          目标：<span className="font-mono">{pluginId || "(未知插件)"}</span>
          <span className="ml-2 badge badge-sm badge-ghost">{tier.trust_level}</span>
        </p>
        {contextNote ? (
          <p className="text-xs opacity-70 font-mono break-all">{contextNote}</p>
        ) : null}
        {tier.requires_provenance ? (
          <label className="flex items-center gap-2 text-xs" data-testid="confirm-provenance">
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={checkedProvenance}
              onChange={(event) => setCheckedProvenance(event.target.checked)}
            />
            我已核对来源定位与许可信息（本地来源确认强度）
          </label>
        ) : null}
        {tier.requires_typed_id ? (
          <div className="text-xs">
            <label className="mb-1 block opacity-70">
              远程来源风险提示：输入插件 id <span className="font-mono">{pluginId}</span>{" "}
              以完成二次确认
            </label>
            <input
              type="text"
              className="input input-sm input-bordered w-full font-mono"
              value={typedId}
              onChange={(event) => setTypedId(event.target.value)}
              placeholder={pluginId}
              data-testid="confirm-typed-id"
            />
            {typedId.length > 0 && typedId !== pluginId ? (
              <p className="mt-1 text-error" data-testid="confirm-id-mismatch">
                输入与插件 id 不一致
              </p>
            ) : null}
          </div>
        ) : null}
        {!typedReady && typedId.length > 0 ? null : (
          <p className="text-xs text-muted">{tier.confirmation_label}</p>
        )}
      </div>
      <div className="mt-4 flex justify-end gap-2">
        <button
          type="button"
          className="btn btn-ghost btn-sm"
          onClick={onClose}
          disabled={busy}
          data-testid="confirm-cancel"
        >
          取消
        </button>
        <button
          type="button"
          className="btn btn-primary btn-sm"
          onClick={() => onConfirm(fields)}
          disabled={!ready}
          data-testid="confirm-proceed"
        >
          {confirmText}
        </button>
      </div>
    </Modal>
  );
}
