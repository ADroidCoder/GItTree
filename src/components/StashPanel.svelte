<script lang="ts">
  import { api, type CommitDetails, type Stash } from "../lib/api";
  import { confirm, errorText, type RepoTab } from "../lib/store.svelte";
  import { formatDate } from "../lib/util";
  import FileList, { type FileItem } from "./FileList.svelte";

  let { tab, stash }: { tab: RepoTab; stash: Stash } = $props();

  // stash 是一个特殊的合并提交：相对第一个父提交的改动是已跟踪文件的修改，
  // 第三个父提交（如果有）保存的是未跟踪的新文件
  let details = $state<CommitDetails | null>(null);
  let untracked = $state<CommitDetails | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    const oid = stash.oid;
    let cancelled = false;
    details = null;
    untracked = null;
    error = null;
    (async () => {
      try {
        const d = await api.commitDetails(tab.id, oid);
        const u = d.parents[2] ? await api.commitDetails(tab.id, d.parents[2]) : null;
        if (!cancelled) {
          details = d;
          untracked = u;
        }
      } catch (e) {
        if (!cancelled) error = errorText(e);
      }
    })();
    return () => (cancelled = true);
  });

  /** 每个文件记下要用哪个提交看 diff */
  const files = $derived.by(() => {
    const out: (FileItem & { oid: string })[] = [];
    for (const f of details?.files ?? [])
      out.push({ key: `t:${f.path}`, path: f.path, oldPath: f.old_path, status: f.status, additions: f.additions, deletions: f.deletions, binary: f.binary, oid: details!.oid });
    for (const f of untracked?.files ?? [])
      out.push({ key: `u:${f.path}`, path: f.path, oldPath: null, status: "?", additions: f.additions, deletions: f.deletions, binary: f.binary, oid: untracked!.oid });
    return out;
  });

  async function drop() {
    if (await confirm("删除 stash", `确定删除 “${stash.message}” 吗？此操作无法撤销。`, "删除", true)) {
      if (await tab.run({ op: "stash", action: "drop", index: stash.index })) tab.selectedStash = null;
    }
  }

  async function pop() {
    if (await tab.run({ op: "stash", action: "pop", index: stash.index }, "正在恢复 stash…")) tab.showWip();
  }
</script>

<div class="panel">
  <div class="head">
    <div class="kind">Stash · {stash.name}</div>
    <div class="subject">{stash.message}</div>
    {#if details}<div class="when">{formatDate(details.author.time)}</div>{/if}
    <div class="actions">
      <button class="primary" onclick={() => tab.run({ op: "stash", action: "apply", index: stash.index }, "正在应用 stash…")}>
        应用
      </button>
      <button onclick={pop} title="应用到工作区并删除这条 stash">恢复并删除</button>
      <button class="danger" onclick={drop}>删除</button>
    </div>
  </div>
  {#if error}
    <div class="msg error">{error}</div>
  {:else if !details}
    <div class="msg">加载中…</div>
  {:else}
    <div class="files-head">{files.length} 个文件{untracked ? `（其中 ${untracked.files.length} 个未跟踪的新文件）` : ""}</div>
    <div class="files">
      <FileList
        {files}
        active={tab.diff?.source === "commit" && files.some((f) => f.oid === tab.diff?.oid) ? tab.diff.path : null}
        onOpen={(f) => {
          const item = files.find((x) => x.key === f.key)!;
          tab.diff = { source: "commit", oid: item.oid, path: f.path, oldPath: f.oldPath };
        }}
      />
    </div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px 16px 12px;
    border-bottom: 1px solid var(--border);
  }
  .kind {
    color: var(--text-faint);
    font-size: 12px;
  }
  .subject {
    font-size: 15px;
    font-weight: 600;
    word-break: break-word;
  }
  .when {
    color: var(--text-dim);
    font-size: 12.5px;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }
  .files-head {
    padding: 8px 16px;
    color: var(--text-dim);
    font-size: 12.5px;
  }
  .files {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .msg {
    padding: 30px;
    text-align: center;
    color: var(--text-dim);
  }
  .msg.error {
    color: var(--del);
  }
</style>
