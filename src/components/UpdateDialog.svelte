<script lang="ts">
  import { t } from "../lib/i18n";
  import { installUpdate, updater } from "../lib/updater.svelte";

  const u = $derived(updater.update!);
  const busy = $derived(updater.status === "downloading" || updater.status === "installing");
  const percent = $derived(updater.total ? Math.min(100, Math.round((updater.downloaded / updater.total) * 100)) : 0);
  const mb = (n: number) => (n / 1024 / 1024).toFixed(1);

  function close() {
    if (!busy) updater.open = false;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<div class="overlay">
  <div class="dialog" role="dialog" aria-modal="true">
    <div class="title">{t("发现新版本 {0}", u.version)}</div>
    <div class="sub">{t("当前版本 {0}", updater.version)}</div>
    {#if u.body}<pre class="notes">{u.body}</pre>{/if}

    {#if updater.status === "downloading"}
      <div class="bar"><span style="width:{percent}%"></span></div>
      <div class="sub">
        {updater.total ? t("正在下载… {0} / {1} MB", mb(updater.downloaded), mb(updater.total)) : t("正在下载…")}
      </div>
    {:else if updater.status === "installing"}
      <div class="sub">{t("正在安装，完成后会自动重启…")}</div>
    {:else if updater.status === "error"}
      <div class="error">{t("更新失败：{0}", updater.error)}</div>
    {/if}

    <div class="buttons">
      <button disabled={busy} onclick={close}>{t("以后再说")}</button>
      <button class="primary" disabled={busy} onclick={installUpdate}>
        {updater.status === "error" ? t("重试") : t("立即更新并重启")}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 140;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 14vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 480px;
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
  .sub {
    color: var(--text-dim);
    font-size: 12.5px;
  }
  .notes {
    max-height: 40vh;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
    font-family: inherit;
    font-size: 12.5px;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--bg-hover);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .error {
    color: var(--del);
    font-size: 12.5px;
    word-break: break-all;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
</style>
