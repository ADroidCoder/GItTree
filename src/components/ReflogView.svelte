<script lang="ts">
  import { api, type ReflogEntry } from "../lib/api";
  import { t } from "../lib/i18n";
  import { confirm, copyText, dialog, errorText, type RepoTab } from "../lib/store.svelte";
  import { formatDate, relativeTime } from "../lib/util";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  let { tab, onClose }: { tab: RepoTab; onClose: () => void } = $props();

  type Row = ReflogEntry & { row: number | null };
  let entries = $state<Row[]>([]);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let filter = $state("");
  let onlyLost = $state(false);
  let selected = $state<string | null>(null);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  $effect(() => {
    void tab.graphVersion;
    let cancelled = false;
    api
      .reflog(tab.id, null)
      .then((r) => {
        if (cancelled) return;
        entries = r;
        error = null;
      })
      .catch((e) => !cancelled && (error = errorText(e)))
      .finally(() => !cancelled && (loading = false));
    return () => (cancelled = true);
  });

  /** reflog 里同一个提交会出现很多次（来回切换分支），只看不在图里的时候每个提交留一条 */
  const shown = $derived.by(() => {
    const f = filter.trim().toLowerCase();
    const seen = new Set<string>();
    return entries.filter((e) => {
      if (onlyLost) {
        if (e.row !== null || seen.has(e.oid)) return false;
        seen.add(e.oid);
      }
      return !f || [e.action, e.message, e.summary, e.oid, e.author].some((x) => x.toLowerCase().includes(f));
    });
  });
  const lostCount = $derived(new Set(entries.filter((e) => e.row === null).map((e) => e.oid)).size);

  /** 动作的中文名和颜色 */
  function actionOf(a: string): { label: string; cls: string } {
    const base = a.split(" ")[0];
    const map: Record<string, [string, string]> = {
      commit: [t("提交"), "add"],
      checkout: [t("切换"), "info"],
      reset: [t("重置"), "warn"],
      rebase: [t("变基"), "warn"],
      merge: [t("合并"), "info"],
      pull: [t("拉取"), "info"],
      "cherry-pick": [t("拣选"), "add"],
      revert: [t("还原"), "add"],
      branch: [t("分支"), "info"],
      clone: [t("克隆"), "info"],
    };
    const [label, cls] = map[base] ?? [base, "info"];
    return { label: a.includes("(amend)") ? t("修改提交") : label, cls };
  }

  function open(e: Row) {
    selected = e.oid + e.time;
    tab.showCommit(e.oid, e.row);
  }

  async function rescue(e: Row) {
    const d = await dialog({
      title: t("从这个提交新建分支"),
      message: t("{0} {1}\n新建一个分支指向它，提交就不会再丢了。", e.oid.slice(0, 7), e.summary),
      input: { placeholder: t("分支名称"), value: `rescue-${e.oid.slice(0, 7)}` },
      checkbox: { label: t("创建后切换到新分支"), value: false },
      confirmText: t("创建"),
    });
    if (d?.input.trim()) await tab.run({ op: "create_branch", name: d.input.trim(), start: e.oid, checkout: d.checkbox }, t("正在创建分支…"));
  }

  async function resetTo(e: Row) {
    const cur = tab.repo?.head.branch ?? "HEAD";
    const d = await dialog({
      title: t("重置到这里"),
      message: t("把 {0} 移到 {1}（{2}）。", cur, e.oid.slice(0, 7), e.summary),
      checkbox: { label: t("同时丢弃工作区里没提交的改动（hard）"), value: false },
      confirmText: t("重置"),
      danger: true,
    });
    if (d) await tab.run({ op: "reset", oid: e.oid, mode: d.checkbox ? "hard" : "mixed" }, t("正在重置…"));
  }

  function onMenu(ev: MouseEvent, e: Row) {
    ev.preventDefault();
    open(e);
    const short = e.oid.slice(0, 7);
    menu = {
      x: ev.clientX,
      y: ev.clientY,
      items: [
        { label: t("从这里新建分支…"), hint: e.row === null ? t("找回") : undefined, action: () => rescue(e) },
        { label: t("检出此提交（分离头指针）"), action: () => tab.run({ op: "checkout_detached", oid: e.oid }, t("正在检出…")) },
        {
          label: t("Cherry-pick 到当前分支"),
          action: async () => {
            if (await confirm("Cherry-pick", t("把提交 {0} 应用到当前分支？", short), t("确定")))
              await tab.run({ op: "cherry_pick", oids: [e.oid] }, t("正在 cherry-pick…"));
          },
        },
        { label: t("把当前分支重置到这里…"), danger: true, action: () => resetTo(e) },
        "sep",
        { label: t("与当前分支对比"), action: () => tab.openCompare(tab.repo?.head.branch ?? "HEAD", e.oid) },
        { label: t("复制提交哈希"), hint: short, action: () => copyText(e.oid) },
      ],
    };
  }
</script>

<div class="rv">
  <div class="bar">
    <span class="title">{t("找回丢失的提交（Reflog）")}</span>
    <input class="filter" bind:value={filter} placeholder={t("筛选：说明、动作、哈希")} spellcheck="false" />
    <label class="lost">
      <input type="checkbox" bind:checked={onlyLost} />
      {t("只看不在提交图里的（{0}）", lostCount)}
    </label>
    <span class="grow"></span>
    <button onclick={onClose} title={t("关闭 (Esc)")}>✕</button>
  </div>
  <div class="tip">
    {t("这里记录了 HEAD 每一次移动：提交、切换分支、重置、变基…… reset 过头、误删分支、变基后找不到的提交都能在这里找到。右键可以从它新建分支把它找回来。")}
  </div>
  {#if error}
    <div class="msg error">{error}</div>
  {:else if loading}
    <div class="msg">{t("加载中…")}</div>
  {:else if !shown.length}
    <div class="msg">{entries.length ? t("没有匹配的记录") : t("还没有任何记录")}</div>
  {:else}
    <div class="list">
      {#each shown as e, i (i)}
        {@const a = actionOf(e.action)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="entry" class:sel={selected === e.oid + e.time} onclick={() => open(e)} oncontextmenu={(ev) => onMenu(ev, e)}>
          <span class="when" title={formatDate(e.time)}>{relativeTime(e.time)}</span>
          <span class="act {a.cls}">{a.label}</span>
          <span class="text">
            <span class="sum">{e.summary}</span>
            {#if e.message && e.message !== e.summary}<span class="how">{e.message}</span>{/if}
          </span>
          {#if e.row === null}<span class="lost-tag" title={t("这个提交不在当前的提交图里")}>{t("不在图中")}</span>{/if}
          <span class="oid">{e.oid.slice(0, 7)}</span>
          <button class="small" onclick={(ev) => (ev.stopPropagation(), rescue(e))} title={t("新建分支指向这个提交")}>{t("找回")}</button>
        </div>
      {/each}
    </div>
  {/if}
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<svelte:window onkeydown={(e) => e.key === "Escape" && !menu && onClose()} />

<style>
  .rv {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-graph);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 42px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .title {
    font-weight: 600;
  }
  .filter {
    width: 220px;
    height: 28px;
  }
  .lost {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-dim);
    font-size: 12.5px;
  }
  .grow {
    flex: 1;
  }
  .tip {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    color: var(--text-faint);
    font-size: 12px;
  }
  .msg {
    padding: 40px;
    text-align: center;
    color: var(--text-dim);
  }
  .msg.error {
    color: var(--del);
    white-space: pre-wrap;
  }
  .list {
    flex: 1;
    overflow: auto;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    font-size: 12.5px;
    cursor: default;
  }
  .entry:hover {
    background: var(--bg-hover);
  }
  .entry.sel {
    background: var(--bg-selected);
  }
  .when {
    width: 80px;
    flex-shrink: 0;
    color: var(--text-faint);
  }
  .act {
    flex-shrink: 0;
    min-width: 52px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--bg-hover);
    font-size: 11.5px;
    text-align: center;
  }
  .act.add {
    color: var(--add);
  }
  .act.warn {
    color: var(--warn);
  }
  .act.info {
    color: var(--accent);
  }
  .text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .how {
    margin-left: 8px;
    color: var(--text-faint);
  }
  .lost-tag {
    flex-shrink: 0;
    padding: 1px 6px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--warn) 18%, transparent);
    color: var(--warn);
    font-size: 11px;
  }
  .oid {
    flex-shrink: 0;
    font-family: var(--mono);
    color: var(--text-faint);
  }
  .small {
    height: 24px;
    padding: 0 8px;
    font-size: 12px;
  }
</style>
