<script lang="ts">
  import { t } from "../lib/i18n";
  import { derive, type ThemeDef } from "../lib/themes.svelte";

  /** 一个缩小的假界面：只用这套主题的颜色画，不受当前界面主题影响 */
  let { def }: { def: ThemeDef } = $props();

  const style = $derived(
    Object.entries(derive(def))
      .map(([k, v]) => `${k}:${v}`)
      .join(";"),
  );
  const lane = (i: number) => def.colors.lanes[i % def.colors.lanes.length];

  /** 每行：泳道、说明、作者、是否选中 */
  const ROWS = [
    { col: 0, msg: t("修复登录页面的样式"), who: "Alice", sel: false },
    { col: 1, msg: t("新增导出功能"), who: "Bob", sel: true },
    { col: 1, msg: t("更新依赖"), who: "Carol", sel: false },
    { col: 2, msg: t("调整首页布局"), who: "Dave", sel: false },
    { col: 0, msg: t("合并分支 feature/export"), who: "Alice", sel: false },
  ];
</script>

<div class="pv" {style}>
  <div class="tabs">
    <span class="tab on">gittree</span>
    <span class="tab">demo</span>
  </div>
  <div class="bar">
    <span class="ico">⟳</span><span class="ico">⬇</span><span class="ico">⬆</span>
    <span class="grow"></span>
    <span class="search">{t("搜索")}</span>
  </div>
  <div class="main">
    <div class="side">
      <div class="sh">{t("本地")}</div>
      <div class="si on">main</div>
      <div class="si">feature/export</div>
      <div class="sh">{t("远程")}</div>
      <div class="si">origin</div>
    </div>
    <div class="graph">
      <svg class="lanes" width="64" height={ROWS.length * 24} aria-hidden="true">
        {#each [0, 1, 2] as c (c)}
          <line x1={12 + c * 18} y1="12" x2={12 + c * 18} y2={ROWS.length * 24 - 12} stroke={lane(c)} stroke-width="2" />
        {/each}
        {#each ROWS as r, i (i)}
          <circle cx={12 + r.col * 18} cy={12 + i * 24} r="6" fill={lane(r.col)} stroke={r.sel ? "#fff" : def.colors.graph} stroke-width="2" />
        {/each}
      </svg>
      {#each ROWS as r, i (i)}
        <div class="row" class:sel={r.sel}>
          {#if i === 0}<span class="badge" style="--c:{lane(0)}">main</span>{/if}
          <span class="msg">{r.msg}</span>
          <span class="who">{r.who}</span>
          <span class="when">{t("{0} 小时前", i + 1)}</span>
        </div>
      {/each}
    </div>
    <div class="panel">
      <div class="card">
        <div class="ct">{t("新增导出功能")}</div>
        <div class="cm">Bob · 3a9f2c1</div>
      </div>
      <div class="diff">
        <div class="hunk">@@ -12,3 +12,4 @@</div>
        <div class="ln">  function save() {"{"}</div>
        <div class="ln del">-   write(a)</div>
        <div class="ln add">+   write(a, fmt)</div>
        <div class="ln add">+   notify()</div>
      </div>
      <div class="btns">
        <span class="btn primary">{t("提交")}</span>
        <span class="btn">{t("取消")}</span>
      </div>
    </div>
  </div>
</div>

<style>
  .pv {
    display: flex;
    flex-direction: column;
    height: 210px;
    overflow: hidden;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--bg-graph);
    color: var(--text);
    font-size: 11px;
    user-select: none;
  }
  .tabs {
    display: flex;
    gap: 2px;
    padding: 4px 6px 0;
    background: var(--bg-tabbar);
  }
  .tab {
    padding: 3px 10px;
    border-radius: 5px 5px 0 0;
    color: var(--text-dim);
  }
  .tab.on {
    background: var(--bg-toolbar);
    color: var(--text);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 26px;
    padding: 0 10px;
    background: var(--bg-toolbar);
    border-bottom: 1px solid var(--border);
    color: var(--text-dim);
  }
  .grow {
    flex: 1;
  }
  .search {
    width: 110px;
    padding: 1px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--input-bg);
    color: var(--text-faint);
  }
  .main {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .side {
    width: 92px;
    flex-shrink: 0;
    padding: 4px 0;
    background: var(--bg-side);
    border-right: 1px solid var(--border);
  }
  .sh {
    padding: 3px 8px;
    color: var(--text-faint);
    font-size: 10px;
  }
  .si {
    padding: 2px 8px 2px 14px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .si.on {
    background: var(--bg-selected);
  }
  .graph {
    position: relative;
    flex: 1;
    min-width: 0;
    padding-top: 2px;
  }
  .lanes {
    position: absolute;
    top: 2px;
    left: 4px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 8px 0 72px;
    white-space: nowrap;
  }
  .row.sel {
    background: var(--bg-selected);
  }
  .badge {
    flex-shrink: 0;
    padding: 0 5px;
    border-radius: 3px;
    background: var(--c);
    color: #fff;
    font-weight: 700;
  }
  .msg {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .who {
    color: var(--text-dim);
  }
  .when {
    color: var(--text-faint);
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 170px;
    flex-shrink: 0;
    padding: 6px;
    background: var(--bg-side);
    border-left: 1px solid var(--border);
  }
  .card {
    padding: 5px 7px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg-card);
    box-shadow: var(--shadow);
  }
  .ct {
    font-weight: 600;
  }
  .cm {
    color: var(--text-dim);
  }
  .diff {
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-graph);
    font-family: var(--mono);
    font-size: 10px;
    white-space: pre;
  }
  .hunk {
    padding: 1px 4px;
    background: var(--bg-hunk);
    color: var(--text-faint);
  }
  .ln {
    overflow: hidden;
    padding: 0 4px;
    text-overflow: ellipsis;
  }
  .ln.add {
    background: var(--bg-add);
    color: var(--add);
  }
  .ln.del {
    background: var(--bg-del);
    color: var(--del);
  }
  .btns {
    display: flex;
    gap: 4px;
  }
  .btn {
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--button-bg);
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
</style>
