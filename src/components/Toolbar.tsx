import { useApp } from "../state/store";
import type { TaskId } from "../state/store";

const TOOLS: { id: Exclude<TaskId, null>; label: string; icon: string }[] = [
  { id: "merge", label: "合并", icon: "🗂" },
  { id: "split", label: "拆分", icon: "✂" },
  { id: "watermark", label: "水印", icon: "💧" },
  { id: "edit", label: "编辑", icon: "✏" },
  { id: "security", label: "密码", icon: "🔒" },
  { id: "export", label: "导出", icon: "🖼" },
];

export default function Toolbar({ onOpenFile }: { onOpenFile: () => void }) {
  const task = useApp((s) => s.task);
  const openTask = useApp((s) => s.openTask);
  const hasDoc = useApp((s) => s.docId !== null);
  const viewMode = useApp((s) => s.viewMode);
  const setViewMode = useApp((s) => s.setViewMode);
  const setSearchOpen = useApp((s) => s.setSearchOpen);
  const searchOpen = useApp((s) => s.searchOpen);

  return (
    <header className="toolbar">
      <span className="logo">PDFe</span>
      <button className="tbtn" onClick={onOpenFile} title="打开 PDF（Ctrl+O）">
        📂 打开
      </button>
      <span className="sep" />
      {TOOLS.map((t) => (
        <button
          key={t.id}
          className={`tbtn${task === t.id ? " active" : ""}`}
          disabled={!hasDoc}
          onClick={() => openTask(task === t.id ? null : t.id)}
        >
          {t.icon} {t.label}
        </button>
      ))}
      <span className="spacer" />
      <button
        className={`tbtn${viewMode === "continuous" ? " active" : ""}`}
        disabled={!hasDoc}
        onClick={() => setViewMode("continuous")}
        title="连续滚动"
      >
        连续
      </button>
      <button
        className={`tbtn${viewMode === "single" ? " active" : ""}`}
        disabled={!hasDoc}
        onClick={() => setViewMode("single")}
        title="单页"
      >
        单页
      </button>
      <button
        className={`tbtn${viewMode === "dual" ? " active" : ""}`}
        disabled={!hasDoc}
        onClick={() => setViewMode("dual")}
        title="双页对开"
      >
        双页
      </button>
      <span className="sep" />
      <button
        className={`tbtn${searchOpen ? " active" : ""}`}
        disabled={!hasDoc}
        onClick={() => setSearchOpen(!searchOpen)}
        title="全文搜索（Ctrl+F）"
      >
        🔍 搜索
      </button>
    </header>
  );
}
