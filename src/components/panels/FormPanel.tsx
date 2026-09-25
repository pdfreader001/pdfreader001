import { useEffect, useState } from "react";
import { useApp } from "../../state/store";
import { useT } from "../../i18n";
import { listFormFields, setFormFieldValue, refreshUndoRedo } from "../../lib/ipc";
import type { FormFieldInfo, DocumentInfo } from "../../lib/ipc";
import type { FormFieldKind } from "../../lib/ipc";

export default function FormPanel() {
  const docId = useApp((s) => s.docId);
  const updatePages = useApp((s) => s.updatePages);
  const markDirty = useApp((s) => s.markDirty);
  const setUndoRedo = useApp((s) => s.setUndoRedo);
  const pushToast = useApp((s) => s.pushToast);
  const errorToast = useApp((s) => s.errorToast);
  const closeTask = useApp((s) => s.closeTask);
  const t = useT();

  const [fields, setFields] = useState<FormFieldInfo[]>([]);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  const loadFields = async () => {
    if (docId === null) return;
    setLoading(true);
    try {
      const data = await listFormFields(docId);
      setFields(data);
      const map: Record<string, string> = {};
      data.forEach((f) => {
        map[f.name] = f.value;
      });
      setDrafts(map);
      if (data.length === 0) {
        pushToast("info", t("当前文档没有表单字段"));
      }
    } catch (e) {
      errorToast(e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (docId !== null) {
      loadFields();
    } else {
      setFields([]);
      setDrafts({});
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [docId]);

  const kindLabel: Record<FormFieldKind, string> = {
    unknown: "?",
    pushButton: "Btn",
    checkbox: "☑",
    radioButton: "◉",
    comboBox: "▼T",
    listBox: "▼L",
    text: "T",
    signature: "✎",
  };

  const isEditable = (kind: FormFieldKind): boolean => kind === "text" || kind === "checkbox";

  const isDirty = fields.some((f) => (drafts[f.name] ?? "") !== f.value);

  const updateDraft = (name: string, value: string) => {
    setDrafts((prev) => ({ ...prev, [name]: value }));
  };

  const save = async () => {
    if (docId === null) return;
    if (!isDirty) {
      pushToast("info", t("没有修改"));
      return;
    }
    setSaving(true);
    try {
      let lastInfo: DocumentInfo | null = null;
      for (const f of fields) {
        const newVal = drafts[f.name] ?? "";
        if (newVal === f.value) continue;
        const info = await setFormFieldValue(docId, {
          name: f.name,
          value: newVal,
        });
        lastInfo = info;
        f.value = newVal;
      }
      if (lastInfo) {
        updatePages(lastInfo);
        markDirty(true);
        setUndoRedo(await refreshUndoRedo(docId));
      }
      pushToast("info", t("表单已保存"));
      closeTask();
    } catch (e) {
      errorToast(e);
    } finally {
      setSaving(false);
    }
  };

  const reset = () => {
    const map: Record<string, string> = {};
    fields.forEach((f) => {
      map[f.name] = f.value;
    });
    setDrafts(map);
  };

  return (
    <div className="task-body">
      <p className="placeholder" style={{ fontSize: 11 }}>
        {t("列出 PDF 表单（AcroForm）字段并填写新值，保存后立即写入文档。")}
      </p>
      <div className="task-footer" style={{ marginBottom: 12, display: "flex", gap: 8 }}>
        <button onClick={loadFields} disabled={loading || docId === null}>
          {loading ? t("加载中…") : t("刷新")}
        </button>
        {isDirty && (
          <button onClick={reset} disabled={saving}>
            {t("重置")}
          </button>
        )}
        <button
          className="btn-primary"
          onClick={save}
          disabled={saving || docId === null || !isDirty}
          style={{ marginLeft: "auto" }}
        >
          {saving ? t("保存中…") : t("保存表单")}
        </button>
      </div>
      {loading && <p className="placeholder">{t("加载中…")}</p>}
      {!loading && fields.length === 0 && (
        <p className="placeholder">{t("当前文档没有表单字段")}</p>
      )}
      {fields.length > 0 && (
        <div className="annot-list">
          {fields.map((f) => (
            <div
              key={f.name}
              className="annot-item"
              style={{
                flexDirection: "column",
                alignItems: "flex-start",
                gap: 4,
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <span
                  className="annot-kind"
                  style={{
                    fontWeight: 600,
                    color: "var(--text)",
                    fontSize: 11,
                    padding: "1px 5px",
                    background: "var(--bg-soft)",
                    borderRadius: 3,
                    border: "1px solid var(--border)",
                  }}
                  title={f.kind}
                >
                  {kindLabel[f.kind]}
                </span>
                <span style={{ fontWeight: 600, color: "var(--text)" }}>{f.name}</span>
              </div>
              {f.kind === "checkbox" ? (
                <label className="chk" style={{ padding: "2px 0" }}>
                  <input
                    type="checkbox"
                    checked={["yes", "true", "1", "on", "checked"].includes(
                      (drafts[f.name] ?? "").toLowerCase(),
                    )}
                    onChange={(e) => updateDraft(f.name, e.target.checked ? "true" : "false")}
                  />
                  <span style={{ marginLeft: 4 }}>{drafts[f.name] || t("（未勾选）")}</span>
                </label>
              ) : isEditable(f.kind) ? (
                <input
                  type="text"
                  value={drafts[f.name] ?? ""}
                  onChange={(e) => updateDraft(f.name, e.target.value)}
                  style={{
                    width: "100%",
                    fontSize: 12,
                    padding: "4px 6px",
                    background: "var(--bg-soft)",
                    border: "1px solid var(--border)",
                    borderRadius: 4,
                    color: "var(--text)",
                  }}
                />
              ) : (
                <div
                  style={{
                    width: "100%",
                    fontSize: 12,
                    padding: "4px 6px",
                    background: "var(--bg-soft)",
                    border: "1px solid var(--border)",
                    borderRadius: 4,
                    color: "var(--text-dim)",
                    fontStyle: "italic",
                  }}
                >
                  {drafts[f.name] || t("（无值）")}
                  <span style={{ marginLeft: 6, fontSize: 11 }}>
                    {t("（{kind} 类型暂不支持编辑）", { kind: f.kind })}
                  </span>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
      {fields.length > 0 && (
        <p className="placeholder" style={{ marginTop: 8, fontSize: 11 }}>
          {t("共 {n} 个字段", { n: fields.length })}
        </p>
      )}
    </div>
  );
}
