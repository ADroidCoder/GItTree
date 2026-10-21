<script lang="ts" module>
  import { t } from "../lib/i18n";
  export interface FileItem {
    key: string;
    path: string;
    oldPath: string | null;
    status: string;
    additions?: number;
    deletions?: number;
    binary?: boolean;
  }
</script>

<script lang="ts">
  import type { Snippet } from "svelte";
  import { splitPath } from "../lib/util";

  let {
    files,
    tree = false,
    active = null,
    onOpen,
    actions,
    onContext,
    selected,
    onSelect,
  }: {
    files: FileItem[];
    tree?: boolean;
    active?: string | null;
    onOpen: (f: FileItem) => void;
    actions?: Snippet<[FileItem]>;
    onContext?: (e: MouseEvent, f: FileItem) => void;
    /** 多选：选中的文件 key */
    selected?: Set<string>;
    /** 点击时调用（传入事件，用来判断 ⌘/Ctrl/Shift）；提供了它就由调用方决定是否打开 diff */
    onSelect?: (f: FileItem, e: MouseEvent, visible: FileItem[]) => void;
  } = $props();

  /** 当前显示顺序的文件（Shift 范围选择要用） */
  const ordered = $derived.by(() => {
    if (!tree) return files.slice(0, limit);
    const out: FileItem[] = [];
    const walk = (d: Dir) => {
      for (const sub of d.dirs) if (!collapsed.has(sub.path)) walk(sub);
      out.push(...d.files);
    };
    walk(root);
    return out;
  });

  /** 一次最多渲染这么多文件，超大提交时避免卡顿 */
  const PAGE = 1500;
  let limit = $state(PAGE);

  interface Dir {
    name: string;
    path: string;
    dirs: Dir[];
    files: FileItem[];
  }

  const root = $derived.by(() => {
    const r: Dir = { name: "", path: "", dirs: [], files: [] };
    if (!tree) return r;
    for (const f of files.slice(0, limit)) {
      const parts = f.path.split("/");
      let d = r;
      for (let i = 0; i < parts.length - 1; i++) {
        let next = d.dirs.find((x) => x.name === parts[i]);
        if (!next) {
          next = { name: parts[i], path: parts.slice(0, i + 1).join("/"), dirs: [], files: [] };
          d.dirs.push(next);
        }
        d = next;
      }
      d.files.push(f);
    }
    // 只有一个子目录的目录合并显示，例如 src/main/java
    const compact = (d: Dir): Dir => {
      d.dirs = d.dirs.map(compact);
      if (d.name && d.files.length === 0 && d.dirs.length === 1) {
        const c = d.dirs[0];
        return { ...c, name: `${d.name}/${c.name}` };
      }
      return d;
    };
    return compact(r);
  });

  let collapsed = $state(new Set<string>());
  function toggle(path: string) {
    const next = new Set(collapsed);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    collapsed = next;
  }

  const statusTitle: Record<string, string> = {
    A: t("新增"),
    M: t("修改"),
    D: t("删除"),
    R: t("重命名"),
    C: t("复制"),
    T: t("类型变化"),
    U: t("冲突"),
    "?": t("未跟踪"),
  };
</script>

{#snippet fileRow(f: FileItem, depth: number, showDir: boolean)}
  {@const sp = splitPath(f.path)}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="file"
    class:active={active === f.path}
    class:selected={selected?.has(f.key)}
    style="padding-left:{12 + depth * 14}px"
    onclick={(e) => (onSelect ? onSelect(f, e, ordered) : onOpen(f))}
    oncontextmenu={(e) => {
      if (onContext) {
        e.preventDefault();
        onContext(e, f);
      }
    }}
    title={f.oldPath ? `${f.oldPath} → ${f.path}` : f.path}
  >
    <span class="st st-{f.status === '?' ? 'U' : f.status}" title={statusTitle[f.status] ?? f.status}>{f.status === "?" ? "+" : f.status}</span>
    <span class="fname">{sp.name}</span>
    {#if showDir && sp.dir}<span class="fdir">{sp.dir}</span>{/if}
    <span class="grow"></span>
    {#if f.binary}
      <span class="bin">{t("二进制")}</span>
    {:else if f.additions !== undefined}
      {#if f.additions}<span class="add">+{f.additions}</span>{/if}
      {#if f.deletions}<span class="del">-{f.deletions}</span>{/if}
    {/if}
    {#if actions}<span class="acts">{@render actions(f)}</span>{/if}
  </div>
{/snippet}

{#snippet dirNode(d: Dir, depth: number)}
  {#each d.dirs as sub (sub.path)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="dir" style="padding-left:{12 + depth * 14}px" onclick={() => toggle(sub.path)}>
      <span class="caret">{collapsed.has(sub.path) ? "▸" : "▾"}</span>
      <span>{sub.name}</span>
    </div>
    {#if !collapsed.has(sub.path)}
      {@render dirNode(sub, depth + 1)}
    {/if}
  {/each}
  {#each d.files as f (f.key)}
    {@render fileRow(f, depth, false)}
  {/each}
{/snippet}

<div class="list">
  {#if tree}
    {@render dirNode(root, 0)}
  {:else}
    {#each files.slice(0, limit) as f (f.key)}
      {@render fileRow(f, 0, true)}
    {/each}
  {/if}
  {#if files.length > limit}
    <button class="more" onclick={() => (limit += PAGE)}>{t("还有 {0} 个文件，点击显示更多", files.length - limit)}</button>
  {/if}
</div>

<style>
  .list {
    padding-bottom: 8px;
  }
  .file,
  .dir {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding-right: 10px;
    font-size: 13px;
    white-space: nowrap;
    cursor: default;
  }
  .file:hover,
  .dir:hover {
    background: var(--bg-hover);
  }
  .file.selected {
    background: var(--bg-hover);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .file.active {
    background: var(--bg-selected);
  }
  .dir {
    color: var(--text-dim);
  }
  .caret {
    width: 10px;
    font-size: 10px;
  }
  .st {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    border-radius: 3px;
    font-size: 10.5px;
    font-weight: 700;
    color: #111;
  }
  .st-A {
    background: var(--add);
  }
  .st-M,
  .st-T {
    background: var(--warn);
  }
  .st-D {
    background: var(--del);
  }
  .st-R,
  .st-C {
    background: var(--accent);
  }
  .st-U {
    background: var(--add);
  }
  .fname {
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 1;
  }
  .fdir {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-faint);
    font-size: 12px;
    direction: rtl;
    text-align: left;
    flex-shrink: 2;
    min-width: 30px;
  }
  .grow {
    flex: 1;
  }
  .add {
    color: var(--add);
    font-size: 12px;
  }
  .del {
    color: var(--del);
    font-size: 12px;
  }
  .bin {
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .acts {
    display: none;
    gap: 4px;
  }
  .file:hover .acts {
    display: flex;
  }
  .more {
    display: block;
    margin: 8px auto;
  }
</style>
