// 记在本机的几个小设置（localStorage）：用到它们的地方和设置窗口共用

import type { MergeMode } from "./api";

function get(key: string) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function set(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // 忽略
  }
}

export const prefs = {
  /** 克隆和新建仓库默认放在哪个文件夹 */
  cloneDir: () => get("gittree.cloneDir") ?? "",
  setCloneDir: (dir: string) => set("gittree.cloneDir", dir),
  /** 差异默认左右并排显示 */
  diffSplit: () => get("gittree.diffSplit") === "1",
  setDiffSplit: (split: boolean) => set("gittree.diffSplit", split ? "1" : "0"),
  /** 合并对话框默认选中的合并方式 */
  mergeMode: () => (get("gittree.mergeMode") as MergeMode | null) ?? "auto",
  setMergeMode: (mode: MergeMode) => set("gittree.mergeMode", mode),
};
