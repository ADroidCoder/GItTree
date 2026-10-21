<script lang="ts">
  import { t } from "../lib/i18n";
  import type { RepoTab } from "../lib/store.svelte";

  let { tab, x, y, onClose }: { tab: RepoTab; x: number; y: number; onClose: () => void } = $props();

  /** 分支很多（远程上千个）时只渲染这么多，其余的靠搜索 */
  const LIMIT = 200;

  let filter = $state("");
  // svelte-ignore state_referenced_locally
  let selected = $state(new Set(tab.focus.filter((f) => f !== "HEAD")));

  const refs = $derived(
    (tab.repo?.refs ?? []).filter((r) => (r.kind === "local" || r.kind === "remote") && !r.name.endsWith("/HEAD")),
  );
  const matched = $derived.by(() => {
    const f = filter.trim().toLowerCase();
    return refs
      .filter((r) => !f || r.name.toLowerCase().includes(f))
      .sort((a, b) => Number(b.is_head) - Number(a.is_head) || Number(selected.has(b.full)) - Number(selected.has(a.full)));
  });
  // 本地分支优先，两组合起来最多 LIMIT 个
  const locals = $derived(matched.filter((r) => r.kind === "local").slice(0, LIMIT));
  const remotes = $derived(matched.filter((r) => r.kind === "remote").slice(0, LIMIT - locals.length));
  const hidden = $derived(matched.length - locals.length - remotes.length);

  const onlyHead = $derived(tab.focus.length === 1 && tab.focus[0] === "HEAD");

  function apply(focus: string[]) {
    onClose();
    tab.setFocus(focus);
  }

  function toggle(full: string) {
    const s = new Set(selected);
    if (s.has(full)) s.delete(full);
    else s.add(full);
    selected = s;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
    else if (e.key === "Enter" && !e.isComposing) {
      if (selected.size) apply([...selected]);
      else if (matched[0]) apply([matched[0].full]);
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}></div>
<div class="panel" style="left:{x}px; top:{y}px">
  <button class="quick" class:on={!tab.focus.length} onclick={() => apply([])}>
    <span class="mark">{!tab.focus.length ? "✓" : ""}</span>{t("全部分支")}
  </button>
  <button class="quick" class:on={onlyHead} onclick={() => apply(["HEAD"])}>
    <span class="mark">{onlyHead ? "✓" : ""}</span>{t("只看当前分支")}
    <span class="hint">{tab.repo?.head.branch ?? "HEAD"}</span>
  </button>
  <div class="sep"></div>
  <!-- svelte-ignore a11y_autofocus -->
  <input bind:value={filter} onkeydown={onKey} placeholder={t("搜索分支，点名称只看这个，勾选可以多选")} autofocus spellcheck="false" />
  <div class="list">
    {#each [{ name: t("本地"), list: locals }, { name: t("远程"), list: remotes }] as g (g.name)}
      {#if g.list.length}
        <div class="group">{g.name}</div>
        {#each g.list as r (r.full)}
          <div class="item" class:sel={selected.has(r.full)}>
            <input type="checkbox" checked={selected.has(r.full)} onchange={() => toggle(r.full)} />
            <button class="name" onclick={() => apply([r.full])} title={t("只看这个分支")}>{r.name}</button>
            {#if r.is_head}<span class="hint">{t("当前")}</span>{/if}
          </div>
        {/each}
      {/if}
    {:else}
      <div class="empty">{t("没有匹配的分支")}</div>
    {/each}
    {#if hidden}<div class="empty">{t("还有 {0} 个分支没有列出，输入关键字筛选", hidden)}</div>{/if}
  </div>
  <div class="foot">
    <span class="count">{selected.size ? t("已选 {0} 个分支", selected.size) : t("勾选多个分支一起看")}</span>
    {#if selected.size}<button class="small" onclick={() => (selected = new Set())}>{t("清空")}</button>{/if}
    <button class="small primary" disabled={!selected.size} onclick={() => apply([...selected])}>{t("应用")}</button>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
  }
  .panel {
    position: fixed;
    z-index: 61;
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 340px;
    padding: 8px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
    font-size: 13px;
  }
  .quick {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 8px;
    border: 0;
    background: none;
    text-align: left;
  }
  .quick:hover {
    background: var(--bg-hover);
  }
  .quick.on {
    color: var(--accent);
    font-weight: 600;
  }
  .mark {
    width: 14px;
    color: var(--add);
  }
  .hint {
    margin-left: auto;
    color: var(--text-faint);
    font-size: 12px;
    font-weight: normal;
  }
  .sep {
    height: 1px;
    margin: 2px 0;
    background: var(--border);
  }
  .list {
    max-height: 46vh;
    overflow: auto;
  }
  .group {
    padding: 6px 8px 2px;
    color: var(--text-faint);
    font-size: 11.5px;
    font-weight: 600;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 8px;
    border-radius: 4px;
  }
  .item:hover {
    background: var(--bg-hover);
  }
  .item.sel .name {
    color: var(--accent);
  }
  .name {
    flex: 1;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    text-align: left;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    cursor: pointer;
  }
  .empty {
    padding: 10px 8px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-top: 6px;
    border-top: 1px solid var(--border);
  }
  .count {
    flex: 1;
    color: var(--text-faint);
    font-size: 12px;
  }
  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }
</style>
