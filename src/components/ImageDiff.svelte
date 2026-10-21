<script lang="ts" module>
  const IMAGE = /\.(png|jpe?g|gif|webp|bmp|ico|avif|svg)$/i;
  /** 能在 diff 里直接显示的图片 */
  export function isImage(path: string) {
    return IMAGE.test(path);
  }
</script>

<script lang="ts">
  import { t } from "../lib/i18n";
  import { api, type ImagePair, type ImageSide } from "../lib/api";
  import { errorText, type DiffTarget, type RepoTab } from "../lib/store.svelte";

  let { tab, target }: { tab: RepoTab; target: DiffTarget } = $props();

  let pair = $state<ImagePair | null>(null);
  let error = $state<string | null>(null);
  let mode = $state<"side" | "swipe">("side");
  let swipe = $state(50);
  /** 图片加载后的尺寸 */
  let dims = $state<{ old?: string; new?: string }>({});

  $effect(() => {
    const tg = target;
    void tab.status;
    let cancelled = false;
    api
      .imagePair(tab.id, tg)
      .then((p) => {
        if (cancelled) return;
        pair = p;
        error = null;
        dims = {};
      })
      .catch((e) => !cancelled && (error = errorText(e)));
    return () => (cancelled = true);
  });

  const mime = $derived.by(() => {
    const ext = target.path.split(".").pop()!.toLowerCase();
    return ext === "svg" ? "image/svg+xml" : ext === "jpg" ? "image/jpeg" : ext === "ico" ? "image/x-icon" : `image/${ext}`;
  });
  const src = (s: ImageSide | null) => (s?.data ? `data:${mime};base64,${s.data}` : null);

  function size(n: number) {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  function onLoad(which: "old" | "new", e: Event) {
    const img = e.currentTarget as HTMLImageElement;
    dims = { ...dims, [which]: `${img.naturalWidth} × ${img.naturalHeight}` };
  }

  const both = $derived(!!src(pair?.old ?? null) && !!src(pair?.new ?? null));
</script>

{#snippet side(which: "old" | "new", s: ImageSide | null)}
  <div class="side">
    <div class="cap {which}">
      {which === "old" ? t("改动前") : t("改动后")}
      {#if s}
        <span class="meta">{dims[which] ? `${dims[which]} · ` : ""}{size(s.size)}{s.lfs ? " · LFS" : ""}</span>
      {/if}
    </div>
    <div class="frame">
      {#if !s}
        <div class="none">{which === "old" ? t("新增的文件") : t("文件已删除")}</div>
      {:else if src(s)}
        <img src={src(s)} alt={which} onload={(e) => onLoad(which, e)} />
      {:else if s.lfs}
        <div class="none">{t("Git LFS 文件还没有下载到本地，拉取 LFS 文件后才能显示")}</div>
      {:else}
        <div class="none">{t("图片太大，不显示")}</div>
      {/if}
    </div>
  </div>
{/snippet}

<div class="img-diff">
  {#if error}
    <div class="none error">{error}</div>
  {:else if pair}
    {#if both}
      <div class="modes">
        <button class:on={mode === "side"} onclick={() => (mode = "side")}>{t("并排")}</button>
        <button class:on={mode === "swipe"} onclick={() => (mode = "swipe")}>{t("滑动对比")}</button>
        {#if mode === "swipe"}<input type="range" min="0" max="100" bind:value={swipe} />{/if}
      </div>
    {/if}
    {#if mode === "swipe" && both}
      <div class="swipe">
        <div class="stack">
          <img class="under" src={src(pair.new)} alt="new" />
          <div class="over" style="clip-path: inset(0 {100 - swipe}% 0 0)">
            <img src={src(pair.old)} alt="old" />
          </div>
          <div class="bar" style="left:{swipe}%"></div>
        </div>
        <div class="legend"><span class="old">← {t("改动前")}</span><span class="new">{t("改动后")} →</span></div>
      </div>
    {:else}
      <div class="sides">
        {@render side("old", pair.old)}
        {@render side("new", pair.new)}
      </div>
    {/if}
  {:else}
    <div class="none">{t("加载中…")}</div>
  {/if}
</div>

<style>
  .img-diff {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: auto;
    padding: 12px;
    gap: 10px;
  }
  .modes {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .modes button {
    height: 26px;
    font-size: 12px;
  }
  .modes button.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .sides {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .side {
    min-width: 0;
  }
  .cap {
    display: flex;
    gap: 8px;
    margin-bottom: 6px;
    font-size: 12.5px;
    font-weight: 600;
  }
  .cap.old {
    color: var(--del);
  }
  .cap.new {
    color: var(--add);
  }
  .meta {
    color: var(--text-faint);
    font-weight: normal;
  }
  /* 棋盘格背景，透明的部分看得出来 */
  .frame,
  .stack {
    background-color: var(--bg-graph);
    background-image:
      linear-gradient(45deg, var(--bg-hover) 25%, transparent 25%),
      linear-gradient(-45deg, var(--bg-hover) 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, var(--bg-hover) 75%),
      linear-gradient(-45deg, transparent 75%, var(--bg-hover) 75%);
    background-size: 16px 16px;
    background-position: 0 0, 0 8px, 8px -8px, -8px 0;
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .frame {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 160px;
    padding: 8px;
  }
  img {
    max-width: 100%;
    max-height: 70vh;
    object-fit: contain;
  }
  .none {
    padding: 30px;
    color: var(--text-dim);
    text-align: center;
  }
  .none.error {
    color: var(--del);
    white-space: pre-wrap;
  }
  .swipe {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .stack {
    position: relative;
    display: inline-block;
    line-height: 0;
  }
  .stack .over {
    position: absolute;
    inset: 0;
  }
  .stack .over img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .bar {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--accent);
  }
  .legend {
    display: flex;
    gap: 24px;
    font-size: 12px;
  }
  .legend .old {
    color: var(--del);
  }
  .legend .new {
    color: var(--add);
  }
</style>
