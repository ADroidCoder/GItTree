// 界面文字的翻译。中文原文直接当作键：t("正在推送…")，英文界面查 en.ts，查不到就显示原文。
// 参数用 {0} {1} 占位：t("已归档 “{0}”", title)。
// 英文里要区分单复数时写成 "{0} commit||{0} commits"，按第一个参数是不是 1 来选。
// 切换语言会重新加载界面（标签页会自动恢复），所以这里不需要做成响应式。

import { invoke } from "@tauri-apps/api/core";
import en from "./en";

export type Lang = "zh" | "en";
const KEY = "gittree.lang";

/** 用户选的语言；null 表示跟随系统 */
export function savedLang(): Lang | null {
  try {
    const v = localStorage.getItem(KEY);
    return v === "zh" || v === "en" ? v : null;
  } catch {
    return null;
  }
}

export const lang: Lang = savedLang() ?? (navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en");

const dict: Record<string, string> = lang === "en" ? en : {};

export function t(text: string, ...args: unknown[]): string {
  let s = dict[text] ?? text;
  if (s.includes("||")) {
    const [one, many] = s.split("||");
    s = Number(args[0]) === 1 ? one : many;
  }
  if (!args.length) return s;
  return s.replace(/\{(\d+)\}/g, (m, i) => (Number(i) < args.length ? String(args[Number(i)]) : m));
}

/** 切换语言后重新加载界面 */
export function setLang(l: Lang | null) {
  try {
    if (l) localStorage.setItem(KEY, l);
    else localStorage.removeItem(KEY);
  } catch {
    // 存不了就只在这次生效
  }
  location.reload();
}

/** 让后端的提示信息也用同一种语言 */
export function syncBackendLang() {
  return invoke("set_lang", { en: lang === "en" }).catch(() => {});
}
