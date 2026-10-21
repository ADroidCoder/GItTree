<script lang="ts">
  import { uid } from "../lib/board";
  import { t } from "../lib/i18n";
  import { nextTime, notify, notifySettings, saveNotifySettings, type Reminder } from "../lib/notify.svelte";

  const s = notifySettings;
  const EVERY = [15, 20, 30, 45, 60, 90, 120, 180, 240];
  /** 周一排在最前面 */
  const DAYS = [1, 2, 3, 4, 5, 6, 0];
  const dayName = (d: number) => [t("日"), t("一"), t("二"), t("三"), t("四"), t("五"), t("六")][d];

  function everyLabel(m: number) {
    return m % 60 === 0 ? t("每 {0} 小时", m / 60) : t("每 {0} 分钟", m);
  }

  function nextLabel(r: Reminder) {
    void s.reminders;
    const d = nextTime(r);
    if (!d) return "";
    const hm = `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`;
    const days = Math.round((new Date(d).setHours(0, 0, 0, 0) - new Date().setHours(0, 0, 0, 0)) / 86400_000);
    return days === 0 ? t("下次 {0}", hm) : days === 1 ? t("下次 明天 {0}", hm) : t("下次 周{0} {1}", dayName(d.getDay()), hm);
  }

  function add() {
    s.reminders.push({ id: uid(), enabled: true, title: t("提醒"), body: "", every: 60, start: "09:00", end: "18:00", days: [1, 2, 3, 4, 5] });
    saveNotifySettings();
  }

  function remove(r: Reminder) {
    s.reminders = s.reminders.filter((x) => x.id !== r.id);
    saveNotifySettings();
  }

  function toggleDay(r: Reminder, d: number) {
    r.days = r.days.includes(d) ? r.days.filter((x) => x !== d) : [...r.days, d];
    saveNotifySettings();
  }
</script>

<!-- 设置窗口里的“提醒与通知”；任何输入变化都立即保存 -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="notify" oninput={saveNotifySettings} onchange={saveNotifySettings}>
  <div>
    <button class="small" onclick={() => notify(t("测试通知"), t("能看到这条，说明通知可以正常显示"))}>{t("发一条测试通知")}</button>
  </div>
  <section>
    <div class="stitle">{t("定时提醒")}</div>
    <div class="hint">{t("从开始时间起，每隔一段时间提醒一次。例如 9:00 开始、每 1 小时，就是每个整点提醒。")}</div>
    {#each s.reminders as r (r.id)}
      <div class="rem" class:off={!r.enabled}>
        <div class="row">
          <input type="checkbox" bind:checked={r.enabled} title={t("启用")} />
          <input class="rtitle" bind:value={r.title} placeholder={t("提醒标题")} />
          <span class="next">{nextLabel(r)}</span>
          <button class="del" title={t("删除")} onclick={() => remove(r)}>✕</button>
        </div>
        <div class="row sub">
          <select bind:value={r.every}>
            {#each EVERY as m (m)}<option value={m}>{everyLabel(m)}</option>{/each}
          </select>
          <input type="time" bind:value={r.start} />
          <span class="dash">–</span>
          <input type="time" bind:value={r.end} />
          <span class="days">
            {#each DAYS as d (d)}
              <button class="day" class:on={r.days.includes(d)} onclick={() => toggleDay(r, d)}>{dayName(d)}</button>
            {/each}
          </span>
        </div>
        <input class="rbody" bind:value={r.body} placeholder={t("提醒内容（可选）")} />
      </div>
    {/each}
    <button class="add" onclick={add}>{t("＋ 添加提醒")}</button>
  </section>

  <section>
    <div class="stitle">{t("任务")}</div>
    <label class="check">
      <input type="checkbox" checked={!!s.taskDueAt} onchange={(e) => (s.taskDueAt = e.currentTarget.checked ? "09:00" : "")} />
      {t("每天")}
      <input type="time" bind:value={s.taskDueAt} disabled={!s.taskDueAt} />
      {t("提醒今天到期和已经逾期的任务")}
    </label>
  </section>

  <section>
    <div class="stitle">Git</div>
    <label class="check"><input type="checkbox" bind:checked={s.gitOps} /> {t("在别的窗口时，推送、拉取、克隆等耗时操作完成后通知我")}</label>
    <label class="check"><input type="checkbox" bind:checked={s.remoteCommits} /> {t("自动获取发现当前分支在远程有新提交时通知我")}</label>
  </section>

  <section>
    <div class="stitle">{t("后台运行")}</div>
    <label class="check"><input type="checkbox" bind:checked={s.keepRunning} /> {t("关闭窗口后继续在后台运行，保持提醒")}</label>
    <div class="hint">
      {t("开启后关闭窗口只是隐藏，菜单栏（Windows 上是任务栏右下角）会有 GitTree 图标，点它重新打开，右键可以退出。")}
    </div>
  </section>
</div>

<style>
  .notify {
    display: flex;
    flex-direction: column;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 14px;
  }
  .stitle {
    font-weight: 600;
  }
  .hint {
    color: var(--text-faint);
    font-size: 12px;
  }
  .rem {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-side);
  }
  .rem.off {
    opacity: 0.6;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .row.sub {
    flex-wrap: wrap;
  }
  .rtitle {
    flex: 1;
    min-width: 0;
    font-weight: 600;
  }
  .rbody {
    width: 100%;
  }
  .next {
    color: var(--text-faint);
    font-size: 12px;
    white-space: nowrap;
  }
  .del {
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-faint);
  }
  .del:hover {
    color: var(--del);
  }
  select {
    height: 28px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--input-bg);
    color: var(--text);
    font: inherit;
  }
  input[type="time"] {
    width: 96px;
    height: 28px;
  }
  .dash {
    color: var(--text-faint);
  }
  .days {
    display: flex;
    gap: 3px;
    margin-left: auto;
  }
  .day {
    width: 26px;
    height: 26px;
    padding: 0;
    font-size: 12px;
  }
  .day.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .add {
    align-self: flex-start;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }
</style>
