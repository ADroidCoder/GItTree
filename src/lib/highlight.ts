// diff 的语法高亮和行内差异。
// diff 是虚拟化渲染的，只给屏幕上看得见的行做高亮；每一行单独高亮，跨行的块注释、多行字符串可能着色不完整。

import hljs from "highlight.js/lib/core";
import bash from "highlight.js/lib/languages/bash";
import c from "highlight.js/lib/languages/c";
import cpp from "highlight.js/lib/languages/cpp";
import csharp from "highlight.js/lib/languages/csharp";
import css from "highlight.js/lib/languages/css";
import dart from "highlight.js/lib/languages/dart";
import go from "highlight.js/lib/languages/go";
import gradle from "highlight.js/lib/languages/gradle";
import ini from "highlight.js/lib/languages/ini";
import java from "highlight.js/lib/languages/java";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import kotlin from "highlight.js/lib/languages/kotlin";
import markdown from "highlight.js/lib/languages/markdown";
import objectivec from "highlight.js/lib/languages/objectivec";
import php from "highlight.js/lib/languages/php";
import python from "highlight.js/lib/languages/python";
import ruby from "highlight.js/lib/languages/ruby";
import rust from "highlight.js/lib/languages/rust";
import scss from "highlight.js/lib/languages/scss";
import sql from "highlight.js/lib/languages/sql";
import swift from "highlight.js/lib/languages/swift";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";

const LANGS = { bash, c, cpp, csharp, css, dart, go, gradle, ini, java, javascript, json, kotlin, markdown, objectivec, php, python, ruby, rust, scss, sql, swift, typescript, xml, yaml };
for (const [name, lang] of Object.entries(LANGS)) hljs.registerLanguage(name, lang);

/** 扩展名 → 语言 */
const BY_EXT: Record<string, string> = {
  sh: "bash", bash: "bash", zsh: "bash",
  c: "c", h: "c",
  cc: "cpp", cpp: "cpp", cxx: "cpp", hpp: "cpp", hh: "cpp",
  cs: "csharp",
  css: "css", less: "scss", scss: "scss",
  dart: "dart",
  go: "go",
  gradle: "gradle",
  ini: "ini", properties: "ini", toml: "ini", cfg: "ini", conf: "ini",
  java: "java",
  js: "javascript", mjs: "javascript", cjs: "javascript", jsx: "javascript",
  json: "json",
  kt: "kotlin", kts: "kotlin",
  md: "markdown", markdown: "markdown",
  m: "objectivec", mm: "objectivec",
  php: "php",
  py: "python",
  rb: "ruby",
  rs: "rust",
  sql: "sql",
  swift: "swift",
  ts: "typescript", tsx: "typescript", mts: "typescript", cts: "typescript",
  // Svelte / Vue 文件用 HTML 高亮，脚本部分不着色
  xml: "xml", html: "xml", htm: "xml", svg: "xml", plist: "xml", svelte: "xml", vue: "xml",
  yml: "yaml", yaml: "yaml",
};

export function languageOf(path: string): string | null {
  const name = path.split("/").pop()!.toLowerCase();
  if (name === "dockerfile" || name === "makefile") return "bash";
  const ext = name.includes(".") ? name.split(".").pop()! : "";
  return BY_EXT[ext] ?? null;
}

const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** 超长的行（压缩过的代码）不高亮，免得卡 */
const MAX_LEN = 2000;

function highlight(text: string, lang: string | null): string {
  if (!lang || !text || text.length > MAX_LEN) return escape(text);
  try {
    return hljs.highlight(text, { language: lang, ignoreIllegals: true }).value;
  } catch {
    return escape(text);
  }
}

/**
 * 一行代码的 HTML。mark 是行内差异的范围 [开始, 结束)，这部分会包在 <span class="chg"> 里；
 * 三段分别高亮，所以改动边界上的着色可能略有不同。
 */
export function renderLine(text: string, lang: string | null, mark: [number, number] | null): string {
  if (!mark) return highlight(text, lang);
  const [s, e] = mark;
  return highlight(text.slice(0, s), lang) + `<span class="chg">${highlight(text.slice(s, e), lang)}</span>` + highlight(text.slice(e), lang);
}

/**
 * 一对删除行和新增行之间的行内差异：去掉相同的开头和结尾，中间就是改动的部分。
 * 两行差别太大（基本整行都改了）时返回 null，不单独标出。
 */
export function intraline(oldText: string, newText: string): { old: [number, number]; new: [number, number] } | null {
  if (oldText === newText) return null;
  const max = Math.min(oldText.length, newText.length);
  let p = 0;
  while (p < max && oldText[p] === newText[p]) p++;
  let q = 0;
  while (q < max - p && oldText[oldText.length - 1 - q] === newText[newText.length - 1 - q]) q++;
  // 改动边界停在单词中间时往外扩到整个单词，看起来更自然
  const isWord = (str: string, i: number) => i >= 0 && i < str.length && /[\w\u4e00-\u9fff]/.test(str[i]);
  while (p > 0 && isWord(oldText, p - 1) && (isWord(oldText, p) || isWord(newText, p))) p--;
  while (q > 0 && isWord(oldText, oldText.length - q) && (isWord(oldText, oldText.length - 1 - q) || isWord(newText, newText.length - 1 - q))) q--;
  const same = p + q;
  const longer = Math.max(oldText.length, newText.length);
  if (longer === 0 || same / longer < 0.3) return null;
  return { old: [p, oldText.length - q], new: [p, newText.length - q] };
}
