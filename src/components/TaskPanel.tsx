import { useApp } from "../state/store";
import type { TaskId } from "../state/store";

const TITLES: Record<Exclude<TaskId, null>, string> = {
  merge: "合并文档",
  split: "拆分文档",
  watermark: "水印",
  edit: "内容编辑",
  security: "文档安全",
  export: "导出图片",
};

const DESC: Record<Exclude<TaskId, null>, string> = {
  merge: "将多个 PDF 按顺序合并为一个文档，可对每个文件选择页码范围。",
  split: "按固定页数、自定义范围或书签层级，将文档拆分为多个文件。",
  watermark: "为页面添加文字或图片水印，支持位置、透明度与平铺。",
  edit: "双击页面文字进入编辑；新增文本框；选中图片可移动、缩放、删除。",
  security: "为文档设置打开密码与权限密码，或移除已有密码。",
  export: "将页面导出为 PNG / JPG 图片。",
};

export default function TaskPanel() {
  const task = useApp((s) => s.task);
  const closeTask = useApp((s) => s.closeTask);

  if (task === null) return null;

  return (
    <aside className="task">
      <h3>
        <span>{TITLES[task]}</span>
        <button title="关闭面板" onClick={closeTask}>
          ✕
        </button>
      </h3>
      <div className="body">
        <p className="placeholder">{DESC[task]}</p>
        <p className="placeholder" style={{ marginTop: 12 }}>
          该功能将在后续里程碑（M3–M6）中交付。
        </p>
      </div>
    </aside>
  );
}
