<script lang="ts">
  import { app, forgetRepo, openRepo, recentRepos } from "../lib/store.svelte";

  let { onOpenFolder }: { onOpenFolder: () => void } = $props();
  let recent = $state(recentRepos());
</script>

<div class="welcome">
  <div class="card">
    <h1>GitTree</h1>
    <p class="sub">快速、免费的 Git 图形客户端</p>
    <button class="primary big" onclick={onOpenFolder}>打开本地仓库…</button>
    <div class="more-actions">
      <button onclick={() => (app.cloneDialog = { mode: "clone" })}>克隆远程仓库…</button>
      <button onclick={() => (app.cloneDialog = { mode: "init" })}>新建仓库…</button>
    </div>
    {#if recent.length}
      <div class="recent-title">最近打开</div>
      <div class="recent">
        {#each recent as p (p)}
          <div class="r">
            <button class="open" onclick={() => openRepo(p)} title={p}>
              <span class="name">{p.split(/[\\/]/).pop()}</span>
              <span class="path">{p}</span>
            </button>
            <button
              class="x"
              title="从列表移除"
              onclick={() => {
                forgetRepo(p);
                recent = recentRepos();
              }}>✕</button
            >
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .more-actions {
    display: flex;
    gap: 8px;
    justify-content: center;
    margin-top: 10px;
  }
  .welcome {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    background: var(--bg-graph);
  }
  .card {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 520px;
    max-width: calc(100vw - 32px);
  }
  h1 {
    margin: 0;
    font-size: 34px;
  }
  .sub {
    margin: 8px 0 28px;
    color: var(--text-dim);
  }
  .big {
    height: 38px;
    padding: 0 22px;
    font-size: 14px;
  }
  .recent-title {
    align-self: flex-start;
    margin: 32px 0 8px;
    color: var(--text-dim);
    font-size: 12px;
  }
  .recent {
    width: 100%;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
  }
  .r {
    display: flex;
    align-items: center;
  }
  .r + .r {
    border-top: 1px solid var(--border);
  }
  .open {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    flex: 1;
    min-width: 0;
    height: auto;
    padding: 10px 14px;
    border: 0;
    border-radius: 0;
    background: none;
    text-align: left;
  }
  .open:hover,
  .x:hover {
    background: var(--bg-hover);
  }
  .name {
    font-weight: 600;
  }
  .path {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-faint);
    font-size: 12px;
  }
  .x {
    align-self: stretch;
    height: auto;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text-faint);
  }
</style>
