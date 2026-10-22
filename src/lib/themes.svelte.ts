// 配色主题：内置几套，也可以自己新建。
// 每套主题只定十几个关键颜色，悬停、选中、差异背景等由关键颜色算出来（derive）。
// 内置的深色、浅色和“跟随系统”直接用 app.css 里写好的变量；其他主题把算出来的变量写到 <html> 的 style 上，
// 同时缓存到 localStorage，index.html 在页面渲染前就套上，启动时不会先闪一下默认配色。

import { t } from "./i18n";

export type Base = "dark" | "light";

export interface ThemeColors {
  tabbar: string;
  toolbar: string;
  side: string;
  graph: string;
  card: string;
  border: string;
  text: string;
  textDim: string;
  textFaint: string;
  accent: string;
  add: string;
  del: string;
  warn: string;
  /** 提交图里分支线的颜色，按顺序循环使用 */
  lanes: string[];
}

export interface ThemeDef {
  id: string;
  name: string;
  /** 深色还是浅色底子：决定语法高亮配色、阴影等没有开放修改的部分 */
  base: Base;
  colors: ThemeColors;
}

/** 编辑器里开放修改的颜色，按界面从外到里排 */
export const COLOR_KEYS: { key: Exclude<keyof ThemeColors, "lanes">; label: string }[] = [
  { key: "tabbar", label: t("标签栏") },
  { key: "toolbar", label: t("工具栏") },
  { key: "side", label: t("侧边栏和面板") },
  { key: "graph", label: t("提交图背景") },
  { key: "card", label: t("菜单和卡片") },
  { key: "border", label: t("边框") },
  { key: "text", label: t("文字") },
  { key: "textDim", label: t("次要文字") },
  { key: "textFaint", label: t("提示文字") },
  { key: "accent", label: t("强调色") },
  { key: "add", label: t("新增") },
  { key: "del", label: t("删除") },
  { key: "warn", label: t("警告") },
];

/** GitKraken 经典的 10 色（青、蓝、紫、品红、粉、红、橙、黄、绿、青绿） */
export const DEFAULT_LANES = ["#15a0bf", "#0669f7", "#8e00c2", "#c517b6", "#d90171", "#cd0101", "#f25d2e", "#f2ca33", "#7bd938", "#2ece9d"];

export const PRESETS: ThemeDef[] = [
  {
    // 和 app.css 里的深色主题一致（半透明的颜色换成了叠在背景上的效果）
    id: "dark",
    name: t("深色"),
    base: "dark",
    colors: {
      tabbar: "#131418",
      toolbar: "#282d35",
      side: "#272a31",
      graph: "#1c1e23",
      card: "#32363f",
      border: "#2e3035",
      text: "#e1e2e3",
      textDim: "#a4a5a7",
      textFaint: "#77787b",
      accent: "#4d88ff",
      add: "#7bd938",
      del: "#de4e4e",
      warn: "#f2ca33",
      lanes: DEFAULT_LANES,
    },
  },
  {
    id: "light",
    name: t("浅色"),
    base: "light",
    colors: {
      tabbar: "#d3d7de",
      toolbar: "#e2e5ea",
      side: "#eceef2",
      graph: "#f8f9fb",
      card: "#ffffff",
      border: "#d5dae2",
      text: "#1d2129",
      textDim: "#535c6b",
      textFaint: "#868f9d",
      accent: "#2f6feb",
      add: "#1a7f37",
      del: "#cf222e",
      warn: "#b07800",
      lanes: DEFAULT_LANES,
    },
  },
  {
    id: "one-dark",
    name: "One Dark",
    base: "dark",
    colors: {
      tabbar: "#1b1d23",
      toolbar: "#21252b",
      side: "#21252b",
      graph: "#282c34",
      card: "#2f343e",
      border: "#3a3f4b",
      text: "#d7dae0",
      textDim: "#abb2bf",
      textFaint: "#7f848e",
      accent: "#61afef",
      add: "#98c379",
      del: "#e06c75",
      warn: "#e5c07b",
      lanes: ["#61afef", "#c678dd", "#e06c75", "#d19a66", "#e5c07b", "#98c379", "#56b6c2", "#528bff", "#be5046", "#a9dc76"],
    },
  },
  {
    id: "github-light",
    name: t("GitHub 浅色"),
    base: "light",
    colors: {
      tabbar: "#dfe3e8",
      toolbar: "#eef1f4",
      side: "#f6f8fa",
      graph: "#ffffff",
      card: "#ffffff",
      border: "#d0d7de",
      text: "#1f2328",
      textDim: "#59636e",
      textFaint: "#818b98",
      accent: "#0969da",
      add: "#1a7f37",
      del: "#cf222e",
      warn: "#9a6700",
      lanes: ["#0969da", "#8250df", "#bf3989", "#cf222e", "#bc4c00", "#9a6700", "#1a7f37", "#1b7c83", "#6639ba", "#0550ae"],
    },
  },
  {
    id: "nord",
    name: "Nord",
    base: "dark",
    colors: {
      tabbar: "#242933",
      toolbar: "#3b4252",
      side: "#353b48",
      graph: "#2e3440",
      card: "#434c5e",
      border: "#4c566a",
      text: "#eceff4",
      textDim: "#d8dee9",
      textFaint: "#8f9bb3",
      accent: "#88c0d0",
      add: "#a3be8c",
      del: "#bf616a",
      warn: "#ebcb8b",
      lanes: ["#88c0d0", "#81a1c1", "#5e81ac", "#b48ead", "#bf616a", "#d08770", "#ebcb8b", "#a3be8c", "#8fbcbb", "#d8dee9"],
    },
  },
  {
    // Solarized Light：暖黄底色，长时间看不刺眼
    id: "solarized-light",
    name: t("护眼"),
    base: "light",
    colors: {
      tabbar: "#e4dcc4",
      toolbar: "#eee8d5",
      side: "#f5efdc",
      graph: "#fdf6e3",
      card: "#fffbef",
      border: "#ddd6c1",
      text: "#073642",
      textDim: "#586e75",
      textFaint: "#93a1a1",
      accent: "#268bd2",
      add: "#859900",
      del: "#dc322f",
      warn: "#b58900",
      lanes: ["#268bd2", "#6c71c4", "#d33682", "#dc322f", "#cb4b16", "#b58900", "#859900", "#2aa198", "#1f6fa8", "#a0458e"],
    },
  },
];

/** 直接用 app.css 里写好的配色的主题 */
const CSS_THEMES = new Set(["dark", "light", "system"]);

const mix = (a: string, b: string, pct: number) => `color-mix(in srgb, ${a} ${pct}%, ${b})`;

/** 由关键颜色算出全部 CSS 变量（语法高亮不在里面，跟着 base 用 app.css 里的） */
export function derive(def: ThemeDef): Record<string, string> {
  const c = def.colors;
  const dark = def.base === "dark";
  return {
    "--bg-graph": c.graph,
    "--bg-side": c.side,
    "--bg-toolbar": c.toolbar,
    "--bg-tabbar": c.tabbar,
    "--bg-menu": c.card,
    "--bg-card": c.card,
    "--bg-hover": mix(c.accent, "transparent", dark ? 10 : 8),
    "--bg-selected": mix(c.accent, "transparent", dark ? 22 : 17),
    "--bg-hit": mix(c.warn, "transparent", dark ? 20 : 28),
    "--bg-wip": mix(c.graph, c.side, 50),
    "--bg-hunk": mix(c.accent, c.graph, dark ? 7 : 9),
    "--bg-add": mix(c.add, "transparent", 16),
    "--bg-del": mix(c.del, "transparent", dark ? 18 : 12),
    "--bg-add-strong": mix(c.add, "transparent", 36),
    "--bg-del-strong": mix(c.del, "transparent", dark ? 42 : 30),
    "--bg-empty": dark ? "rgba(255, 255, 255, 0.025)" : "rgba(0, 0, 0, 0.035)",
    "--border": c.border,
    "--border-strong": dark ? c.tabbar : mix(c.border, c.text, 85),
    "--text": c.text,
    "--text-dim": c.textDim,
    "--text-faint": c.textFaint,
    "--accent": c.accent,
    "--add": c.add,
    "--del": c.del,
    "--warn": c.warn,
    "--scrollbar": mix(c.text, "transparent", dark ? 22 : 24),
    "--scrollbar-hover": mix(c.text, "transparent", dark ? 36 : 40),
    "--input-bg": dark ? "rgba(0, 0, 0, 0.2)" : c.card,
    "--button-bg": dark ? mix(c.card, c.toolbar, 70) : c.card,
    "--shadow": dark ? "0 10px 30px rgba(0, 0, 0, 0.5)" : "0 10px 30px rgba(16, 24, 40, 0.18)",
  };
}

const KEY_CURRENT = "gittree.theme";
const KEY_CUSTOM = "gittree.customThemes";
/** 启动时 index.html 读它，在渲染前套上配色 */
const KEY_BOOT = "gittree.themeVars";

function load<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}

function store(key: string, value: unknown) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, typeof value === "string" ? value : JSON.stringify(value));
  } catch {
    // 忽略
  }
}

function loadCurrent(): string {
  try {
    return localStorage.getItem(KEY_CURRENT) || "dark";
  } catch {
    return "dark";
  }
}

export const themes = $state({
  /** "system"、内置主题的 id 或自定义主题的 id */
  current: loadCurrent(),
  custom: load<ThemeDef[]>(KEY_CUSTOM, []).filter(valid),
  /** 当前的分支线颜色 */
  lanes: DEFAULT_LANES,
  /** 每次套用配色后加一，提交图据此重画 */
  version: 0,
});

export function findTheme(id: string): ThemeDef | undefined {
  return PRESETS.find((p) => p.id === id) ?? themes.custom.find((c) => c.id === id);
}

export function isCustom(id: string) {
  return themes.custom.some((c) => c.id === id);
}

/** 当前实际生效的主题（跟随系统时按系统深浅色取内置主题） */
export function effectiveTheme(): ThemeDef {
  if (themes.current === "system") return PRESETS[matchMedia("(prefers-color-scheme: dark)").matches ? 0 : 1];
  return findTheme(themes.current) ?? PRESETS[0];
}

export function laneColor(i: number) {
  return themes.lanes[i % themes.lanes.length];
}

/** 把当前主题套到界面上 */
export function applyTheme() {
  const el = document.documentElement;
  // 先清掉上一套主题（或 index.html 启动时）写上的变量
  for (const k of Object.keys(derive(PRESETS[0]))) el.style.removeProperty(k);
  const id = themes.current;
  const def = effectiveTheme();
  if (CSS_THEMES.has(id)) {
    el.dataset.theme = id;
    el.style.removeProperty("color-scheme");
    store(KEY_BOOT, null);
  } else {
    const vars = derive(def);
    el.dataset.theme = def.base;
    for (const [k, v] of Object.entries(vars)) el.style.setProperty(k, v);
    el.style.colorScheme = def.base;
    store(KEY_BOOT, { base: def.base, vars });
  }
  // 启动画面时设的底色，换主题后去掉，交给 body 的背景
  el.style.removeProperty("background");
  themes.lanes = def.colors.lanes.length ? def.colors.lanes : DEFAULT_LANES;
  themes.version++;
}

export function setTheme(id: string) {
  themes.current = id;
  store(KEY_CURRENT, id);
  applyTheme();
}

function saveCustom() {
  store(KEY_CUSTOM, $state.snapshot(themes.custom));
}

/** 改了自定义主题的颜色后调用：保存，并且如果正在用它就重新套用 */
export function customChanged(id: string) {
  saveCustom();
  if (themes.current === id) applyTheme();
}

/** 以某套主题为底新建一个自定义主题，并切换过去 */
export function newCustom(from: ThemeDef, name = t("{0} 副本", from.name)): ThemeDef {
  const def: ThemeDef = {
    id: `custom-${Date.now().toString(36)}`,
    name,
    base: from.base,
    colors: { ...from.colors, lanes: [...from.colors.lanes] },
  };
  themes.custom.push(def);
  saveCustom();
  setTheme(def.id);
  return def;
}

export function deleteCustom(id: string) {
  const def = themes.custom.find((c) => c.id === id);
  themes.custom = themes.custom.filter((c) => c.id !== id);
  saveCustom();
  if (themes.current === id) setTheme(def?.base ?? "dark");
}

/** 导出成可以分享的 JSON（不带 id） */
export function exportTheme(def: ThemeDef) {
  return JSON.stringify({ name: def.name, base: def.base, colors: def.colors }, null, 2);
}

/** 导入别人分享的主题；格式不对时抛出说明 */
export function importTheme(json: string): ThemeDef {
  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    throw new Error(t("不是有效的主题 JSON"));
  }
  const def = { ...(raw as ThemeDef), id: "x" };
  if (!valid(def)) throw new Error(t("主题里缺少颜色，或者颜色不是 #RRGGBB 格式"));
  return newCustom(def, def.name || t("导入的主题"));
}

const HEX = /^#[0-9a-f]{6}$/i;

export function isHex(v: string) {
  return HEX.test(v);
}

function valid(d: ThemeDef): boolean {
  if (!d || typeof d.id !== "string" || (d.base !== "dark" && d.base !== "light") || !d.colors) return false;
  if (!COLOR_KEYS.every(({ key }) => HEX.test(d.colors[key]))) return false;
  if (!Array.isArray(d.colors.lanes) || !d.colors.lanes.length || !d.colors.lanes.every((l) => HEX.test(l))) d.colors.lanes = [...DEFAULT_LANES];
  return true;
}

// 跟随系统时，系统切换深浅色要换分支线颜色并重画
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  if (themes.current === "system") applyTheme();
});
