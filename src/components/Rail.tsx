import { useApp } from "../state/store";

const RAIL: { id: "thumbnails" | "bookmarks"; icon: string; label: string }[] = [
  { id: "thumbnails", icon: "▦", label: "页面缩略图" },
  { id: "bookmarks", icon: "🔖", label: "书签" },
];

export default function Rail() {
  const leftTab = useApp((s) => s.leftTab);
  const leftVisible = useApp((s) => s.leftVisible);
  const setLeftTab = useApp((s) => s.setLeftTab);
  const toggleLeft = useApp((s) => s.toggleLeft);
  const theme = useApp((s) => s.theme);
  const setTheme = useApp((s) => s.setTheme);

  return (
    <nav className="rail">
      {RAIL.map((r) => (
        <button
          key={r.id}
          title={r.label}
          className={leftVisible && leftTab === r.id ? "active" : ""}
          onClick={() => {
            if (leftVisible && leftTab === r.id) toggleLeft();
            else setLeftTab(r.id);
          }}
        >
          {r.icon}
        </button>
      ))}
      <span style={{ flex: 1 }} />
      <button
        title={theme === "dark" ? "切换浅色主题" : "切换深色主题"}
        onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
      >
        {theme === "dark" ? "☀" : "🌙"}
      </button>
    </nav>
  );
}
