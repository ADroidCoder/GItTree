// 系统通知：定时提醒（喝水、起来活动……）、任务到期提醒、Git 操作完成提醒。
// 只在软件运行时生效；打开“后台运行”后关掉窗口也会继续提醒。

import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { uid } from "./board";
import { t } from "./i18n";
import { app, preloadTasks } from "./store.svelte";

export interface Reminder {
  id: string;
  enabled: boolean;
  title: string;
  body: string;
  /** 间隔（分钟），从开始时间算起，例如 9:00 开始每 60 分钟就是每个整点 */
  every: number;
  /** 一天里生效的时间段，HH:MM */
  start: string;
  end: string;
  /** 哪几天生效，0 = 周日 */
  days: number[];
}

export interface NotifySettings {
  reminders: Reminder[];
  /** 每天这个时间提醒今天到期和已经逾期的任务，空表示不提醒 */
  taskDueAt: string;
  /** 在别的窗口时，耗时的 git 操作完成后通知 */
  gitOps: boolean;
  /** 自动获取发现当前分支在远程有新提交时通知 */
  remoteCommits: boolean;
  /** 关掉窗口后在后台继续运行 */
  keepRunning: boolean;
}

const KEY = "gittree.notify";
const FIRED_KEY = "gittree.notify.fired";

function defaults(): NotifySettings {
  const workdays = [1, 2, 3, 4, 5];
  return {
    reminders: [
      { id: uid(), enabled: false, title: t("该喝水了"), body: t("起来倒杯水，休息一下眼睛"), every: 60, start: "09:00", end: "18:00", days: workdays },
      { id: uid(), enabled: false, title: t("起来活动一下"), body: t("坐了两个小时了，站起来走走、伸个懒腰"), every: 120, start: "10:00", end: "18:00", days: workdays },
    ],
    taskDueAt: "09:00",
    gitOps: true,
    remoteCommits: true,
    keepRunning: false,
  };
}

function load(): NotifySettings {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "");
    if (v && Array.isArray(v.reminders)) return { ...defaults(), ...v };
  } catch {
    // 第一次用或数据坏了：用默认设置
  }
  return defaults();
}

export const notifySettings = $state(load());

export function saveNotifySettings() {
  try {
    localStorage.setItem(KEY, JSON.stringify($state.snapshot(notifySettings)));
  } catch {
    // 存不了就只在这次生效
  }
  syncKeepRunning();
}

/** 每条提醒最后一次发出的时间点，重新加载界面也不会重复发 */
function fired(): Record<string, string> {
  try {
    return JSON.parse(localStorage.getItem(FIRED_KEY) ?? "{}") ?? {};
  } catch {
    return {};
  }
}
function markFired(id: string, slot: string) {
  try {
    localStorage.setItem(FIRED_KEY, JSON.stringify({ ...fired(), [id]: slot }));
  } catch {
    // 忽略
  }
}

let permission: boolean | null = null;

export async function notify(title: string, body: string) {
  if (permission === null) {
    permission = await isPermissionGranted().catch(() => false);
    if (!permission) permission = (await requestPermission().catch(() => "denied")) === "granted";
  }
  if (permission) sendNotification({ title, body });
}

const minutes = (hhmm: string) => {
  const [h, m] = hhmm.split(":").map(Number);
  return (h || 0) * 60 + (m || 0);
};
const today = (d = new Date()) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/** 这一分钟是不是这条提醒该响的时间点 */
export function isDue(r: Reminder, now = new Date()) {
  if (!r.enabled || r.every <= 0 || !r.days.includes(now.getDay())) return false;
  const m = now.getHours() * 60 + now.getMinutes();
  const start = minutes(r.start);
  return m >= start && m <= minutes(r.end) && (m - start) % r.every === 0;
}

/** 下一次提醒的时间，用于在设置里显示 */
export function nextTime(r: Reminder, now = new Date()): Date | null {
  if (!r.enabled || r.every <= 0 || !r.days.length) return null;
  const d = new Date(now);
  d.setSeconds(0, 0);
  for (let i = 1; i <= 8 * 24 * 60; i++) {
    d.setMinutes(d.getMinutes() + 1);
    if (isDue(r, d)) return d;
  }
  return null;
}

function checkReminders(now: Date) {
  const done = fired();
  const slot = `${today(now)} ${now.getHours()}:${now.getMinutes()}`;
  for (const r of notifySettings.reminders) {
    if (isDue(r, now) && done[r.id] !== slot) {
      markFired(r.id, slot);
      notify(r.title, r.body);
    }
  }
}

async function checkTasks(now: Date) {
  const at = notifySettings.taskDueAt;
  const day = today(now);
  if (!at || now.getHours() * 60 + now.getMinutes() < minutes(at) || fired()["task-due"] === day) return;
  await preloadTasks();
  const board = app.tasks;
  if (!board) return;
  markFired("task-due", day);
  const last = board.columns[board.columns.length - 1]?.id;
  const open = board.cards.filter((c) => !c.archived && c.column !== last && c.due && c.due <= day);
  if (!open.length) return;
  const overdue = open.filter((c) => c.due < day).length;
  const title =
    overdue && overdue === open.length
      ? t("{0} 个任务已经逾期", overdue)
      : overdue
        ? t("{0} 个任务今天到期，{1} 个已经逾期", open.length - overdue, overdue)
        : t("{0} 个任务今天到期", open.length);
  notify(title, open.slice(0, 5).map((c) => `· ${c.title}`).join("\n") + (open.length > 5 ? "\n…" : ""));
}

/** 逾期任务数：标签栏上用红色显示 */
export function overdueCount() {
  const board = app.tasks;
  if (!board) return 0;
  const last = board.columns[board.columns.length - 1]?.id;
  const day = today();
  return board.cards.filter((c) => !c.archived && c.column !== last && c.due && c.due < day).length;
}

/** 窗口不在前台时才通知，正在看着软件就不打扰 */
export function notifyInBackground(title: string, body: string) {
  if (!document.hasFocus()) notify(title, body);
}

function syncKeepRunning() {
  invoke("set_keep_running", {
    on: notifySettings.keepRunning,
    showLabel: t("显示 GitTree"),
    quitLabel: t("退出 GitTree"),
  }).catch(() => {});
}

export function startNotifier() {
  syncKeepRunning();
  // 每 20 秒看一次，保证每一分钟都能检查到
  const tick = () => {
    const now = new Date();
    checkReminders(now);
    checkTasks(now);
  };
  tick();
  setInterval(tick, 20_000);
}
