import { t } from "./i18n";
/** 分支线的颜色：GitKraken 经典的 10 色（青、蓝、紫、品红、粉、红、橙、黄、绿、青绿） */
export const PALETTE = [
  "#15a0bf",
  "#0669f7",
  "#8e00c2",
  "#c517b6",
  "#d90171",
  "#cd0101",
  "#f25d2e",
  "#f2ca33",
  "#7bd938",
  "#2ece9d",
];

export function laneColor(i: number) {
  return PALETTE[i % PALETTE.length];
}

const pad = (n: number) => String(n).padStart(2, "0");

export function formatDate(sec: number) {
  const d = new Date(sec * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function relativeTime(sec: number) {
  const diff = Date.now() / 1000 - sec;
  if (diff < 60) return t("刚刚");
  if (diff < 3600) return t("{0} 分钟前", Math.floor(diff / 60));
  if (diff < 86400) return t("{0} 小时前", Math.floor(diff / 3600));
  if (diff < 86400 * 7) return t("{0} 天前", Math.floor(diff / 86400));
  const d = new Date(sec * 1000);
  const now = new Date();
  if (d.getFullYear() === now.getFullYear()) return t("{0}月{1}日", d.getMonth() + 1, d.getDate());
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** 根据邮箱生成稳定的头像底色 */
export function avatarColor(key: string) {
  let h = 0;
  for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) | 0;
  return `hsl(${Math.abs(h) % 360} 45% 45%)`;
}

export function initials(name: string) {
  const t = name.trim();
  if (!t) return "?";
  // 中文名取最后一个字，英文取首字母
  if (/[一-鿿]/.test(t)) return t.slice(-1);
  const parts = t.split(/\s+/);
  return (parts[0][0] + (parts.length > 1 ? parts[parts.length - 1][0] : "")).toUpperCase();
}

export function splitPath(p: string) {
  const i = p.lastIndexOf("/");
  return i < 0 ? { dir: "", name: p } : { dir: p.slice(0, i + 1), name: p.slice(i + 1) };
}

export const isMac = typeof navigator !== "undefined" && /Mac/.test(navigator.platform);

/**
 * 把 git 远程地址转成仓库网页地址，认不出来时返回 null：
 * git@host:a/b.git、ssh://git@host:22/a/b.git、https://user@host/a/b.git → https://host/a/b
 */
export function webUrl(remote: string): string | null {
  const u = remote.trim();
  let m = u.match(/^[\w.-]+@([^:/]+):(?!\/)(.+?)(?:\.git)?\/?$/);
  if (m) return `https://${m[1]}/${m[2]}`;
  m = u.match(/^ssh:\/\/(?:[^@/]+@)?([^:/]+)(?::\d+)?\/(.+?)(?:\.git)?\/?$/);
  if (m) return `https://${m[1]}/${m[2]}`;
  m = u.match(/^(https?):\/\/(?:[^@/]+@)?([^/]+)\/(.+?)(?:\.git)?\/?$/);
  if (m) return `${m[1]}://${m[2]}/${m[3]}`;
  return null;
}

/** 分享给别人的地址：去掉 https 地址里的用户名和密码 / token */
export function shareUrl(remote: string): string {
  return remote.trim().replace(/^(https?:\/\/)[^@/]+@/, "$1");
}
