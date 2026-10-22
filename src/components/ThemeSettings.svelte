<script lang="ts">
  import { t } from "../lib/i18n";
  import { confirm, dialog, errorText, toast } from "../lib/store.svelte";
  import {
    COLOR_KEYS,
    DEFAULT_LANES,
    PRESETS,
    customChanged,
    deleteCustom,
    effectiveTheme,
    exportTheme,
    importTheme,
    isHex,
    newCustom,
    setTheme,
    themes,
    type Base,
    type ThemeDef,
  } from "../lib/themes.svelte";
  import ThemePreview from "./ThemePreview.svelte";

  const all = $derived([...PRESETS, ...themes.custom]);
  /** 正在编辑的自定义主题（当前用的是自定义主题时） */
  const editing = $derived(themes.custom.find((c) => c.id === themes.current));
  const shown = $derived.by(() => {
    void themes.version; // 跟随系统时系统切换深浅色
    return editing ?? effectiveTheme();
  });

  function swatch(d: ThemeDef) {
    const c = d.colors;
    return `background: linear-gradient(90deg, ${c.tabbar} 0 18%, ${c.side} 18% 42%, ${c.graph} 42% 100%)`;
  }

  /** 改了一个颜色：十六进制格式不对时先不套用，等输完 */
  function changed() {
    if (editing && COLOR_KEYS.every(({ key }) => isHex(editing.colors[key])) && editing.colors.lanes.every(isHex)) customChanged(editing.id);
  }

  function setBase(b: Base) {
    if (!editing) return;
    editing.base = b;
    customChanged(editing.id);
  }

  async function remove() {
    if (!editing) return;
    if (await confirm(t("删除主题"), t("确定删除主题「{0}」吗？", editing.name), t("删除"), true)) deleteCustom(editing.id);
  }

  async function exportIt() {
    if (!editing) return;
    try {
      await navigator.clipboard.writeText(exportTheme(editing));
      toast(t("主题已复制到剪贴板，可以发给别人导入"));
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  async function importIt() {
    const r = await dialog({
      title: t("导入主题"),
      message: t("粘贴别人导出的主题 JSON"),
      input: { value: "", placeholder: '{ "name": "...", "base": "dark", "colors": { ... } }', multiline: true },
      confirmText: t("导入"),
    });
    if (!r?.input.trim()) return;
    try {
      importTheme(r.input);
    } catch (e) {
      toast(errorText(e), "error");
    }
  }
</script>

<div class="themes">
  <div class="cards">
    <button class="card" class:on={themes.current === "system"} onclick={() => setTheme("system")}>
      <span class="sw" style="background: linear-gradient(135deg, {PRESETS[1].colors.graph} 0 50%, {PRESETS[0].colors.graph} 50% 100%)"></span>
      <span class="name">{t("跟随系统")}</span>
    </button>
    {#each all as d (d.id)}
      <button class="card" class:on={themes.current === d.id} onclick={() => setTheme(d.id)} title={d.name}>
        <span class="sw" style={swatch(d)}>
          <span class="dot" style="background:{d.colors.accent}"></span>
          {#each d.colors.lanes.slice(0, 4) as l, i (i)}<span class="lane" style="background:{l}"></span>{/each}
        </span>
        <span class="name">{d.name}</span>
      </button>
    {/each}
  </div>
  <div class="ops">
    <button onclick={() => newCustom(effectiveTheme())}>{t("＋ 基于当前主题新建")}</button>
    <button onclick={importIt}>{t("导入…")}</button>
  </div>

  <ThemePreview def={shown} />

  {#if editing}
    <div class="edit">
      <div class="erow">
        <input class="tname" bind:value={editing.name} onchange={() => customChanged(editing.id)} placeholder={t("主题名称")} />
        <div class="seg">
          <button class:on={editing.base === "dark"} onclick={() => setBase("dark")}>{t("深色底")}</button>
          <button class:on={editing.base === "light"} onclick={() => setBase("light")}>{t("浅色底")}</button>
        </div>
        <span class="grow"></span>
        <button onclick={exportIt}>{t("导出")}</button>
        <button class="danger" onclick={remove}>{t("删除")}</button>
      </div>
      <div class="hint">{t("改动立即生效并自动保存。底色深浅决定语法高亮和阴影的配色。")}</div>

      <div class="colors">
        {#each COLOR_KEYS as c (c.key)}
          <label class="color">
            <input type="color" bind:value={editing.colors[c.key]} oninput={changed} />
            <span class="clabel">{c.label}</span>
            <input class="hex" class:bad={!isHex(editing.colors[c.key])} bind:value={editing.colors[c.key]} oninput={changed} spellcheck="false" />
          </label>
        {/each}
      </div>

      <div class="lanes-head">
        <span>{t("分支线颜色")}</span>
        <button class="small" onclick={() => ((editing.colors.lanes = [...DEFAULT_LANES]), changed())}>{t("恢复默认")}</button>
      </div>
      <div class="lanes">
        {#each editing.colors.lanes as _, i (i)}
          <input type="color" bind:value={editing.colors.lanes[i]} oninput={changed} title={t("第 {0} 种", i + 1)} />
        {/each}
      </div>
    </div>
  {:else}
    <div class="hint">{t("内置主题不能直接修改。想调整颜色，点「基于当前主题新建」复制一份再改。")}</div>
  {/if}
</div>

<style>
  .themes {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px 0;
    border-bottom: 1px solid var(--border);
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: 8px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    height: auto;
    padding: 6px;
    border-radius: 8px;
    text-align: left;
  }
  .card.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .sw {
    position: relative;
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 40px;
    padding: 5px;
    border: 1px solid var(--border);
    border-radius: 5px;
  }
  .dot {
    position: absolute;
    top: 5px;
    right: 5px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .lane {
    width: 4px;
    height: 16px;
    border-radius: 2px;
  }
  .lane:first-of-type {
    margin-left: auto;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }
  .ops {
    display: flex;
    gap: 8px;
  }
  .edit {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .erow {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tname {
    width: 180px;
  }
  .grow {
    flex: 1;
  }
  .hint {
    color: var(--text-faint);
    font-size: 12px;
  }
  .colors {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 6px 16px;
  }
  .color {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  input[type="color"] {
    width: 28px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: none;
    cursor: pointer;
  }
  .clabel {
    flex: 1;
    min-width: 0;
  }
  .hex {
    width: 84px;
    height: 26px;
    font-family: var(--mono);
    font-size: 12px;
  }
  .hex.bad {
    border-color: var(--del);
  }
  .lanes-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .lanes {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .small {
    height: 24px;
    padding: 0 8px;
    font-size: 12px;
  }
  .seg {
    display: flex;
  }
  .seg button {
    border-radius: 0;
  }
  .seg button + button {
    margin-left: -1px;
  }
  .seg button:first-child {
    border-radius: 5px 0 0 5px;
  }
  .seg button:last-child {
    border-radius: 0 5px 5px 0;
  }
  .seg button.on {
    position: relative;
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
</style>
