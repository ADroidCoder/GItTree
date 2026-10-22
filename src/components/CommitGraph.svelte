<script lang="ts">
  import { t } from "../lib/i18n";
  import { onMount, untrack } from "svelte";
  import { api, type Edge, type RefInfo, type Row } from "../lib/api";
  import { drag as refDrag, type RepoTab } from "../lib/store.svelte";
  import { themes } from "../lib/themes.svelte";
  import { formatDate, initials, laneColor, relativeTime } from "../lib/util";

  let {
    tab,
    active,
    onRowMenu,
    onPickedMenu,
    onRefMenu,
    onRefDrop,
    onRefDblClick,
  }: {
    tab: RepoTab;
    /** 是否是当前显示的标签页（隐藏的标签页不画图） */
    active: boolean;
    onRowMenu: (e: MouseEvent, row: Row, index: number) => void;
    /** 在多选的提交上点右键 */
    onPickedMenu: (e: MouseEvent) => void;
    onRefMenu: (e: MouseEvent, ref: RefInfo) => void;
    /** 把一个分支拖到另一个分支上 */
    onRefDrop: (e: MouseEvent, src: RefInfo, dst: RefInfo) => void;
    onRefDblClick: (ref: RefInfo) => void;
  } = $props();

  const ROW_H = 30;
  const LANE_W = 22;
  const GRAPH_PAD = 16;
  const NODE_R = 10;
  /** 每次向后端请求多少行 */
  const BLOCK = 128;
  /** 最多缓存多少块，超出时丢弃离当前位置最远的 */
  const MAX_BLOCKS = 300;

  let viewport: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let vh = $state(0);
  let scrollTop = $state(0);
  let refWidth = $state(180);
  /** 提交列表的宽度 */
  let vw = $state(0);
  /** 画下所有泳道需要的宽度 */
  let lanesWidth = $state(160);
  /** 用户拖动过分支图列宽后就固定用它 */
  let userGraph = $state<number | null>(null);

  // 提交说明是最重要的一列，窄的时候依次让出空间：先隐藏提交号，再收窄分支图，再隐藏作者
  const MSG_MIN = 240;
  const AUTHOR_W = 130;
  const DATE_W = 90;
  const SHA_W = 78;
  const cols = $derived.by(() => {
    let graph = userGraph ?? Math.max(70, Math.min(lanesWidth, 360));
    let sha = true;
    let author = true;
    const free = () => vw - refWidth - graph - DATE_W - (author ? AUTHOR_W : 0) - (sha ? SHA_W : 0);
    if (!vw) return { graph, sha, author };
    if (free() < MSG_MIN) sha = false;
    if (free() < MSG_MIN && userGraph === null) graph = Math.max(Math.min(graph, 120), graph - (MSG_MIN - free()));
    if (free() < MSG_MIN) author = false;
    if (free() < MSG_MIN && userGraph === null) graph = Math.max(70, graph - (MSG_MIN - free()));
    return { graph, sha, author };
  });
  const graphWidth = $derived(cols.graph);

  // 行数据缓存放在普通对象里（不做响应式，避免每次写入都触发更新），用 tick 通知界面重绘
  interface Block {
    gen: number;
    rows: Row[];
    edges: Edge[];
  }
  const blocks = new Map<number, Block>();
  const inflight = new Map<number, number>();
  let gen = 0;
  let tick = $state(0);

  /** 有未提交的改动时，第 0 行显示 WIP，提交整体往下挪一行 */
  const off = $derived(tab.changeCount > 0 ? 1 : 0);
  const commits = $derived(tab.repo?.total ?? 0);
  const total = $derived(commits + off);
  const maxScroll = $derived(Math.max(0, total * ROW_H - vh));
  const first = $derived(Math.floor(scrollTop / ROW_H));
  const last = $derived(Math.min(total, first + Math.ceil(vh / ROW_H) + 1));
  const searchHits = $derived(new Set(tab.search.hits));
  const currentHit = $derived(tab.search.hits[tab.search.index] ?? -1);
  const headRow = $derived(tab.repo?.head.row ?? -1);

  function rowAt(r: number): Row | undefined {
    if (r < 0) return undefined;
    return blocks.get(Math.floor(r / BLOCK))?.rows[r % BLOCK];
  }

  function setScroll(v: number) {
    scrollTop = Math.max(0, Math.min(maxScroll, v));
  }

  // 刷新后保留旧数据先显示，新数据到了再替换，避免闪烁
  $effect(() => {
    const v = tab.graphVersion;
    const lanes = tab.repo?.max_lanes ?? 0;
    untrack(() => {
      gen = v;
      if (lanes) lanesWidth = lanes * LANE_W + GRAPH_PAD * 2;
      setScroll(scrollTop);
      tick++;
    });
  });

  // 根据可见区域请求数据（前后各多取一块，滚动时基本不会看到空行）
  $effect(() => {
    void tick;
    if (!commits || !vh || !tab.id) return;
    const r0 = Math.max(0, first - off);
    const r1 = Math.max(0, last - off);
    const b0 = Math.max(0, Math.floor(r0 / BLOCK) - 1);
    const b1 = Math.min(Math.floor((commits - 1) / BLOCK), Math.floor(r1 / BLOCK) + 1);
    for (let b = b0; b <= b1; b++) {
      const cur = blocks.get(b);
      if ((cur && cur.gen === gen) || inflight.get(b) === gen) continue;
      const g = gen;
      inflight.set(b, g);
      api
        .getRows(tab.id, b * BLOCK, Math.min(commits, (b + 1) * BLOCK))
        .then((res) => {
          if (inflight.get(b) === g) inflight.delete(b);
          const existing = blocks.get(b);
          if (existing && existing.gen > g) return;
          blocks.set(b, { gen: g, rows: res.rows, edges: res.edges });
          evict(b);
          tick++;
        })
        .catch(() => inflight.delete(b));
    }
  });

  function evict(center: number) {
    if (blocks.size <= MAX_BLOCKS) return;
    const keys = [...blocks.keys()].sort((a, b) => Math.abs(b - center) - Math.abs(a - center));
    for (const k of keys.slice(0, blocks.size - MAX_BLOCKS)) blocks.delete(k);
  }

  /** 可见的显示行：d 是显示行号，r 是提交行号（WIP 行的 r 为 -1） */
  const visible = $derived.by(() => {
    void tick;
    const out: { d: number; r: number; row: Row | undefined }[] = [];
    for (let d = first; d < last; d++) {
      const r = d - off;
      out.push({ d, r, row: rowAt(r) });
    }
    return out;
  });

  /** 多选的行号 */
  const pickedRows = $derived(new Set(tab.picked.map((p) => p.row)));

  function clickRow(e: MouseEvent, r: number, row: Row) {
    if (e.metaKey || e.ctrlKey) tab.togglePick(r, row);
    else if (e.shiftKey) tab.pickRange(r, row);
    else tab.select(r, row.oid);
  }

  // ---------- 画图 ----------
  let frame = 0;
  $effect(() => {
    // 这些值变化时重绘
    void [tick, scrollTop, vh, graphWidth, tab.selectedRow, pickedRows, headRow, off, active, themes.version, systemDark];
    if (active && !frame) frame = requestAnimationFrame(draw);
  });

  function draw() {
    frame = 0;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = graphWidth;
    const h = vh;
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
      canvas.style.width = `${w}px`;
      canvas.style.height = `${h}px`;
    }
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    if (!total) return;

    const styles = getComputedStyle(canvas);
    const bg = styles.getPropertyValue("--bg-graph").trim() || "#1e222e";
    const x = (c: number) => GRAPH_PAD + c * LANE_W;
    /** 提交行 r 的中心 y 坐标 */
    const y = (r: number) => (r + off) * ROW_H - scrollTop + ROW_H / 2;
    const lo = -ROW_H;
    const hi = h + ROW_H;

    // 收集可见区域内的连线（不同块里可能有同一条，按编号去重）
    const edges = new Map<number, Edge>();
    const r0 = Math.max(0, first - off);
    const r1 = Math.max(r0, last - off - 1);
    for (let b = Math.floor(r0 / BLOCK); b <= Math.floor(r1 / BLOCK); b++) {
      blocks.get(b)?.edges.forEach((e) => edges.set(e[0], e));
    }

    ctx.lineWidth = 2;
    ctx.lineCap = "round";
    for (const [, a, b, cc, pc, lane, color] of edges.values()) {
      const ya = y(a);
      const yb = y(b);
      ctx.strokeStyle = laneColor(color);
      ctx.beginPath();
      // 1. 从子提交弯到泳道
      let v0 = ya;
      if (lane !== cc) {
        v0 = Math.min(ya + ROW_H, yb);
        if (v0 >= lo && ya <= hi) curve(ctx, x(cc), ya, x(lane), v0);
      }
      // 3. 从泳道弯到父提交
      let v1 = yb;
      if (lane !== pc) {
        v1 = Math.max(yb - ROW_H, v0);
        if (yb >= lo && v1 <= hi) curve(ctx, x(lane), v1, x(pc), yb);
      }
      // 2. 沿泳道竖直向下（只画屏幕内的部分）
      const s = Math.max(v0, lo);
      const t = Math.min(v1, hi);
      if (s < t) {
        ctx.moveTo(x(lane), s);
        ctx.lineTo(x(lane), t);
      }
      ctx.stroke();
    }

    // WIP 节点：虚线圆，用虚线连到 HEAD
    if (off) {
      const head = rowAt(headRow);
      const col = head?.col ?? 0;
      const color = head ? laneColor(head.color) : "#888";
      const wy = ROW_H / 2 - scrollTop;
      ctx.save();
      ctx.setLineDash([3, 3]);
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      if (headRow >= 0) {
        ctx.beginPath();
        ctx.moveTo(x(col), wy + NODE_R);
        ctx.lineTo(x(col), Math.min(y(headRow) - NODE_R, hi));
        ctx.stroke();
      }
      ctx.beginPath();
      ctx.arc(x(col), wy, NODE_R - 1, 0, Math.PI * 2);
      ctx.fillStyle = bg;
      ctx.fill();
      ctx.stroke();
      ctx.restore();
    }

    // 提交节点：普通提交画成头像圆（作者首字），合并提交画成小圆点
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.font = `600 10px ${styles.getPropertyValue("--sans")}`;
    for (let d = Math.max(first, off); d < last; d++) {
      const r = d - off;
      const row = rowAt(r);
      if (!row) continue;
      const cx = x(row.col);
      const cy = y(r);
      const color = laneColor(row.color);
      // 有分支或标签的提交：从左边的标签列画一条连接线
      if (row.refs.length) {
        ctx.beginPath();
        ctx.moveTo(0, cy);
        ctx.lineTo(cx - (row.parents > 1 ? 4 : NODE_R), cy);
        ctx.strokeStyle = color;
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
      if (row.parents > 1) {
        ctx.beginPath();
        ctx.arc(cx, cy, 4.5, 0, Math.PI * 2);
        ctx.fillStyle = color;
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = bg;
        ctx.stroke();
        continue;
      }
      ctx.beginPath();
      ctx.arc(cx, cy, NODE_R, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.fill();
      ctx.lineWidth = 2;
      ctx.strokeStyle = r === tab.selectedRow || pickedRows.has(r) ? "#fff" : bg;
      ctx.stroke();
      ctx.fillStyle = "#fff";
      ctx.fillText(initials(row.author), cx, cy + 0.5);
      if (r === headRow) {
        ctx.beginPath();
        ctx.arc(cx, cy, NODE_R + 3.5, 0, Math.PI * 2);
        ctx.lineWidth = 2;
        ctx.strokeStyle = color;
        ctx.stroke();
      }
    }
  }

  function curve(ctx: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number) {
    const my = (y1 + y2) / 2;
    ctx.moveTo(x1, y1);
    ctx.bezierCurveTo(x1, my, x2, my, x2, y2);
  }

  // 跟随系统主题时，系统切换深浅色也要重绘画布
  let systemDark = $state(false);

  // ---------- 滚动和键盘 ----------
  onMount(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    systemDark = mq.matches;
    const onScheme = () => (systemDark = mq.matches);
    mq.addEventListener("change", onScheme);
    const ro = new ResizeObserver(() => {
      vh = viewport.clientHeight;
      vw = viewport.clientWidth;
      setScroll(scrollTop);
    });
    ro.observe(viewport);
    // 需要 passive: false 才能阻止默认滚动
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      let dy = e.deltaY;
      if (e.deltaMode === 1) dy *= ROW_H;
      else if (e.deltaMode === 2) dy *= vh;
      setScroll(scrollTop + dy);
    };
    viewport.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      mq.removeEventListener("change", onScheme);
      ro.disconnect();
      viewport.removeEventListener("wheel", onWheel);
      cancelAnimationFrame(frame);
    };
  });

  // 外部请求滚动到某个提交（点击侧边栏分支、搜索结果等）：不在屏幕内时居中显示
  let lastSeq = 0;
  $effect(() => {
    const req = tab.scrollRequest;
    if (!req || req.seq === lastSeq || !vh) return;
    lastSeq = req.seq;
    const top = (req.row + off) * ROW_H;
    if (top < scrollTop || top + ROW_H > scrollTop + vh) setScroll(top - vh / 2 + ROW_H / 2);
  });

  function ensureVisible(d: number) {
    const top = d * ROW_H;
    if (top < scrollTop) setScroll(top);
    else if (top + ROW_H > scrollTop + vh) setScroll(top + ROW_H - vh);
  }

  async function selectDisplay(d: number) {
    d = Math.max(0, Math.min(total - 1, d));
    ensureVisible(d);
    const r = d - off;
    if (r < 0) return tab.showWip();
    let row = rowAt(r);
    if (!row) row = (await api.getRows(tab.id, r, r + 1)).rows[0];
    if (row) tab.select(r, row.oid);
  }

  function onKey(e: KeyboardEvent) {
    const page = Math.max(1, Math.floor(vh / ROW_H) - 1);
    const cur = tab.selectedRow !== null ? tab.selectedRow + off : tab.panel === "wip" && off ? 0 : first - 1;
    const map: Record<string, number> = {
      ArrowDown: cur + 1,
      ArrowUp: cur - 1,
      PageDown: cur + page,
      PageUp: cur - page,
      Home: 0,
      End: total - 1,
      j: cur + 1,
      k: cur - 1,
    };
    if (e.key === "Escape" && tab.picked.length) {
      e.preventDefault();
      const r = tab.selectedRow;
      if (r !== null && tab.selectedOid) tab.select(r, tab.selectedOid);
      else tab.picked = [];
      return;
    }
    if (e.key in map && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      selectDisplay(map[e.key]);
    }
  }

  // ---------- 自定义滚动条 ----------
  const thumbH = $derived(total * ROW_H > vh ? Math.max(28, (vh * vh) / (total * ROW_H)) : 0);
  const thumbTop = $derived(maxScroll > 0 ? (scrollTop / maxScroll) * (vh - thumbH) : 0);
  let drag: { y: number; top: number } | null = null;

  function thumbDown(e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    drag = { y: e.clientY, top: scrollTop };
  }
  function thumbMove(e: PointerEvent) {
    if (!drag) return;
    const ratio = maxScroll / Math.max(1, vh - thumbH);
    setScroll(drag.top + (e.clientY - drag.y) * ratio);
  }
  function trackDown(e: PointerEvent) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const pos = (e.clientY - rect.top - thumbH / 2) / Math.max(1, vh - thumbH);
    setScroll(pos * maxScroll);
  }

  // ---------- 列宽拖动 ----------
  function resizer(which: "ref" | "graph") {
    return (e: PointerEvent) => {
      e.preventDefault();
      const startX = e.clientX;
      const startW = which === "ref" ? refWidth : graphWidth;
      const move = (ev: PointerEvent) => {
        const w = startW + ev.clientX - startX;
        if (which === "ref") refWidth = Math.max(60, Math.min(500, w));
        else userGraph = Math.max(40, Math.min(1500, w));
      };
      const up = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    };
  }

  // ---------- 分支 / 标签 ----------
  interface Badge {
    ref: RefInfo;
    label: string;
    remote: boolean;
    local: boolean;
  }
  function badges(row: Row): Badge[] {
    const refs = tab.repo?.refs;
    if (!refs || row.refs.length === 0) return [];
    const list = row.refs.map((i) => refs[i]).filter(Boolean);
    const out: Badge[] = [];
    const usedRemote = new Set<string>();
    for (const r of list.filter((r) => r.kind === "local")) {
      // 本地分支和它的上游在同一个提交上时合并成一个标签
      const up = list.find((x) => x.kind === "remote" && x.full === r.upstream);
      if (up) usedRemote.add(up.full);
      out.push({ ref: r, label: r.name, local: true, remote: !!up });
    }
    for (const r of list.filter((r) => r.kind === "remote" && !usedRemote.has(r.full))) {
      out.push({ ref: r, label: r.name, local: false, remote: true });
    }
    for (const r of list.filter((r) => r.kind === "tag")) out.push({ ref: r, label: r.name, local: false, remote: false });
    // 当前分支排第一
    out.sort((a, b) => Number(b.ref.is_head) - Number(a.ref.is_head));
    return out;
  }

  let hoverRefs = $state<number | null>(null);
  /** 拖动分支时，鼠标下面的目标分支 */
  let dropOn = $state<string | null>(null);
</script>

<div class="graph">
  <div class="header">
    <div class="h-ref" style="width:{refWidth}px">
      {t("分支 / 标签")}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="resize" onpointerdown={resizer("ref")}></div>
    </div>
    <div class="h-graph" style="width:{graphWidth}px">
      {t("分支图")}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="resize" onpointerdown={resizer("graph")}></div>
    </div>
    <div class="h-msg">
      {t("提交说明")}
      {#if tab.focus.length}
        <span class="focus-info">
          {tab.focus.length === 1 && tab.focus[0] === "HEAD"
            ? t("只显示当前分支")
            : t("只显示：{0}", tab.focus.map((f) => f.replace(/^refs\/(heads|remotes)\//, "")).join("、"))}
          <button class="focus-clear" onclick={() => tab.setFocus([])}>{t("显示全部")}</button>
        </span>
      {/if}
    </div>
    {#if cols.author}<div class="h-author">{t("作者")}</div>{/if}
    <div class="h-date">{t("时间")}</div>
    {#if cols.sha}<div class="h-sha">{t("提交")}</div>{/if}
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="viewport" bind:this={viewport} tabindex="0" onkeydown={onKey} role="grid">
    <div class="rows" style="transform: translateY({first * ROW_H - scrollTop}px)">
      {#each visible as v (v.d)}
        {#if v.r < 0}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div class="row wip" class:selected={tab.panel === "wip"} style="height:{ROW_H}px" role="row" tabindex="-1" onclick={() => tab.showWip()}>
            <div class="c-ref" style="width:{refWidth}px"></div>
            <div class="c-graph" style="width:{graphWidth}px"></div>
            <div class="c-msg">
              <span class="wip-label">{t("// 未提交的更改")}</span>
              <span class="wip-count">{t("{0} 个文件", tab.changeCount)}</span>
              {#if tab.status?.in_progress}<span class="wip-state">{t("{0} 进行中", tab.status.in_progress)}</span>{/if}
            </div>
          </div>
        {:else}
          {@const row = v.row}
          {@const bs = row ? badges(row) : []}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="row"
            class:selected={v.r === tab.selectedRow || pickedRows.has(v.r)}
            class:hit={searchHits.has(v.r)}
            class:current-hit={v.r === currentHit}
            class:head={v.r === headRow}
            style="height:{ROW_H}px"
            role="row"
            tabindex="-1"
            onclick={(e) => row && clickRow(e, v.r, row)}
            onmousedown={(e) => e.shiftKey && e.preventDefault()}
            oncontextmenu={(e) => {
              e.preventDefault();
              if (!row) return;
              if (pickedRows.has(v.r)) return onPickedMenu(e);
              tab.select(v.r, row.oid);
              onRowMenu(e, row, v.r);
            }}
          >
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="c-ref"
              style="width:{refWidth}px"
              onmouseenter={() => bs.length > 1 && (hoverRefs = v.r)}
              onmouseleave={() => (hoverRefs = null)}
            >
              {#if row && bs.length}
                {@const b = bs[0]}
                {@render badge(b, row)}
                {#if bs.length > 1}<span class="more" style="--c:{laneColor(row.color)}">+{bs.length - 1}</span>{/if}
                <span class="connector" style="--c:{laneColor(row.color)}"></span>
                {#if hoverRefs === v.r}
                  <div class="ref-pop">
                    {#each bs as b2 (b2.ref.full)}{@render badge(b2, row)}{/each}
                  </div>
                {/if}
              {/if}
            </div>
            <div class="c-graph" style="width:{graphWidth}px"></div>
            {#if row}
              <div class="c-msg">
                <span class="summary" class:merge={row.parents > 1}>{row.summary}</span>
              </div>
              {#if cols.author}<div class="c-author" title={`${row.author} <${row.email}>`}>{row.author}</div>{/if}
              <div class="c-date" title={formatDate(row.time)}>{relativeTime(row.time)}</div>
              {#if cols.sha}<div class="c-sha">{row.oid.slice(0, 7)}</div>{/if}
            {:else}
              <div class="c-msg"><span class="skeleton"></span></div>
            {/if}
          </div>
        {/if}
      {/each}
    </div>
    <canvas bind:this={canvas} class="canvas" style="left:{refWidth}px"></canvas>
    {#if thumbH > 0}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="track" onpointerdown={trackDown}>
        <div
          class="thumb"
          style="height:{thumbH}px; transform: translateY({thumbTop}px)"
          onpointerdown={thumbDown}
          onpointermove={thumbMove}
          onpointerup={() => (drag = null)}
        ></div>
      </div>
    {/if}
    {#if total === 0 && tab.repo}
      <div class="empty">{t("这个仓库还没有任何提交")}</div>
    {/if}
  </div>
</div>

{#snippet badge(b: Badge, row: Row)}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <span
    class="badge"
    class:tag={b.ref.kind === "tag"}
    class:is-head={b.ref.is_head}
    style="--c:{laneColor(row.color)}"
    class:drop-target={dropOn === b.ref.full}
    title={b.ref.kind === "tag" ? b.ref.full : t("{0}\n拖到另一个分支上可以合并或变基", b.ref.full)}
    draggable={b.ref.kind !== "tag"}
    ondragstart={(e) => {
      refDrag.ref = b.ref;
      e.dataTransfer?.setData("text/plain", b.ref.name);
      if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
    }}
    ondragend={() => {
      refDrag.ref = null;
      dropOn = null;
    }}
    ondragover={(e) => {
      if (!refDrag.ref || refDrag.ref.full === b.ref.full || b.ref.kind === "tag") return;
      e.preventDefault();
      dropOn = b.ref.full;
    }}
    ondragleave={() => dropOn === b.ref.full && (dropOn = null)}
    ondrop={(e) => {
      e.preventDefault();
      const src = refDrag.ref;
      refDrag.ref = null;
      dropOn = null;
      if (src) onRefDrop(e, src, b.ref);
    }}
    ondblclick={(e) => {
      e.stopPropagation();
      onRefDblClick(b.ref);
    }}
    oncontextmenu={(e) => {
      e.preventDefault();
      e.stopPropagation();
      onRefMenu(e, b.ref);
    }}
  >
    {#if b.ref.is_head}<span class="ico">✓</span>{/if}
    {#if b.ref.kind === "tag"}<span class="ico">◆</span>{:else if b.local}<span class="ico">💻</span>{/if}
    <span class="blabel">{b.label}</span>
    {#if b.remote}<span class="ico" title={b.local ? t("已与远程同步") : t("远程分支")}>☁</span>{/if}
  </span>
{/snippet}

<style>
  .graph {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--bg-graph);
  }
  .header {
    display: flex;
    height: 28px;
    align-items: center;
    border-bottom: 1px solid var(--border);
    color: var(--text-faint);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    flex-shrink: 0;
    user-select: none;
  }
  .header > div {
    padding: 0 10px;
    white-space: nowrap;
  }
  .h-ref,
  .h-graph {
    position: relative;
    flex-shrink: 0;
    border-right: 1px solid var(--border);
    height: 100%;
    display: flex;
    align-items: center;
  }
  .resize {
    position: absolute;
    right: -3px;
    top: 0;
    bottom: 0;
    width: 6px;
    cursor: col-resize;
    z-index: 2;
  }
  .resize:hover {
    background: var(--accent);
  }
  .h-msg,
  .c-msg {
    flex: 1;
    min-width: 0;
  }
  .h-msg {
    display: flex;
    align-items: center;
    gap: 10px;
    overflow: hidden;
  }
  .focus-info {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 4px 0 8px;
    height: 20px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--accent) 25%, transparent);
    color: var(--text);
    font-weight: normal;
    letter-spacing: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .focus-clear {
    height: 16px;
    padding: 0 6px;
    border: 0;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-size: 11px;
  }
  .h-author,
  .c-author {
    width: 130px;
    flex-shrink: 0;
  }
  .h-date,
  .c-date {
    width: 90px;
    flex-shrink: 0;
  }
  .h-sha,
  .c-sha {
    width: 78px;
    flex-shrink: 0;
  }
  .viewport {
    position: relative;
    flex: 1;
    overflow: hidden;
    outline: none;
    contain: strict;
  }
  .rows {
    position: absolute;
    z-index: 1;
    left: 0;
    right: 10px;
    top: 0;
    will-change: transform;
  }
  .row {
    display: flex;
    align-items: center;
    cursor: default;
    white-space: nowrap;
    font-size: 13px;
    outline: none;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.selected {
    background: var(--bg-selected);
  }
  .row.hit .c-msg {
    box-shadow: inset 3px 0 0 var(--warn);
  }
  .row.current-hit {
    background: var(--bg-hit);
  }
  .row > div {
    padding: 0 10px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .c-graph {
    flex-shrink: 0;
  }
  .c-ref {
    position: relative;
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    padding-right: 0 !important;
    overflow: visible !important;
  }
  .connector {
    flex: 1;
    min-width: 6px;
    height: 0;
    border-top: 1.5px solid var(--c);
  }
  .ref-pop {
    position: absolute;
    left: 6px;
    top: 26px;
    z-index: 10;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 6px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: var(--shadow);
  }
  .c-msg {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .summary {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .summary.merge {
    color: var(--text-dim);
  }
  .row.head .summary {
    font-weight: 600;
  }
  .wip-label {
    color: var(--text-dim);
    font-style: italic;
  }
  .wip-count {
    color: var(--text-faint);
  }
  .wip-state {
    color: var(--warn);
    font-weight: 600;
  }
  .badge.drop-target {
    outline: 2px dashed var(--accent);
    outline-offset: 1px;
  }
  .badge[draggable="true"] {
    cursor: grab;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 1;
    min-width: 0;
    max-width: 100%;
    height: 22px;
    padding: 0 8px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--c) 30%, var(--bg-graph));
    border: 1px solid color-mix(in srgb, var(--c) 70%, transparent);
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
  }
  .badge.is-head {
    background: var(--c);
    color: #fff;
    font-weight: 700;
  }
  .badge.tag {
    border-style: dashed;
  }
  .blabel {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .more {
    flex-shrink: 0;
    padding: 0 5px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--c) 30%, var(--bg-graph));
    font-size: 11px;
  }
  .ico {
    font-size: 10px;
    flex-shrink: 0;
  }
  .c-author,
  .c-date {
    color: var(--text-dim);
  }
  .c-sha {
    font-family: var(--mono);
    color: var(--text-faint);
    font-size: 12px;
  }
  .skeleton {
    display: block;
    width: 40%;
    height: 10px;
    border-radius: 3px;
    background: var(--bg-hover);
  }
  /* 画布在行的下面，行背景是半透明的，这样悬停弹出的标签不会被线条盖住 */
  .canvas {
    position: absolute;
    z-index: 0;
    top: 0;
    pointer-events: none;
  }
  .track {
    position: absolute;
    z-index: 2;
    top: 0;
    right: 0;
    bottom: 0;
    width: 10px;
  }
  .thumb {
    width: 6px;
    margin-left: 2px;
    border-radius: 3px;
    background: var(--scrollbar);
  }
  .thumb:hover {
    background: var(--scrollbar-hover);
  }
  .empty {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-dim);
  }
</style>
