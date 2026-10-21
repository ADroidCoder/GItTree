<script lang="ts">
  import { t } from "../lib/i18n";
  import { api } from "../lib/api";
  import { app } from "../lib/store.svelte";

  /** 一次只问一个；git 会先问用户名、再问密码 */
  const req = $derived(app.askpass[0]);

  /** 从提示里认出要问什么 */
  const kind = $derived.by(() => {
    const p = req.prompt;
    if (/\(yes\/no/i.test(p)) return "confirm";
    if (/^username/i.test(p)) return "username";
    if (/passphrase/i.test(p)) return "passphrase";
    if (/password|PIN|token/i.test(p)) return "password";
    return "text";
  });
  /** 提示里的地址，例如 https://github.com */
  const host = $derived(req.prompt.match(/'([^']+)'/)?.[1] ?? "");
  const isGithub = $derived(/github\.com/i.test(host));
  const isGitee = $derived(/gitee\.com/i.test(host));

  let value = $state("");
  let input = $state<HTMLInputElement>();

  $effect(() => {
    void req.id;
    value = "";
    input?.focus();
  });

  function reply(answer: string | null) {
    api.askpassReply(req.id, answer);
    app.askpass = app.askpass.slice(1);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") reply(null);
    if (e.key === "Enter" && !e.isComposing && kind !== "confirm") reply(value);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={onKey}>
  <div class="dialog" role="dialog" aria-modal="true">
    {#if kind === "confirm"}
      <div class="title">{t("确认连接")}</div>
      <pre class="prompt">{req.prompt}</pre>
      <div class="hint">{t("第一次连接这台服务器时 SSH 会让你确认它的身份。如果是你常用的代码托管网站（如 GitHub、Gitee），一般选“信任”即可。")}</div>
      <div class="buttons">
        <button onclick={() => reply("no")}>{t("不信任")}</button>
        <button class="primary" onclick={() => reply("yes")}>{t("信任并继续")}</button>
      </div>
    {:else}
      <div class="title">
        {kind === "username" ? t("需要登录") : kind === "passphrase" ? t("输入 SSH 密钥密码") : kind === "password" ? t("需要密码") : t("需要输入")}
      </div>
      <div class="prompt-line">{host ? t("正在连接 {0}", host) : req.prompt}</div>
      <label for="askpass-input">
        {kind === "username" ? t("用户名") : kind === "passphrase" ? t("密钥密码") : kind === "password" ? (isGithub ? "Token" : t("密码或令牌")) : req.prompt}
      </label>
      <input
        id="askpass-input"
        bind:this={input}
        bind:value
        type={kind === "username" || kind === "text" ? "text" : "password"}
        autocomplete="off"
        spellcheck="false"
      />
      {#if kind === "password" && isGithub}
        <div class="hint">
          {t("GitHub 已经不支持用账号密码推送，请填写 Personal Access Token。在 GitHub → Settings → Developer settings → Personal access tokens 里生成，至少需要仓库的 Contents 读写权限。")}
        </div>
      {:else if kind === "password" && isGitee}
        <div class="hint">{t("可以填 Gitee 登录密码，也可以填在 Gitee → 设置 → 私人令牌 里生成的令牌。")}</div>
      {:else if kind === "passphrase"}
        <div class="hint">{req.prompt}</div>
      {/if}
      {#if kind !== "text"}<div class="hint">{t("验证成功后 git 会用系统的凭据管理器记住，下次不用再填。")}</div>{/if}
      <div class="buttons">
        <button onclick={() => reply(null)}>{t("取消")}</button>
        <button class="primary" onclick={() => reply(value)}>{t("确定")}</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 150;
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
  .prompt-line {
    color: var(--text-dim);
    font-size: 12.5px;
    word-break: break-all;
  }
  label {
    color: var(--text-dim);
    font-size: 12.5px;
  }
  .prompt {
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12px;
    white-space: pre-wrap;
    word-break: break-all;
  }
  .hint {
    color: var(--text-faint);
    font-size: 12px;
    word-break: break-all;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
</style>
