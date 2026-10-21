<script lang="ts">
  import { t } from "../lib/i18n";
  import { api, type Choice, type ConflictFile } from "../lib/api";
  import { confirm, errorText, type RepoTab } from "../lib/store.svelte";

  let { tab, path, onClose }: { tab: RepoTab; path: string; onClose: () => void } = $props();

  let file = $state<ConflictFile | null>(null);
  let error = $state<string | null>(null);
  /** 每一处冲突的选择，null 表示还没选 */
  let choices = $state<(Choice | null)[]>([]);
  /** 正在手动编辑的冲突 */
  let editing = $state<Set<number>>(new Set());
  let saving = $state(false);

  $effect(() => {
    const p = path;
    let cancelled = false;
    file = null;
    error = null;
    api
      .conflictFile(tab.id, p)
      .then((f) => {
        if (cancelled) return;
        file = f;
        choices = Array(f.conflicts).fill(null);
        editing = new Set();
      })
      .catch((e) => !cancelled && (error = errorText(e)));
    return () => (cancelled = true);
  });

  /** 给每个冲突块编上序号 */
  const blocks = $derived.by(() => {
    let n = 0;
    return (file?.chunks ?? []).map((c) => ({ c, idx: c.kind === "conflict" ? n++ : -1 }));
  });
  const done = $derived(choices.filter(Boolean).length);
  const rebasing = $derived(tab.status?.in_progress === "rebase");

  function resultOf(i: number): string {
    const b = blocks.find((x) => x.idx === i)?.c;
    const ch = choices[i];
    if (!b || b.kind !== "conflict" || !ch) return "";
    switch (ch.pick) {
      case "ours":
        return b.ours;
      case "theirs":
        return b.theirs;
      case "base":
        return b.base ?? "";
      case "ours_theirs":
        return b.ours + b.theirs;
      case "theirs_ours":
        return b.theirs + b.ours;
      case "text":
        return ch.text;
    }
  }

  function pick(i: number, ch: Choice) {
    choices[i] = ch;
    const next = new Set(editing);
    next.delete(i);
    editing = next;
  }

  function edit(i: number) {
    choices[i] = { pick: "text", text: resultOf(i) || (blocks.find((x) => x.idx === i)?.c as { ours: string }).ours };
    editing = new Set(editing).add(i);
  }

  /** 还没选的冲突都用同一边 */
  function pickAll(p: "ours" | "theirs") {
    choices = choices.map((c) => c ?? { pick: p });
  }

  async function save() {
    if (!file || done < file.conflicts) return;
    saving = true;
    const ok = await tab.run({ op: "resolve_conflict", path, choices: choices as Choice[] }, t("正在保存…"));
    saving = false;
    if (ok) onClose();
  }

  async function useSide(theirs: boolean) {
    const exists = theirs ? file?.theirs_exists : file?.ours_exists;
    const name = theirs ? t("传入") : t("当前");
    if (!exists && !(await confirm(t("删除文件"), t("{0}一侧已经删除了这个文件，使用{1}的版本会删除 {2}。", name, name, path), t("删除"), true))) return;
    if (await tab.run({ op: "resolve_side", path, theirs }, t("正在解决冲突…"))) onClose();
  }

  /** 普通内容太长时只显示开头和结尾几行 */
  function preview(text: string) {
    const lines = text.replace(/\n$/, "").split("\n");
    if (lines.length <= 8) return { head: lines.join("\n"), hidden: 0, tail: "" };
    return { head: lines.slice(0, 3).join("\n"), hidden: lines.length - 6, tail: lines.slice(-3).join("\n") };
  }
  let expanded = $state<Set<number>>(new Set());

  const show = (t: string) => t.replace(/\n$/, "") || " ";
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !editing.size && onClose()} />

<div class="cv">
  <div class="bar">
    <div class="title">
      <span class="badge">{t("冲突")}</span>
      <span class="path">{path}</span>
      {#if file?.conflicts}<span class="count">{t("{0} / {1} 处已选择", done, file.conflicts)}</span>{/if}
    </div>
    <div class="actions">
      <button onclick={() => useSide(false)} title={t("整个文件使用当前分支的版本")}>{t("全部用当前")}</button>
      <button onclick={() => useSide(true)} title={t("整个文件使用传入的版本")}>{t("全部用传入")}</button>
      <button onclick={onClose} title={t("关闭 (Esc)")}>✕</button>
    </div>
  </div>
  <div class="hint">
    {#if rebasing}
      {t("变基时，“当前”是你要变基到的目标分支上的内容，“传入”是你自己正在重放的提交。")}
    {:else}
      {t("“当前”是你所在分支（HEAD）的内容，“传入”是要合并进来的内容。")}
    {/if}
    {t("每一处冲突选一个结果，全部选完后点“保存并标记为已解决”。")}
  </div>

  {#if error}
    <div class="msg error">{error}</div>
  {:else if !file}
    <div class="msg">{t("加载中…")}</div>
  {:else if file.binary || file.conflicts === 0}
    <div class="msg">
      {#if file.binary}
        {t("二进制文件无法逐处合并，请选择整个文件使用哪一边的版本。")}
      {:else if !file.ours_exists || !file.theirs_exists}
        {t("一边修改了这个文件，另一边把它删除了。请选择保留修改后的文件，或者删除它。")}
      {:else}
        {t("文件里已经没有冲突标记了。如果你已经在编辑器里改好，可以直接标记为已解决。")}
      {/if}
      <div class="side-btns">
        <button onclick={() => useSide(false)}>{file.ours_exists ? t("使用当前的版本") : t("删除文件（当前一侧已删除）")}</button>
        <button onclick={() => useSide(true)}>{file.theirs_exists ? t("使用传入的版本") : t("删除文件（传入一侧已删除）")}</button>
        {#if !file.binary && file.ours_exists && file.theirs_exists}
          <button class="primary" onclick={() => tab.run({ op: "stage", paths: [path] }).then((ok) => ok && onClose())}>{t("标记为已解决")}</button>
        {/if}
      </div>
    </div>
  {:else}
    <div class="body">
      {#each blocks as { c, idx }, k (k)}
        {#if c.kind === "common"}
          {@const p = preview(c.text)}
          {#if p.hidden && !expanded.has(k)}
            <pre class="common">{p.head}</pre>
            <button class="fold" onclick={() => (expanded = new Set(expanded).add(k))}>{t("… {0} 行相同内容，点击展开 …", p.hidden)}</button>
            <pre class="common">{p.tail}</pre>
          {:else}
            <pre class="common">{show(c.text)}</pre>
          {/if}
        {:else}
          <div class="conflict" class:resolved={!!choices[idx]}>
            <div class="chead">
              <span>{t("冲突 {0}", idx + 1)}</span>
              {#if choices[idx]}<span class="ok">{t("✓ 已选择")}</span>{/if}
              <span class="grow"></span>
              {#if choices[idx]}<button class="small" onclick={() => (choices[idx] = null)}>{t("重新选择")}</button>{/if}
            </div>
            {#if choices[idx] && !editing.has(idx)}
              <pre class="result">{show(resultOf(idx))}</pre>
              {#if file.utf8}<button class="small edit" onclick={() => edit(idx)}>{t("手动编辑")}</button>{/if}
            {:else if editing.has(idx) && choices[idx]?.pick === "text"}
              <textarea
                class="editor"
                rows={Math.min(20, Math.max(4, resultOf(idx).split("\n").length + 1))}
                value={resultOf(idx)}
                oninput={(e) => (choices[idx] = { pick: "text", text: (e.target as HTMLTextAreaElement).value })}
              ></textarea>
              <div class="erow">
                <button class="small" onclick={() => (editing = new Set([...editing].filter((x) => x !== idx)))}>{t("完成编辑")}</button>
              </div>
            {:else}
              <div class="sides">
                <div class="side ours">
                  <div class="slabel">{t("当前")} <span class="ref">{c.ours_label}</span></div>
                  <pre>{show(c.ours)}</pre>
                  <button class="primary small" onclick={() => pick(idx, { pick: "ours" })}>{t("用当前")}</button>
                </div>
                <div class="side theirs">
                  <div class="slabel">{t("传入")} <span class="ref">{c.theirs_label}</span></div>
                  <pre>{show(c.theirs)}</pre>
                  <button class="primary small" onclick={() => pick(idx, { pick: "theirs" })}>{t("用传入")}</button>
                </div>
              </div>
              <div class="more">
                <button class="small" onclick={() => pick(idx, { pick: "ours_theirs" })}>{t("两边都要（当前在前）")}</button>
                <button class="small" onclick={() => pick(idx, { pick: "theirs_ours" })}>{t("两边都要（传入在前）")}</button>
                {#if c.base !== null}<button class="small" onclick={() => pick(idx, { pick: "base" })}>{t("用修改前的")}</button>{/if}
                {#if file.utf8}<button class="small" onclick={() => edit(idx)}>{t("手动编辑")}</button>{/if}
              </div>
            {/if}
          </div>
        {/if}
      {/each}
    </div>
    <div class="footer">
      <button class="small" onclick={() => pickAll("ours")}>{t("剩下的都用当前")}</button>
      <button class="small" onclick={() => pickAll("theirs")}>{t("剩下的都用传入")}</button>
      <span class="grow"></span>
      {#if !file.utf8}<span class="note">{t("非 UTF-8 文件，不能手动编辑")}</span>{/if}
      <button class="primary" disabled={saving || done < file.conflicts} onclick={save}>
        {t("保存并标记为已解决{0}", done < file.conflicts ? t("（还有 {0} 处）", file.conflicts - done) : "")}
      </button>
    </div>
  {/if}
</div>

<style>
  .cv {
    display: flex;
    flex-direction: column;
    height: 100%;
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
    overflow: hidden;
    white-space: nowrap;
  }
  .badge {
    padding: 1px 6px;
    border-radius: 3px;
    background: var(--del);
    color: #fff;
    font-size: 11px;
  }
  .path {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .count {
    color: var(--text-dim);
    font-size: 12px;
  }
  .actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .hint {
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    color: var(--text-dim);
    font-size: 12px;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 10px 14px 20px;
  }
  pre {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12.5px;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-all;
    user-select: text;
  }
  .common {
    padding: 0 8px;
    color: var(--text-dim);
  }
  .fold {
    display: block;
    width: 100%;
    height: 22px;
    margin: 2px 0;
    border: 0;
    background: var(--bg-hover);
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .conflict {
    margin: 8px 0;
    border: 1px solid var(--del);
    border-radius: 6px;
    overflow: hidden;
  }
  .conflict.resolved {
    border-color: var(--add);
  }
  .chead {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 8px 0 10px;
    background: var(--bg-hover);
    font-size: 12px;
    font-weight: 600;
  }
  .ok {
    color: var(--add);
  }
  .grow {
    flex: 1;
  }
  .sides {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    padding: 8px 10px;
  }
  .side.ours {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    border-right: 1px solid var(--border);
  }
  .side.theirs {
    background: color-mix(in srgb, var(--add) 10%, transparent);
  }
  .side pre {
    flex: 1;
  }
  .side button {
    align-self: flex-start;
  }
  .slabel {
    color: var(--text-dim);
    font-size: 11.5px;
    font-weight: 600;
  }
  .ref {
    margin-left: 4px;
    color: var(--text-faint);
    font-weight: normal;
  }
  .more {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 8px 10px;
    border-top: 1px solid var(--border);
  }
  .result {
    padding: 8px 10px;
    background: color-mix(in srgb, var(--add) 8%, transparent);
  }
  .edit {
    margin: 0 10px 8px;
  }
  .editor {
    display: block;
    width: calc(100% - 20px);
    margin: 8px 10px 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12.5px;
    resize: vertical;
  }
  .erow {
    padding: 6px 10px 8px;
  }
  .footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .note {
    color: var(--text-faint);
    font-size: 12px;
  }
  .small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }
  .msg {
    padding: 40px 30px;
    text-align: center;
    color: var(--text-dim);
  }
  .msg.error {
    color: var(--del);
  }
  .side-btns {
    display: flex;
    justify-content: center;
    gap: 8px;
    margin-top: 16px;
  }
</style>
