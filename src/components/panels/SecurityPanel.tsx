import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { useApp } from "../../state/store";
import { useT } from "../../i18n";
import {
  exportEncryptedCopy,
  exportPlainCopy,
  getSecurityStatus,
  isApiError,
  refreshUndoRedo,
  reloadPlain,
} from "../../lib/ipc";
import type { SecurityStatus } from "../../lib/ipc";

interface PermRow {
  key: keyof Omit<SecurityStatus, "handlerRevision">;
  labelKey: string;
}

const PERM_ROWS: PermRow[] = [
  { key: "canPrintHighQuality", labelKey: "高质量打印" },
  { key: "canPrintLowQuality", labelKey: "低质量打印" },
  { key: "canModifyDocument", labelKey: "修改文档内容" },
  { key: "canExtractTextAndGraphics", labelKey: "抽取文本与图形" },
  { key: "canAddAnnotations", labelKey: "添加或修改注释" },
  { key: "canFillFormFields", labelKey: "填写表单字段" },
  { key: "canAssembleDocument", labelKey: "组装文档（插页/旋转/删页等）" },
  { key: "canCreateNewFormFields", labelKey: "创建新表单字段" },
];

type PermKey = "allowPrint" | "allowCopy" | "allowModify" | "allowAnnotate";

const PERM_TOGGLES: { key: PermKey; labelKey: string }[] = [
  { key: "allowPrint", labelKey: "允许打印（含高质量打印）" },
  { key: "allowCopy", labelKey: "允许复制文本与图形" },
  { key: "allowModify", labelKey: "允许修改文档内容" },
  { key: "allowAnnotate", labelKey: "允许添加或修改注释" },
];

export default function SecurityPanel() {
  const docId = useApp((s) => s.docId);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const setUndoRedo = useApp((s) => s.setUndoRedo);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const t = useT();

  const [status, setStatus] = useState<SecurityStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  // 现有文档的打开密码：加密文档必须以它读取加密状态、导出明文副本、去除加密。
  const [openPw, setOpenPw] = useState("");
  // 无密码读取加密状态时被拒绝 → 说明文档已加密，需要用户补上现有密码。
  const [needOpenPw, setNeedOpenPw] = useState(false);
  const [userPw, setUserPw] = useState("");
  const [ownerPw, setOwnerPw] = useState("");
  const [permFlags, setPermFlags] = useState<Record<PermKey, boolean>>({
    allowPrint: true,
    allowCopy: false,
    allowModify: false,
    allowAnnotate: false,
  });

  const reload = async () => {
    if (docId === null) return;
    setLoading(true);
    try {
      const s = await getSecurityStatus(docId, openPw);
      setStatus(s);
      setNeedOpenPw(false);
    } catch (e) {
      // 加密文档缺密码时属预期情况：不弹错误，改为提示用户填入现有密码。
      if (isApiError(e) && e.code === "password") {
        setStatus(null);
        setNeedOpenPw(true);
      } else {
        errorToast(e);
      }
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    setStatus(null);
    setOpenPw("");
    setNeedOpenPw(false);
    setUserPw("");
    setOwnerPw("");
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [docId]);

  const isProtected =
    needOpenPw ||
    (status !== null &&
      status.handlerRevision !== "Unprotected" &&
      status.handlerRevision !== "Unknown");

  // 两个密码至少填一个：全空时 PDF 既无打开限制也无权限限制，加密无意义。
  const canEncrypt = docId !== null && (userPw.length > 0 || ownerPw.length > 0);

  const onExportPlain = async () => {
    if (docId === null) return;
    const p = await save({
      title: t("导出明文副本"),
      defaultPath: "plain.pdf",
      filters: [{ name: t("PDF 文档"), extensions: ["pdf"] }],
    });
    if (!p) return;
    setBusy(true);
    try {
      await exportPlainCopy(docId, p, openPw);
      // 同时把内存里的 doc 也去掉加密
      const info = await reloadPlain(docId, openPw);
      updatePages(info);
      markDirty(true);
      setUndoRedo(await refreshUndoRedo(docId));
      pushToast("info", t("已导出明文副本：{path}", { path: p }));
      await reload();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  const onStripInMemory = async () => {
    if (docId === null) return;
    if (!confirm(t("去除当前文档的密码/加密？文件需另存到磁盘生效。"))) return;
    setBusy(true);
    try {
      const info = await reloadPlain(docId, openPw);
      updatePages(info);
      markDirty(true);
      setUndoRedo(await refreshUndoRedo(docId));
      pushToast("info", t("已去除内存中的加密，请立即 Ctrl+S 另存"));
      await reload();
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  // 加密只作用于落盘副本：内存中的文档保持明文，编辑/撤销重做不受影响。
  const onExportEncrypted = async () => {
    if (docId === null || !canEncrypt) return;
    const p = await save({
      title: t("导出加密副本"),
      defaultPath: "encrypted.pdf",
      filters: [{ name: t("PDF 文档"), extensions: ["pdf"] }],
    });
    if (!p) return;
    setBusy(true);
    try {
      await exportEncryptedCopy(docId, p, {
        userPassword: userPw,
        ownerPassword: ownerPw,
        ...permFlags,
      });
      pushToast("info", t("已导出加密副本：{path}", { path: p }));
    } catch (e) {
      errorToast(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="task-body">
      <p className="placeholder">
        {t(
          "读取当前文档的加密状态与权限矩阵；加密文档可导出为明文副本，或在内存中去加密后另存。也可设置打开密码与权限密码，另存为 AES-128 加密副本。",
        )}
      </p>

      {loading && <p className="placeholder">{t("加载中…")}</p>}
      {status && (
        <>
          <div className="security-status">
            <span className="security-status-label">{t("加密状态")}</span>
            <span
              className={`security-badge ${
                isProtected ? "protected" : "unprotected"
              }`}
            >
              {isProtected
                ? t("已加密（{handler}）", {
                    handler: status.handlerRevision,
                  })
                : t("未加密")}
            </span>
          </div>

          <div className="security-perms">
            {PERM_ROWS.map((row) => {
              const ok = status[row.key];
              return (
                <div key={row.key} className="perm-row">
                  <span className={`perm-dot ${ok ? "ok" : "deny"}`} />
                  <span className="perm-label">{t(row.labelKey)}</span>
                  <span className="perm-result">
                    {ok ? t("允许") : t("禁止")}
                  </span>
                </div>
              );
            })}
          </div>
        </>
      )}

      {docId !== null && (
        <>
          <div className="field">
            <label htmlFor="sec-open-pw">
              {t("现有打开密码（读取加密状态与去除密码时需要）")}
            </label>
            <input
              id="sec-open-pw"
              type="password"
              autoComplete="current-password"
              value={openPw}
              onChange={(e) => setOpenPw(e.target.value)}
            />
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <button onClick={reload} disabled={busy || loading}>
              {t("读取加密状态")}
            </button>
          </div>

          {needOpenPw && (
            <p className="placeholder">
              {t("该文档已加密，请填写现有打开密码后重试")}
            </p>
          )}

          <div className="field">
            <label htmlFor="sec-user-pw">
              {t("打开密码（打开文档时需输入，可留空）")}
            </label>
            <input
              id="sec-user-pw"
              type="password"
              autoComplete="new-password"
              value={userPw}
              onChange={(e) => setUserPw(e.target.value)}
            />
          </div>

          <div className="field">
            <label htmlFor="sec-owner-pw">
              {t("权限密码（留空则与打开密码相同）")}
            </label>
            <input
              id="sec-owner-pw"
              type="password"
              autoComplete="new-password"
              value={ownerPw}
              onChange={(e) => setOwnerPw(e.target.value)}
            />
          </div>

          <div className="security-perms">
            {PERM_TOGGLES.map((row) => (
              <label key={row.key} className="perm-row" style={{ cursor: "pointer" }}>
                <input
                  type="checkbox"
                  checked={permFlags[row.key]}
                  onChange={(e) =>
                    setPermFlags({ ...permFlags, [row.key]: e.target.checked })
                  }
                />
                <span className="perm-label">{t(row.labelKey)}</span>
              </label>
            ))}
          </div>

          {!canEncrypt && (
            <p className="placeholder">
              {t("请至少填写打开密码或权限密码，否则不会产生任何限制。")}
            </p>
          )}
        </>
      )}

      <div className="task-footer">
        <button
          className="btn-primary"
          onClick={onExportPlain}
          disabled={busy || docId === null}
        >
          {busy
            ? t("导出中…")
            : isProtected
              ? t("导出明文副本")
              : t("导出当前文档")}
        </button>
        {isProtected && (
          <button onClick={onStripInMemory} disabled={busy}>
            {t("在内存中去除加密")}
          </button>
        )}
        {docId !== null && (
          <button onClick={onExportEncrypted} disabled={busy || !canEncrypt}>
            {t("导出加密副本")}
          </button>
        )}
      </div>
    </div>
  );
}
