<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import { api } from "../lib/api";
  import { t } from "../lib/i18n";
  import { app, errorText, type RepoTab } from "../lib/store.svelte";

  let { tab }: { tab: RepoTab } = $props();

  const partial = $derived(tab.extras?.partial);
  /** 勾选的目录；勾了父目录就包含了它下面的所有子目录 */
  // svelte-ignore state_referenced_locally
  const picked = new SvelteSet<string>(tab.extras?.partial.sparse ?? []);
  /** 已经读过的子目录：目录 → 子目录（根目录的键是 ""） */
  let children = $state<Record<string, string[]>>({});
  const open = new SvelteSet<string>();
  let error = $state<string | null>(null);

  async function load(dir: string) {
    if (children[dir]) return;
    try {
      children[dir] = await api.sparseSubdirs(tab.id, dir);
    } catch (e) {
      error = errorText(e);
    }
  }
  load("");
  // 已经勾选的深层目录：把它的上级都展开，打开就能看到
  for (const d of picked) {
    const parts = d.split("/");
    for (let i = 1; i < parts.length; i++) {
      const up = parts.slice(0, i).join("/");
      open.add(up);
      load(up);
    }
  }

  const name = (d: string) => d.slice(d.lastIndexOf("/") + 1);
  /** 上级目录已经勾选了 */
  const coveredByParent = (d: string) => [...picked].some((p) => d.startsWith(p + "/"));

  function toggleOpen(d: string) {
    if (open.has(d)) open.delete(d);
    else {
      open.add(d);
      load(d);
    }
  }

  function toggle(d: string) {
    if (picked.has(d)) picked.delete(d);
    else {
      picked.add(d);
      // 勾了上级，下面单独勾的就多余了
      for (const p of [...picked]) if (p.startsWith(d + "/")) picked.delete(p);
    }
  }

  /** 按树的顺序展开成一行一行 */
  const rows = $derived.by(() => {
    const out: { dir: string; depth: number }[] = [];
    const walk = (parent: string, depth: number) => {
      for (const d of children[parent] ?? []) {
        out.push({ dir: d, depth });
        if (open.has(d)) walk(d, depth + 1);
      }
    };
    walk("", 0);
    return out;
  });

  function close() {
    app.sparseDialog = null;
  }

  async function apply() {
    const dirs = [...picked].sort();
    close();
    await tab.run({ op: "sparse_set", dirs }, t("正在更新检出的目录…"));
  }

  async function disable() {
    close();
    await tab.run({ op: "sparse_disable" }, t("正在检出全部文件…"));
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={onKey}>
  <div class="dialog" role="dialog" aria-modal="true">
    <div class="title">{t("部分检出")}</div>
    <div class="hint">
      {t("只把勾选的目录放到工作区，其他目录不占硬盘，也不会出现在改动里。根目录下的文件总是会检出。")}
      {#if partial?.lazy}<br />{t("这个仓库的文件内容用到时才下载，新勾选的目录需要联网下载。")}{/if}
    </div>

    <div class="tree">
      {#if error}
        <div class="error">{error}</div>
      {:else if !children[""]}
        <div class="empty">{t("正在读取目录…")}</div>
      {:else if !rows.length}
        <div class="empty">{t("仓库里没有目录")}</div>
      {/if}
      {#each rows as r (r.dir)}
        {@const covered = coveredByParent(r.dir)}
        <div class="row" style="padding-left: {8 + r.depth * 18}px">
          <button class="arrow" class:open={open.has(r.dir)} onclick={() => toggleOpen(r.dir)} aria-label={t("展开")}>▸</button>
          <label class:dim={covered}>
            <input type="checkbox" checked={covered || picked.has(r.dir)} disabled={covered} onchange={() => toggle(r.dir)} />
            <span class="name">{name(r.dir)}</span>
          </label>
        </div>
      {/each}
    </div>

    <div class="summary">
      {picked.size ? t("将检出 {0} 个目录：{1}", picked.size, [...picked].sort().join("、")) : t("没有勾选目录：只检出根目录下的文件")}
    </div>

    <div class="buttons">
      {#if partial?.sparse}
        <button onclick={disable}>{t("关闭部分检出，检出全部文件")}</button>
      {/if}
      <span class="grow"></span>
      <button onclick={close}>{t("取消")}</button>
      <button class="primary" onclick={apply}>{partial?.sparse ? t("更新检出的目录") : t("开始部分检出")}</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 8vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 560px;
    max-width: calc(100vw - 32px);
    max-height: 84vh;
    padding: 16px 20px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
  }
  .title {
    font-size: 15px;
    font-weight: 600;
  }
  .hint,
  .summary {
    color: var(--text-dim);
    font-size: 12px;
    line-height: 1.5;
  }
  .summary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tree {
    flex: 1;
    min-height: 160px;
    max-height: 46vh;
    overflow: auto;
    padding: 4px 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--input-bg);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 26px;
    padding-right: 8px;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .arrow {
    width: 20px;
    height: 20px;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-faint);
    font-size: 11px;
    transition: transform 0.12s;
  }
  .arrow.open {
    transform: rotate(90deg);
  }
  label {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 6px;
    min-width: 0;
    cursor: pointer;
  }
  label.dim {
    color: var(--text-dim);
    cursor: default;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty,
  .error {
    padding: 12px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .error {
    color: var(--del);
    white-space: pre-wrap;
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
</style>
