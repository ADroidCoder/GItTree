<script lang="ts">
  import { t } from "../lib/i18n";
  import type { RefInfo, Stash } from "../lib/api";
  import { drag, type RepoTab } from "../lib/store.svelte";

  let {
    tab,
    onRefMenu,
    onRefDblClick,
    onStashMenu,
    onRefDrop,
    onRemoteMenu,
  }: {
    tab: RepoTab;
    onRefMenu: (e: MouseEvent, ref: RefInfo) => void;
    onRefDblClick: (ref: RefInfo) => void;
    onStashMenu: (e: MouseEvent, s: Stash) => void;
    onRefDrop: (e: MouseEvent, src: RefInfo, dst: RefInfo) => void;
    /** 右键远程仓库（name 为空表示右键“远程”标题） */
    onRemoteMenu: (e: MouseEvent, name: string | null) => void;
  } = $props();

  /** 拖动分支时，鼠标下面的目标分支 */
  let dropOn = $state<string | null>(null);

  let filter = $state("");
  let open = $state<Record<string, boolean>>(loadOpen());

  function loadOpen(): Record<string, boolean> {
    try {
      return JSON.parse(localStorage.getItem("gittree.sidebar") ?? "") ?? {};
    } catch {
      return {};
    }
  }
  /** 默认展开：本地、远程、stash；标签和子文件夹默认按下面的规则 */
  function isOpen(key: string) {
    return open[key] ?? !(key === "tag" || key.startsWith("dir:remote"));
  }
  function toggle(key: string) {
    open = { ...open, [key]: !isOpen(key) };
    try {
      localStorage.setItem("gittree.sidebar", JSON.stringify(open));
    } catch {
      // 忽略
    }
  }

  const refs = $derived(tab.repo?.refs ?? []);
  const f = $derived(filter.trim().toLowerCase());
  const match = (r: RefInfo) => !f || r.name.toLowerCase().includes(f);

  /** 按 “/” 把分支组织成文件夹 */
  interface Folder {
    key: string;
    name: string;
    folders: Folder[];
    items: { ref: RefInfo; label: string }[];
  }
  function buildTree(list: RefInfo[], keyPrefix: string, strip: number): Folder {
    const root: Folder = { key: keyPrefix, name: "", folders: [], items: [] };
    for (const r of [...list].sort((a, b) => a.name.localeCompare(b.name))) {
      const parts = r.name.split("/").slice(strip);
      let d = root;
      for (let i = 0; i < parts.length - 1; i++) {
        let next = d.folders.find((x) => x.name === parts[i]);
        if (!next) {
          next = { key: `${d.key}/${parts[i]}`, name: parts[i], folders: [], items: [] };
          d.folders.push(next);
        }
        d = next;
      }
      d.items.push({ ref: r, label: parts[parts.length - 1] });
    }
    return root;
  }

  const locals = $derived(refs.filter((r) => r.kind === "local" && match(r)));
  const localTree = $derived(buildTree(locals, "dir:local", 0));
  const remotes = $derived.by(() => {
    const groups = new Map<string, RefInfo[]>();
    for (const r of refs) {
      if (r.kind !== "remote" || !match(r)) continue;
      const remote = r.name.split("/")[0];
      if (!groups.has(remote)) groups.set(remote, []);
      groups.get(remote)!.push(r);
    }
    return [...groups.entries()]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([name, list]) => ({ name, count: list.length, tree: buildTree(list, `dir:remote:${name}`, 1) }))
      // 还没获取过（没有远程分支）的远程仓库也要显示，才能右键复制地址、删除
      .concat(
        filter.trim()
          ? []
          : tab.remotes.filter((r) => !groups.has(r.name)).map((r) => ({ name: r.name, count: 0, tree: buildTree([], `dir:remote:${r.name}`, 1) })),
      );
  });
  const remoteCount = $derived(remotes.reduce((n, g) => n + g.count, 0));
  // 标签按版本号倒序，新版本在前面
  const tags = $derived(
    refs
      .filter((r) => r.kind === "tag" && match(r))
      .sort((a, b) => b.name.localeCompare(a.name, undefined, { numeric: true })),
  );
  const stashes = $derived(tab.stashes.filter((s) => !f || s.message.toLowerCase().includes(f)));

  function goto(r: RefInfo) {
    if (r.row === null) return;
    tab.select(r.row, r.oid);
    tab.scrollTo(r.row);
  }

  /** 标签很多时只渲染一部分 */
  let tagLimit = $state(200);
</script>

{#snippet refItem(r: RefInfo, label: string, depth: number)}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="item"
    class:head={r.is_head}
    class:selected={r.row !== null && r.row === tab.selectedRow}
    class:missing={r.row === null}
    class:drop-target={dropOn === r.full}
    style="padding-left:{14 + depth * 14}px"
    draggable={r.kind !== "tag"}
    ondragstart={(e) => {
      drag.ref = r;
      e.dataTransfer?.setData("text/plain", r.name);
    }}
    ondragend={() => {
      drag.ref = null;
      dropOn = null;
    }}
    ondragover={(e) => {
      if (!drag.ref || drag.ref.full === r.full || r.kind === "tag") return;
      e.preventDefault();
      dropOn = r.full;
    }}
    ondragleave={() => dropOn === r.full && (dropOn = null)}
    ondrop={(e) => {
      e.preventDefault();
      const src = drag.ref;
      drag.ref = null;
      dropOn = null;
      if (src) onRefDrop(e, src, r);
    }}
    onclick={() => goto(r)}
    ondblclick={() => onRefDblClick(r)}
    oncontextmenu={(e) => {
      e.preventDefault();
      onRefMenu(e, r);
    }}
    title={r.full}
  >
    <span class="mark">{r.is_head ? "✓" : ""}</span>
    <span class="label">{label}</span>
    {#if r.ahead}<span class="ab up" title={t("领先上游 {0} 个提交", r.ahead)}>{r.ahead}↑</span>{/if}
    {#if r.behind}<span class="ab down" title={t("落后上游 {0} 个提交", r.behind)}>{r.behind}↓</span>{/if}
  </div>
{/snippet}

{#snippet folder(d: Folder, depth: number)}
  {#each d.folders as sub (sub.key)}
    <button class="folder" style="padding-left:{14 + depth * 14}px" onclick={() => toggle(sub.key)}>
      <span class="caret">{isOpen(sub.key) || f ? "▾" : "▸"}</span>
      <span class="fico">📁</span>
      {sub.name}
    </button>
    {#if isOpen(sub.key) || f}
      {@render folder(sub, depth + 1)}
    {/if}
  {/each}
  {#each d.items as it (it.ref.full)}
    {@render refItem(it.ref, it.label, depth)}
  {/each}
{/snippet}

{#snippet section(key: string, icon: string, title: string, count: number)}
  <button class="sec" onclick={() => toggle(key)}>
    <span class="caret">{isOpen(key) ? "▾" : "▸"}</span>
    <span class="sico">{icon}</span>
    <span class="stitle">{title}</span>
    <span class="count">{count}</span>
  </button>
{/snippet}

<div class="sidebar">
  <div class="filter">
    <input placeholder={t("筛选分支、标签、stash")} bind:value={filter} />
  </div>
  <div class="scroll">
    {@render section("local", "💻", t("本地"), locals.length)}
    {#if isOpen("local")}
      {@render folder(localTree, 0)}
    {/if}

    <div class="sec-wrap" oncontextmenu={(e) => (e.preventDefault(), onRemoteMenu(e, null))} role="presentation">
      {@render section("remote", "☁", t("远程"), remoteCount)}
    </div>
    {#if isOpen("remote")}
      {#each remotes as g (g.name)}
        <button
          class="folder"
          style="padding-left:14px"
          onclick={() => toggle(`remote:${g.name}`)}
          oncontextmenu={(e) => (e.preventDefault(), onRemoteMenu(e, g.name))}
          title={tab.remotes.find((r) => r.name === g.name)?.url ?? g.name}
        >
          <span class="caret">{isOpen(`remote:${g.name}`) ? "▾" : "▸"}</span>
          <span class="fico">🌐</span>
          {g.name}
          <span class="count">{g.count}</span>
        </button>
        {#if isOpen(`remote:${g.name}`)}
          {@render folder(g.tree, 1)}
        {/if}
      {:else}
        <button class="more" onclick={(e) => onRemoteMenu(e, null)}>{t("＋ 添加远程仓库")}</button>
      {/each}
    {/if}

    {@render section("tag", "◆", t("标签"), tags.length)}
    {#if isOpen("tag")}
      {#each tags.slice(0, tagLimit) as r (r.full)}
        {@render refItem(r, r.name, 0)}
      {/each}
      {#if tags.length > tagLimit}
        <button class="more" onclick={() => (tagLimit += 500)}>{t("还有 {0} 个标签…", tags.length - tagLimit)}</button>
      {/if}
    {/if}

    {@render section("stash", "⊟", "Stash", stashes.length)}
    {#if isOpen("stash")}
      {#each stashes as s (s.oid)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="item"
          class:selected={tab.panel === "stash" && tab.selectedStash?.oid === s.oid}
          style="padding-left:14px"
          onclick={() => tab.showStash(s)}
          oncontextmenu={(e) => {
            e.preventDefault();
            onStashMenu(e, s);
          }}
          title={s.message}
        >
          <span class="mark"></span>
          <span class="label">{s.message}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-side);
    border-right: 1px solid var(--border);
  }
  .filter {
    padding: 10px;
  }
  .filter input {
    width: 100%;
  }
  .scroll {
    flex: 1;
    overflow: auto;
    padding-bottom: 12px;
  }
  .sec {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 32px;
    margin-top: 4px;
    padding: 0 10px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text-dim);
    font: inherit;
    font-size: 11.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-align: left;
    cursor: pointer;
  }
  .sec:hover {
    color: var(--text);
    border: 0;
  }
  .sico {
    width: 16px;
    text-align: center;
    font-size: 12px;
  }
  .stitle {
    flex: 1;
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 5px;
    width: 100%;
    height: 26px;
    padding-right: 10px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text-dim);
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .folder:hover {
    background: var(--bg-hover);
    border: 0;
  }
  .fico {
    font-size: 11px;
  }
  .caret {
    width: 10px;
    font-size: 9px;
    color: var(--text-faint);
  }
  .count {
    margin-left: auto;
    min-width: 20px;
    padding: 0 6px;
    border-radius: 9px;
    background: var(--bg-hover);
    color: var(--text-faint);
    font-size: 11px;
    font-weight: normal;
    letter-spacing: 0;
    text-align: center;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding-right: 10px;
    font-size: 13px;
    white-space: nowrap;
    cursor: default;
  }
  .item:hover {
    background: var(--bg-hover);
  }
  .item.drop-target {
    outline: 2px dashed var(--accent);
    outline-offset: -2px;
  }
  .item.selected {
    background: var(--bg-selected);
  }
  .item.head {
    font-weight: 600;
    color: var(--accent);
  }
  .item.missing {
    color: var(--text-faint);
  }
  .mark {
    width: 14px;
    flex-shrink: 0;
    color: var(--add);
    font-size: 11px;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
  }
  .ab {
    font-size: 11px;
    padding: 0 5px;
    border-radius: 3px;
    background: var(--bg-hover);
  }
  .up {
    color: var(--add);
  }
  .down {
    color: var(--warn);
  }
  .more {
    margin: 4px 14px;
    font-size: 12px;
  }
</style>
