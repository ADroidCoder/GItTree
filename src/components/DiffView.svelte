<script lang="ts">
  import { t } from "../lib/i18n";
  import type { DiffLine, FileDiff } from "../lib/api";
  import { confirm, errorText, type DiffTarget, type RepoTab } from "../lib/store.svelte";
  import { intraline, languageOf, renderLine } from "../lib/highlight";
  import ImageDiff, { isImage } from "./ImageDiff.svelte";
  import { prefs } from "../lib/prefs";

  let {
    tab,
    target,
    onClose,
    embedded = false,
  }: {
    tab: RepoTab;
    target: DiffTarget;
    onClose: () => void;
    /** 嵌在文件历史里：不显示历史 / Blame / 关闭按钮 */
    embedded?: boolean;
  } = $props();

  const LINE_H = 20;
  let diff = $state<FileDiff | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let split = $state(prefs.diffSplit());
  let scroller = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewH = $state(600);
  /** 并排视图左右两边共用的横向滚动位置 */
  let hx = $state(0);
  let hbars: HTMLDivElement[] = $state([]);
  /** 铺满窗口查看 */
  let full = $state(false);

  function toggleSplit() {
    split = !split;
    prefs.setDiffSplit(split);
  }

  // 目标文件变化或仓库刷新后重新加载
  $effect(() => {
    const t = target;
    void tab.graphVersion;
    void tab.status;
    let cancelled = false;
    loading = true;
    tab
      .loadDiff(t)
      .then((d) => {
        if (cancelled) return;
        diff = d;
        error = null;
      })
      .catch((e) => {
        if (!cancelled) error = errorText(e);
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => (cancelled = true);
  });

  type Item =
    | { t: "hunk"; h: number; header: string }
    | { t: "line"; line: DiffLine; h: number; i: number }
    | { t: "pair"; left: DiffLine | null; right: DiffLine | null; h: number; li: number; ri: number };

  // ---------- 按行选择（暂存 / 取消暂存 / 丢弃选中的行） ----------
  /** 选中的行都在同一个代码块里：h 是代码块下标，lines 是代码块里的行下标 */
  let sel = $state<{ h: number; lines: Set<number> } | null>(null);
  let anchor: { h: number; i: number } | null = null;

  function toggleLine(e: MouseEvent, h: number, i: number) {
    if (!canStageHunk || !diff) return;
    const line = diff.hunks[h].lines[i];
    if (!line || line.kind === " ") return;
    const lines = sel && sel.h === h ? new Set(sel.lines) : new Set<number>();
    if (e.shiftKey && anchor && anchor.h === h) {
      // Shift 点击：选中从上次点击到这里之间的所有改动行
      const [a, b] = anchor.i < i ? [anchor.i, i] : [i, anchor.i];
      for (let k = a; k <= b; k++) if (diff.hunks[h].lines[k].kind !== " ") lines.add(k);
    } else if (lines.has(i)) {
      lines.delete(i);
    } else {
      lines.add(i);
    }
    anchor = { h, i };
    sel = lines.size ? { h, lines } : null;
  }

  const isSel = (h: number, i: number) => !!sel && sel.h === h && sel.lines.has(i);

  async function linesAction(header: string, action: "stage" | "unstage" | "discard") {
    if (!sel) return;
    const lines = [...sel.lines].sort((a, b) => a - b);
    if (action === "discard") {
      if (!(await confirm(t("丢弃选中的行"), t("确定丢弃选中的 {0} 行改动吗？此操作无法撤销。", lines.length), t("丢弃"), true))) return;
      if (await tab.run({ op: "discard_lines", path: target.path, header, lines })) sel = null;
    } else if (await tab.run({ op: "stage_lines", path: target.path, header, staged: action === "unstage", lines })) {
      sel = null;
    }
  }

  // 换了文件就清掉选择
  $effect(() => {
    void target.path;
    void target.staged;
    sel = null;
  });

  const lang = $derived(languageOf(target.path));
  /** 行内差异：删除行 / 新增行里真正变了的范围 */
  let marks = new WeakMap<DiffLine, [number, number]>();
  /** 已经高亮过的行，滚动时不重复计算 */
  let htmlCache = new WeakMap<DiffLine, string>();

  function html(line: DiffLine): string {
    let h = htmlCache.get(line);
    if (h === undefined) {
      h = renderLine(line.text, lang, marks.get(line) ?? null);
      htmlCache.set(line, h);
    }
    return h;
  }

  /** 相邻的一组删除行和新增行按顺序配对，算出每一对的行内差异 */
  function computeMarks(d: FileDiff) {
    marks = new WeakMap();
    htmlCache = new WeakMap();
    for (const hunk of d.hunks) {
      const lines = hunk.lines;
      let i = 0;
      while (i < lines.length) {
        if (lines[i].kind === " ") {
          i++;
          continue;
        }
        const dels: DiffLine[] = [];
        const adds: DiffLine[] = [];
        while (i < lines.length && lines[i].kind === "-") dels.push(lines[i++]);
        while (i < lines.length && lines[i].kind === "+") adds.push(lines[i++]);
        for (let k = 0; k < Math.min(dels.length, adds.length); k++) {
          const r = intraline(dels[k].text, adds[k].text);
          if (r) {
            marks.set(dels[k], r.old);
            marks.set(adds[k], r.new);
          }
        }
      }
    }
  }

  const items = $derived.by(() => {
    const out: Item[] = [];
    if (!diff) return out;
    computeMarks(diff);
    diff.hunks.forEach((hunk, h) => {
      out.push({ t: "hunk", h, header: hunk.header });
      if (!split) {
        hunk.lines.forEach((line, i) => out.push({ t: "line", line, h, i }));
        return;
      }
      // 并排视图：把相邻的删除行和新增行配对
      const lines = hunk.lines;
      let i = 0;
      while (i < lines.length) {
        if (lines[i].kind === " ") {
          out.push({ t: "pair", left: lines[i], right: lines[i], h, li: i, ri: i });
          i++;
          continue;
        }
        const dels: number[] = [];
        const adds: number[] = [];
        while (i < lines.length && lines[i].kind === "-") dels.push(i++);
        while (i < lines.length && lines[i].kind === "+") adds.push(i++);
        for (let k = 0; k < Math.max(dels.length, adds.length); k++) {
          const li = dels[k] ?? -1;
          const ri = adds[k] ?? -1;
          out.push({ t: "pair", left: lines[li] ?? null, right: lines[ri] ?? null, h, li, ri });
        }
      }
    });
    return out;
  });

  /** 并排视图最长一行的字符数：横向滚动条按它算可滚动的宽度 */
  const splitCols = $derived.by(() => {
    if (!split || !diff) return 0;
    let max = 0;
    for (const h of diff.hunks) for (const l of h.lines) max = Math.max(max, textCols(l.text));
    return max;
  });

  const start = $derived(Math.max(0, Math.floor(scrollTop / LINE_H) - 20));
  const end = $derived(Math.min(items.length, Math.ceil((scrollTop + viewH) / LINE_H) + 20));
  const stats = $derived.by(() => {
    let add = 0;
    let del = 0;
    for (const h of diff?.hunks ?? [])
      for (const l of h.lines) {
        if (l.kind === "+") add++;
        else if (l.kind === "-") del++;
      }
    return { add, del };
  });

  const canStageHunk = $derived(target.source === "workdir");

  /** Git LFS 管理的文件：仓库里存的是指针，diff 里只看得到指针的变化 */
  const lfsSizes = $derived.by(() => {
    const lines = diff?.hunks.flatMap((h) => h.lines) ?? [];
    if (!lines.some((l) => l.text.startsWith("version https://git-lfs.github.com/spec/"))) return null;
    const size = (kind: string) => lines.find((l) => l.kind !== (kind === "-" ? "+" : "-") && l.text.startsWith("size "))?.text.slice(5);
    return { old: size("-"), new: size("+") };
  });
  /** 图片：二进制的，或者是 LFS 指针 */
  const showImage = $derived(isImage(target.path) && !!diff && (diff.binary || !!lfsSizes));

  async function hunkAction(header: string, action: "stage" | "unstage" | "discard") {
    if (action === "discard") {
      if (!(await confirm(t("丢弃代码块"), t("确定丢弃这个代码块的改动吗？此操作无法撤销。"), t("丢弃"), true))) return;
      await tab.run({ op: "discard_hunk", path: target.path, header });
    } else {
      await tab.run({ op: "stage_hunk", path: target.path, header, staged: action === "unstage" });
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    // 有选中的行时 Esc 先取消选择，全屏时先退出全屏
    if (sel) sel = null;
    else if (full) full = false;
    else onClose();
  }

  /** 两条横向滚动条拖哪条，另一条和两边的代码都跟着动 */
  function onHScroll(e: Event) {
    hx = (e.currentTarget as HTMLDivElement).scrollLeft;
    for (const b of hbars) if (b && b.scrollLeft !== hx) b.scrollLeft = hx;
  }

  /** 触控板左右滑、Shift+滚轮：转给横向滚动条 */
  function onWheel(e: WheelEvent) {
    if (!split || !hbars[0]) return;
    const dx = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.shiftKey ? e.deltaY : 0;
    if (dx) hbars[0].scrollLeft += dx;
  }

  // 换了文件或切换视图时回到最左边
  $effect(() => {
    void target;
    void split;
    hx = 0;
    for (const b of hbars) if (b) b.scrollLeft = 0;
  });

  /** 一行代码占几列：制表符按 8 列对齐，中文等全角字符占两列 */
  function textCols(s: string) {
    let n = 0;
    for (const ch of s) {
      if (ch === "\t") n += 8 - (n % 8);
      else n += /[\u1100-\u115f\u2e80-\ua4cf\uac00-\ud7a3\uf900-\ufaff\ufe30-\ufe4f\uff00-\uff60\uffe0-\uffe6]|[\u{1f300}-\u{1faff}\u{20000}-\u{3fffd}]/u.test(ch) ? 2 : 1;
    }
    return n;
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="diff" class:full>
  <div class="bar">
    <div class="title">
      {#if target.oldPath && target.oldPath !== target.path}
        <span class="old">{target.oldPath}</span> →
      {/if}
      <span class="path">{target.path}</span>
      {#if target.source === "workdir"}
        <span class="tag">{target.staged ? t("已暂存") : t("未暂存")}</span>
      {:else if target.oid}
        <span class="tag mono">{target.oid.slice(0, 7)}</span>
      {/if}
      <span class="add">+{stats.add}</span>
      <span class="del">-{stats.del}</span>
    </div>
    <div class="actions">
      {#if !embedded}
        <button onclick={() => tab.openFile(target.path, "history")} title={t("查看修改过这个文件的所有提交")}>{t("历史")}</button>
        <button
          onclick={() => tab.openFile(target.path, "blame", target.source === "commit" ? target.oid! : null)}
          title={t("逐行查看是谁在哪次提交里改的")}>Blame</button
        >
      {/if}
      <button onclick={toggleSplit}>{split ? t("上下对比") : t("左右对比")}</button>
      <button class:on={full} onclick={() => (full = !full)} title={full ? t("退出全屏 (Esc)") : t("铺满窗口查看")}>{full ? t("⤡ 还原") : t("⤢ 全屏")}</button>
      {#if !embedded}<button onclick={onClose} title={t("关闭 (Esc)")}>✕</button>{/if}
    </div>
  </div>
  {#if error}
    <div class="msg error">{error}</div>
  {:else if loading && !diff}
    <div class="msg">{t("加载中…")}</div>
  {:else if !diff}
    <div class="msg">{t("没有改动")}</div>
  {:else if showImage}
    <ImageDiff {tab} {target} />
  {:else if diff.binary}
    <div class="msg">{t("二进制文件，无法显示差异")}</div>
  {:else if lfsSizes}
    <div class="msg">
      {t("这是 Git LFS 管理的文件，仓库里只保存了指向它的指针。")}
      {#if lfsSizes.old && lfsSizes.new && lfsSizes.old !== lfsSizes.new}
        <br />{t("文件大小：{0} → {1} 字节", lfsSizes.old, lfsSizes.new)}
      {:else if lfsSizes.new}
        <br />{t("文件大小：{0} 字节", lfsSizes.new)}
      {/if}
    </div>
  {:else if diff.hunks.length === 0}
    <div class="msg">{t("没有文本改动（可能只是文件权限变化）")}</div>
  {:else}
    <div
      class="scroller"
      bind:this={scroller}
      bind:clientHeight={viewH}
      onscroll={() => (scrollTop = scroller!.scrollTop)}
      onwheel={onWheel}
    >
      <div
        class="spacer"
        class:split
        style="height:{items.length * LINE_H + (diff.truncated ? 40 : 0)}px;--hx:{-hx}px"
      >
        <div class="lines" style="transform: translateY({start * LINE_H}px)">
          {#each items.slice(start, end) as it, k (start + k)}
            {#if it.t === "hunk"}
              <div class="hunk" style="height:{LINE_H}px">
                <span class="hh">{it.header}</span>
                {#if canStageHunk}
                  <span class="hunk-actions">
                    {#if sel && sel.h === it.h}
                      {#if target.staged}
                        <button class="primary" onclick={() => linesAction(it.header, "unstage")}>{t("取消暂存选中的 {0} 行", sel.lines.size)}</button>
                      {:else}
                        <button class="primary" onclick={() => linesAction(it.header, "stage")}>{t("暂存选中的 {0} 行", sel.lines.size)}</button>
                        <button class="danger" onclick={() => linesAction(it.header, "discard")}>{t("丢弃选中的行")}</button>
                      {/if}
                      <button onclick={() => (sel = null)}>{t("取消选择")}</button>
                    {:else if target.staged}
                      <button onclick={() => hunkAction(it.header, "unstage")}>{t("取消暂存代码块")}</button>
                    {:else}
                      <button onclick={() => hunkAction(it.header, "stage")}>{t("暂存代码块")}</button>
                      <button class="danger" onclick={() => hunkAction(it.header, "discard")}>{t("丢弃")}</button>
                      <span class="tip">{t("点行号可以只选几行")}</span>
                    {/if}
                  </span>
                {/if}
              </div>
            {:else if it.t === "line"}
              {@const ln = it}
              <div
                class="line k{it.line.kind === '+' ? 'add' : it.line.kind === '-' ? 'del' : 'ctx'}"
                class:picked={isSel(it.h, it.i)}
                style="height:{LINE_H}px"
              >
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <span class="gut" class:pick={canStageHunk && it.line.kind !== " "} onclick={(e) => toggleLine(e, ln.h, ln.i)}>
                  <span class="no">{it.line.old ?? ""}</span>
                  <span class="no">{it.line.new ?? ""}</span>
                </span>
                <span class="sign">{it.line.kind}</span>
                <span class="code">{@html html(it.line)}</span>
              </div>
            {:else}
              {@const pr = it}
              <div class="line pair" style="height:{LINE_H}px">
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div class="half {it.left ? (it.left.kind === '-' ? 'kdel' : 'kctx') : 'kempty'}" class:picked={isSel(it.h, it.li)}>
                  <span class="no" class:pick={canStageHunk && it.left?.kind === "-"} onclick={(e) => toggleLine(e, pr.h, pr.li)}>{it.left?.old ?? ""}</span>
                  <span class="clip"><span class="code shift">{#if it.left}{@html html(it.left)}{/if}</span></span>
                </div>
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div class="half {it.right ? (it.right.kind === '+' ? 'kadd' : 'kctx') : 'kempty'}" class:picked={isSel(it.h, it.ri)}>
                  <span class="no" class:pick={canStageHunk && it.right?.kind === "+"} onclick={(e) => toggleLine(e, pr.h, pr.ri)}>{it.right?.new ?? ""}</span>
                  <span class="clip"><span class="code shift">{#if it.right}{@html html(it.right)}{/if}</span></span>
                </div>
              </div>
            {/if}
          {/each}
        </div>
        {#if diff.truncated && end >= items.length}
          <div class="truncated" style="top:{items.length * LINE_H}px">{t("文件改动太多，只显示了前 20000 行")}</div>
        {/if}
      </div>
    </div>
    {#if split}
      <!-- 左右各一条横向滚动条，同步滚动；行号那一截不滚 -->
      <div class="hbars">
        {#each [0, 1] as i (i)}
          <div class="hside">
            <div class="hbar" bind:this={hbars[i]} onscroll={onHScroll}>
              <div style="width:calc({splitCols}ch + 24px)"></div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .diff {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--bg-graph);
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    height: 38px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
  }
  .path {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .old {
    color: var(--text-dim);
  }
  .tag {
    padding: 1px 6px;
    border-radius: 3px;
    background: var(--bg-hover);
    color: var(--text-dim);
    font-size: 12px;
  }
  .mono {
    font-family: var(--mono);
  }
  .add {
    color: var(--add);
  }
  .del {
    color: var(--del);
  }
  .actions {
    display: flex;
    gap: 6px;
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
  .scroller {
    flex: 1;
    overflow: auto;
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .spacer {
    position: relative;
    min-width: 100%;
    width: max-content;
  }
  .lines {
    will-change: transform;
  }
  .line,
  .hunk {
    display: flex;
    align-items: center;
    white-space: pre;
    line-height: 20px;
  }
  .hunk {
    position: sticky;
    left: 0;
    gap: 12px;
    padding-left: 8px;
    background: var(--bg-hunk);
    color: var(--text-dim);
  }
  .hunk-actions {
    display: flex;
    gap: 6px;
    font-family: var(--sans);
  }
  .hunk-actions button {
    height: 18px;
    padding: 0 8px;
    font-size: 11.5px;
  }
  .no {
    display: inline-block;
    width: 48px;
    padding-right: 8px;
    text-align: right;
    color: var(--text-faint);
    flex-shrink: 0;
    user-select: none;
  }
  .sign {
    width: 16px;
    flex-shrink: 0;
    color: var(--text-faint);
    user-select: none;
  }
  .code {
    padding-right: 24px;
  }
  .kadd {
    background: var(--bg-add);
  }
  .kdel {
    background: var(--bg-del);
  }
  .kadd :global(.chg) {
    border-radius: 2px;
    background: var(--bg-add-strong);
  }
  .kdel :global(.chg) {
    border-radius: 2px;
    background: var(--bg-del-strong);
  }
  .gut {
    display: flex;
    flex-shrink: 0;
  }
  .pick {
    cursor: pointer;
  }
  .pick:hover,
  .gut.pick:hover .no {
    color: var(--text);
    background: var(--bg-hover);
  }
  /* 选中的行：左边一条强调色竖线 */
  .picked {
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .picked .no {
    color: var(--accent);
    font-weight: 600;
  }
  .tip {
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .kempty {
    background: var(--bg-empty);
  }
  .pair {
    width: 100%;
  }
  .spacer.split {
    width: 100%;
  }
  /* 代码往左滚时不压到行号上 */
  .clip {
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }
  .shift {
    display: inline-block;
    transform: translateX(var(--hx));
  }
  .hbars {
    display: flex;
    flex-shrink: 0;
    border-top: 1px solid var(--border);
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .hside {
    flex: 1 1 0;
    min-width: 0;
    padding-left: 48px;
  }
  .hside + .hside {
    border-left: 1px solid var(--border);
  }
  .hbar {
    overflow-x: auto;
    overflow-y: hidden;
  }
  .hbar > div {
    height: 1px;
  }
  .diff.full {
    position: fixed;
    top: 38px;
    right: 0;
    bottom: 0;
    left: 0;
    z-index: 50;
    box-shadow: var(--shadow);
  }
  .half {
    display: flex;
    flex: 1 1 0;
    min-width: 0;
    height: 100%;
    overflow: hidden;
  }
  .half + .half {
    border-left: 1px solid var(--border);
  }
  .truncated {
    position: absolute;
    left: 0;
    right: 0;
    padding: 10px;
    text-align: center;
    color: var(--warn);
    font-family: var(--sans);
  }
</style>
