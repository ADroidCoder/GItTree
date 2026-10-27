<script lang="ts">
  import { t } from "../lib/i18n";
  import { api, type CommitDetails, type Signature } from "../lib/api";
  import { errorText, openIn, rewordHead, toast, type RepoTab } from "../lib/store.svelte";
  import { avatarColor, formatDate, initials } from "../lib/util";
  import FileList from "./FileList.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  let { tab, oid }: { tab: RepoTab; oid: string } = $props();

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  // 提交内容不会变，按哈希缓存
  const cache = new Map<string, CommitDetails>();
  let details = $state<CommitDetails | null>(null);
  let error = $state<string | null>(null);
  let treeView = $state(false);

  $effect(() => {
    const id = oid;
    const hit = cache.get(id);
    if (hit) {
      details = hit;
      error = null;
      return;
    }
    let cancelled = false;
    // 切换太快时不要每一行都去请求
    const timer = setTimeout(() => {
      api
        .commitDetails(tab.id, id)
        .then((d) => {
          if (cache.size > 200) cache.clear();
          cache.set(id, d);
          if (!cancelled) {
            details = d;
            error = null;
          }
        })
        .catch((e) => {
          if (!cancelled) error = errorText(e);
        });
    }, 30);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  // 签名验证要启动 gpg / ssh-keygen，单独异步读，不拖慢详情
  const sigCache = new Map<string, Signature | null>();
  let sig = $state<Signature | null>(null);
  $effect(() => {
    const id = oid;
    if (sigCache.has(id)) {
      sig = sigCache.get(id)!;
      return;
    }
    sig = null;
    let cancelled = false;
    const timer = setTimeout(() => {
      api
        .commitSignature(tab.id, id)
        .then((s) => {
          if (sigCache.size > 200) sigCache.clear();
          sigCache.set(id, s);
          if (!cancelled) sig = s;
        })
        .catch(() => {});
    }, 120);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  /** 签名状态的说明和颜色 */
  const sigInfo = $derived.by(() => {
    if (!sig) return null;
    const kind = sig.format === "ssh" ? "SSH" : "GPG";
    const who = sig.signer ? ` · ${sig.signer}` : "";
    switch (sig.status) {
      case "G":
        return { cls: "ok", text: t("✓ {0} 签名有效{1}", kind, who) };
      case "U":
        return { cls: "ok", text: t("✓ {0} 签名有效，但密钥没有被设为受信任{1}", kind, who) };
      case "X":
      case "Y":
        return { cls: "warn", text: t("⚠ {0} 签名或密钥已过期{1}", kind, who) };
      case "R":
        return { cls: "bad", text: t("✗ {0} 签名的密钥已被吊销{1}", kind, who) };
      case "B":
        return { cls: "bad", text: t("✗ {0} 签名无效：提交内容和签名对不上", kind) };
      default:
        return {
          cls: "warn",
          text:
            sig.format === "ssh"
              ? t("? 有 SSH 签名，但无法验证（需要设置 gpg.ssh.allowedSignersFile）")
              : t("? 有 GPG 签名，但无法验证（本机没有签名者的公钥{0}）", sig.key ? ` ${sig.key}` : ""),
        };
    }
  });

  const subject = $derived(details?.message.split("\n")[0] ?? "");
  const body = $derived(details?.message.split("\n").slice(1).join("\n").trim() ?? "");
  /** 最新的提交可以双击说明直接改 */
  const editable = $derived(oid === tab.repo?.head.oid);
  const totals = $derived.by(() => {
    let a = 0;
    let d = 0;
    for (const f of details?.files ?? []) {
      a += f.additions;
      d += f.deletions;
    }
    return { a, d };
  });

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast(t("已复制"));
    } catch {
      toast(t("复制失败"), "error");
    }
  }
</script>

<div class="panel">
  {#if error}
    <div class="error">{error}</div>
  {:else if details}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="head"
      class:editable
      title={editable ? t("双击修改提交说明") : undefined}
      ondblclick={() => editable && (getSelection()?.removeAllRanges(), rewordHead(tab))}
    >
      <div class="subject">{subject}</div>
      {#if body}<pre class="body">{body}</pre>{/if}
    </div>
    <div class="meta">
      <div class="person">
        <span class="avatar" style="background:{avatarColor(details.author.email)}">{initials(details.author.name)}</span>
        <div>
          <div class="pname">{details.author.name} <span class="email">&lt;{details.author.email}&gt;</span></div>
          <div class="when">{t("创作于 {0}", formatDate(details.author.time))}</div>
          {#if details.committer.name !== details.author.name || details.committer.time !== details.author.time}
            <div class="when">{t("由 {0} 提交于 {1}", details.committer.name, formatDate(details.committer.time))}</div>
          {/if}
        </div>
      </div>
      <div class="ids">
        <div>
          <span class="label">{t("提交")}</span>
          <button class="link mono" onclick={() => copy(details!.oid)} title={t("点击复制")}>{details.oid.slice(0, 10)}</button>
        </div>
        {#if details.parents.length}
          <div>
            <span class="label">{details.parents.length > 1 ? t("父提交（合并）") : t("父提交")}</span>
            {#each details.parents as p (p)}
              <button class="link mono" onclick={() => tab.jumpToOid(p)}>{p.slice(0, 7)}</button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
    {#if sigInfo}
      <div class="sig {sigInfo.cls}" title={sig?.key ? t("密钥 {0}", sig.key) : ""}>{sigInfo.text}</div>
    {/if}
    <div class="files-head">
      <span>{t("{0} 个文件改动", details.files.length)}</span>
      <span class="add">+{totals.a}</span>
      <span class="del">-{totals.d}</span>
      <button class="view-toggle" onclick={() => (treeView = !treeView)}>{treeView ? t("列表") : t("目录树")}</button>
    </div>
    <div class="files">
      <FileList
        files={details.files.map((f) => ({
          key: f.path,
          path: f.path,
          oldPath: f.old_path,
          status: f.status,
          additions: f.additions,
          deletions: f.deletions,
          binary: f.binary,
        }))}
        tree={treeView}
        active={tab.diff?.source === "commit" && tab.diff.oid === details.oid ? tab.diff.path : null}
        onOpen={(f) => (tab.diff = { source: "commit", oid: details!.oid, path: f.path, oldPath: f.oldPath })}
        onContext={(e, f) => {
          const oid = details!.oid;
          menu = {
            x: e.clientX,
            y: e.clientY,
            items: [
              { label: t("查看改动"), action: () => (tab.diff = { source: "commit", oid, path: f.path, oldPath: f.oldPath }) },
              "sep",
              { label: t("文件历史"), action: () => tab.openFile(f.path, "history") },
              { label: t("Blame 这个版本"), disabled: f.status === "D", action: () => tab.openFile(f.path, "blame", oid) },
              { label: t("用编辑器打开（工作区里的版本）"), action: () => openIn(tab, "editor", f.path) },
              "sep",
              { label: t("复制路径"), action: () => navigator.clipboard.writeText(f.path) },
            ],
          };
        }}
      />
    </div>
  {:else}
    <div class="loading">{t("加载中…")}</div>
  {/if}
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .head {
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--border);
  }
  .head.editable:hover {
    background: var(--bg-hover);
  }
  .subject {
    font-size: 15px;
    font-weight: 600;
    line-height: 1.4;
    word-break: break-word;
  }
  .body {
    margin: 8px 0 0;
    max-height: 180px;
    overflow: auto;
    color: var(--text-dim);
    font-family: var(--sans);
    font-size: 13px;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .meta {
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    font-size: 12.5px;
  }
  .person {
    display: flex;
    gap: 10px;
  }
  .avatar {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    flex-shrink: 0;
    border-radius: 50%;
    color: #fff;
    font-weight: 600;
  }
  .pname {
    font-weight: 600;
  }
  .email,
  .when {
    color: var(--text-dim);
    font-weight: normal;
  }
  .ids {
    margin-top: 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .label {
    display: inline-block;
    min-width: 84px;
    color: var(--text-dim);
  }
  .link {
    padding: 0 4px;
    border: 0;
    background: none;
    color: var(--accent);
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
  }
  .mono {
    font-family: var(--mono);
    font-size: 12px;
  }
  .files-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    color: var(--text-dim);
    font-size: 12.5px;
  }
  .view-toggle {
    margin-left: auto;
    height: 22px;
    font-size: 12px;
  }
  .add {
    color: var(--add);
  }
  .del {
    color: var(--del);
  }
  .files {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .loading,
  .error {
    padding: 30px;
    text-align: center;
    color: var(--text-dim);
  }
  .error {
    color: var(--del);
  }
  .sig {
    margin: 0 14px 8px;
    padding: 4px 8px;
    border-radius: 5px;
    background: var(--bg-hover);
    font-size: 12px;
  }
  .sig.ok {
    color: var(--add);
  }
  .sig.warn {
    color: var(--warn);
  }
  .sig.bad {
    color: var(--del);
  }
</style>
