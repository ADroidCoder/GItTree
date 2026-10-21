<script lang="ts">
  import { t } from "../lib/i18n";
  import type { BisectState } from "../lib/api";
  import type { RepoTab } from "../lib/store.svelte";

  let { tab, state }: { tab: RepoTab; state: BisectState } = $props();

  const short = (o: string | null) => o?.slice(0, 7) ?? "";
  /** 剩下的提交还要测几次（git 每次排除一半） */
  const steps = $derived(state.remaining ? Math.ceil(Math.log2(state.remaining + 1)) : 0);

  const mark = (verdict: "good" | "bad" | "skip") =>
    tab.run({ op: "bisect_mark", verdict, oid: null }, t("正在标记…"));
</script>

<div class="bisect" class:found={!!state.found}>
  <span class="ico">⌖</span>
  {#if state.found}
    <span class="text">
      {t("找到了：第一个有问题的提交是")}
      <button class="link mono" onclick={() => tab.jumpToOid(state.found!)}>{short(state.found)}</button>
    </span>
    <span class="grow"></span>
    <button class="primary" onclick={() => tab.run({ op: "bisect_reset" }, t("正在结束二分查错…"))}>{t("结束二分查错")}</button>
  {:else if !state.bad || !state.good.length}
    <span class="text">
      {!state.bad ? t("二分查错：请右键一个有问题的提交，标记为“坏的”") : t("二分查错：请右键一个没问题的提交，标记为“好的”")}
    </span>
    <span class="grow"></span>
    <button onclick={() => tab.run({ op: "bisect_reset" }, t("正在结束二分查错…"))}>{t("结束")}</button>
  {:else}
    <span class="text">
      {t("二分查错：正在测试")}
      <button class="link mono" onclick={() => state.current && tab.jumpToOid(state.current)}>{short(state.current)}</button>
      {#if state.remaining !== null}
        <span class="dim">{t("· 还剩 {0} 个提交，大约 {1} 步", state.remaining, steps)}</span>
      {/if}
    </span>
    <span class="grow"></span>
    <span class="dim">{t("这个提交：")}</span>
    <button class="good" onclick={() => mark("good")} title="git bisect good">✓ {t("没问题")}</button>
    <button class="bad" onclick={() => mark("bad")} title="git bisect bad">✗ {t("有问题")}</button>
    <button onclick={() => mark("skip")} title={t("这个提交没法测试（比如编译不过），换一个")}>{t("跳过")}</button>
    <button onclick={() => tab.run({ op: "bisect_reset" }, t("正在结束二分查错…"))} title={t("回到开始前的分支")}>{t("结束")}</button>
  {/if}
</div>

<style>
  .bisect {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 38px;
    padding: 4px 12px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in srgb, var(--warn) 14%, var(--bg-graph));
    font-size: 13px;
    flex-shrink: 0;
  }
  .bisect.found {
    background: color-mix(in srgb, var(--add) 14%, var(--bg-graph));
  }
  .ico {
    font-size: 16px;
  }
  .grow {
    flex: 1;
  }
  .dim {
    color: var(--text-dim);
  }
  .mono {
    font-family: var(--mono);
  }
  .link {
    height: auto;
    padding: 0 2px;
    border: 0;
    background: none;
    color: var(--accent);
  }
  button {
    height: 26px;
    font-size: 12.5px;
  }
  .good {
    color: var(--add);
  }
  .bad {
    color: var(--del);
  }
</style>
