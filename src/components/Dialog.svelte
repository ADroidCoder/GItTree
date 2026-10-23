<script lang="ts">
  import { t } from "../lib/i18n";
  import { app } from "../lib/store.svelte";

  const d = $derived(app.dialog!);
  let input = $state("");
  let checkbox = $state(false);
  let choice = $state("");
  const showInput = $derived(!!d.input && (!d.inputWhen || d.inputWhen.includes(choice)));
  let inputEl = $state<HTMLInputElement | HTMLTextAreaElement>();
  let confirmEl = $state<HTMLButtonElement>();

  $effect(() => {
    input = d.input?.value ?? "";
    checkbox = d.checkbox?.value ?? false;
    choice = d.choices?.value ?? "";
    // 打开后聚焦输入框，没有输入框时聚焦确认按钮
    queueMicrotask(() => (inputEl ?? confirmEl)?.focus());
  });

  function close(ok: boolean) {
    const spec = app.dialog;
    app.dialog = null;
    spec?.resolve(ok ? { input, checkbox, choice } : null);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close(false);
    if (e.key === "Enter" && !e.isComposing && !(e.target instanceof HTMLTextAreaElement)) {
      e.preventDefault();
      close(true);
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={onKey}>
  <div class="dialog" class:wide={d.input?.code} role="dialog" aria-modal="true">
    <div class="title">{d.title}</div>
    {#if d.message}<div class="message">{d.message}</div>{/if}
    {#if d.choices}
      <div class="choices">
        {#each d.choices.options as o (o.value)}
          <label class="choice" class:on={choice === o.value}>
            <input type="radio" bind:group={choice} value={o.value} />
            <span>
              <span class="label">{o.label}</span>
              {#if o.hint}<span class="hint">{o.hint}</span>{/if}
            </span>
          </label>
        {/each}
      </div>
    {/if}
    {#if d.input && showInput}
      {#if d.input.multiline}
        <textarea
          bind:this={inputEl}
          bind:value={input}
          class:code={d.input.code}
          placeholder={d.input.placeholder}
          rows={d.input.code ? 14 : 4}
          spellcheck={d.input.code ? false : undefined}
        ></textarea>
      {:else}
        <input bind:this={inputEl} bind:value={input} placeholder={d.input.placeholder} spellcheck="false" />
      {/if}
    {/if}
    {#if d.checkbox}
      <label class="check"><input type="checkbox" bind:checked={checkbox} /> {d.checkbox.label}</label>
    {/if}
    <div class="buttons">
      <button onclick={() => close(false)}>{t("取消")}</button>
      <button bind:this={confirmEl} class={d.danger ? "danger-solid" : "primary"} onclick={() => close(true)}>
        {d.confirmText ?? t("确定")}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 15vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog.wide {
    width: 680px;
  }
  textarea.code {
    font-family: var(--mono);
    font-size: 12px;
    white-space: pre;
    overflow-wrap: normal;
  }
  .dialog {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 440px;
    max-width: calc(100vw - 32px);
    padding: 18px 20px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
  }
  .title {
    font-size: 15px;
    font-weight: 600;
  }
  .message {
    color: var(--text-dim);
    font-size: 13px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .choice {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 6px 8px;
    border: 1px solid transparent;
    border-radius: 6px;
    cursor: pointer;
  }
  .choice:hover {
    background: var(--bg-hover);
  }
  .choice.on {
    border-color: var(--border);
    background: var(--bg-hover);
  }
  .choice input {
    margin-top: 3px;
  }
  .choice .label {
    display: block;
    font-size: 13px;
  }
  .choice .hint {
    display: block;
    color: var(--text-faint);
    font-size: 12px;
    line-height: 1.45;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
</style>
