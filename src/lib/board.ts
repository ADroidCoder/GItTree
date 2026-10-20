import { invoke } from "@tauri-apps/api/core";

export type Priority = "high" | "medium" | "low" | "";

export interface Column {
  id: string;
  name: string;
}

export interface CheckItem {
  id: string;
  text: string;
  done: boolean;
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
  /** 检查项（子任务） */
  checklist: CheckItem[];
  /** 在列内的排序，越小越靠前 */
  order: number;
  created: number;
  updated: number;
  /** 进入最后一列（已完成）的时间，0 表示没有完成 */
  doneAt: number;
  /** 归档时间，0 表示没有归档；归档的任务不在看板上显示 */
  archived: number;
}

export interface Board {
  version: 1;
  columns: Column[];
  cards: Card[];
  /** 完成多少天后自动归档，0 表示不自动归档 */
  autoArchiveDays: number;
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
    autoArchiveDays: 7,
  };
}

export async function loadTasks(): Promise<Board> {
  const b = await invoke<Board | null>("tasks_load");
  if (!b || !Array.isArray(b.columns) || !Array.isArray(b.cards)) return defaultBoard();
  // 补上旧数据里没有的字段
  b.autoArchiveDays ??= 7;
  const last = b.columns[b.columns.length - 1]?.id;
  for (const c of b.cards) {
    c.repo ??= "";
    c.checklist ??= [];
    c.archived ??= 0;
    c.doneAt ??= c.column === last ? c.updated : 0;
  }
  return b;
}

/** 移到了哪一列之后更新完成时间：最后一列表示已完成 */
export function markDone(board: Board, card: Card) {
  const last = board.columns[board.columns.length - 1]?.id;
  if (card.column !== last) card.doneAt = 0;
  else if (!card.doneAt) card.doneAt = Date.now();
}

/** 完成超过设定天数的任务自动归档，返回归档了几个 */
export function autoArchive(board: Board) {
  if (!board.autoArchiveDays) return 0;
  const last = board.columns[board.columns.length - 1]?.id;
  const before = Date.now() - board.autoArchiveDays * 86400_000;
  let n = 0;
  for (const c of board.cards) {
    if (!c.archived && c.column === last && c.doneAt && c.doneAt < before) {
      c.archived = Date.now();
      n++;
    }
  }
  return n;
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

/** 从归档里恢复：原来的列被删了就放到最后一列；完成时间重新算，免得马上又被自动归档 */
export function unarchive(board: Board, card: Card) {
  card.archived = 0;
  if (!board.columns.some((c) => c.id === card.column)) card.column = board.columns[board.columns.length - 1]?.id ?? card.column;
  card.doneAt = 0;
  markDone(board, card);
  card.updated = Date.now();
}
