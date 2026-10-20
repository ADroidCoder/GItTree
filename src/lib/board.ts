import { invoke } from "@tauri-apps/api/core";

export type Priority = "high" | "medium" | "low" | "";

export interface Column {
  id: string;
  name: string;
}

export interface Card {
  id: string;
  column: string;
  title: string;
  description: string;
  assignee: string;
  priority: Priority;
  /** 截止日期 YYYY-MM-DD，空表示没有 */
  due: string;
  /** 关联的仓库（工作区路径），空表示不关联；只是引用，不会自动做任何 git 操作 */
  repo: string;
  /** 关联的分支，例如 feature/login */
  branch: string;
  /** 在列内的排序，越小越靠前 */
  order: number;
  created: number;
  updated: number;
}

export interface Board {
  version: 1;
  columns: Column[];
  cards: Card[];
}

export const PRIORITIES: { value: Priority; label: string; color: string }[] = [
  { value: "high", label: "高", color: "#de4e4e" },
  { value: "medium", label: "中", color: "#f2ca33" },
  { value: "low", label: "低", color: "#15a0bf" },
  { value: "", label: "无", color: "transparent" },
];

export function uid() {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

export function defaultBoard(): Board {
  return {
    version: 1,
    columns: [
      { id: "todo", name: "待办" },
      { id: "doing", name: "进行中" },
      { id: "review", name: "待验证" },
      { id: "done", name: "已完成" },
    ],
    cards: [],
  };
}

export async function loadTasks(): Promise<Board> {
  const b = await invoke<Board | null>("tasks_load");
  if (!b || !Array.isArray(b.columns) || !Array.isArray(b.cards)) return defaultBoard();
  // 旧数据没有 repo 字段
  for (const c of b.cards) c.repo ??= "";
  return b;
}

export function saveTasks(board: Board) {
  return invoke<void>("tasks_save", { board });
}

/** 任务标题 → 分支名，例如 “用户登录 Login page” → “login-page”；全是中文时用任务编号 */
export function branchSlug(card: Card) {
  const slug = card.title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
  return slug || `task-${card.id.slice(-6)}`;
}
