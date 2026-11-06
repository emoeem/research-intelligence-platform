/* RIP Workbench 骨架脚本：fetch 四个只读 API，渲染四页签。
 * 安全约定：所有动态数据一律 textContent 注入，不用 innerHTML。 */
"use strict";

(function () {
  const $ = (sel, root) => (root || document).querySelector(sel);
  const $$ = (sel, root) => Array.from((root || document).querySelectorAll(sel));

  const state = {
    tab: "overview",
    resources: { keyword: "", grade: "", limit: 50 },
    reviews: { queue: "" },
  };

  const TAB_IDS = ["overview", "resources", "reviews", "tasks"];

  // ---------- 基础工具 ----------

  function el(tag, attrs, children) {
    const node = document.createElement(tag);
    if (attrs) {
      for (const [k, v] of Object.entries(attrs)) {
        if (v === null || v === undefined) continue;
        if (k === "class") node.className = v;
        else if (k === "text") node.textContent = v;
        else if (k === "title") node.title = v;
        else node.setAttribute(k, v);
      }
    }
    if (children) {
      for (const child of children) if (child) node.appendChild(child);
    }
    return node;
  }

  function authToken() {
    try {
      return localStorage.getItem("rpToken") || "";
    } catch (err) {
      return "";
    }
  }

  async function api(path) {
    let resp;
    const headers = { Accept: "application/json" };
    const token = authToken();
    if (token) headers.Authorization = `Bearer ${token}`;
    try {
      resp = await fetch(path, { headers });
    } catch (err) {
      throw new Error(`网络请求失败：${err.message}`);
    }
    if (resp.status === 401) {
      localStorage.removeItem("rpToken");
      location.href = "/login.html";
      throw new Error("需要登录");
    }
    let payload = null;
    try {
      payload = await resp.json();
    } catch (err) {
      throw new Error(`响应不是合法 JSON（HTTP ${resp.status}）`);
    }
    if (!resp.ok) {
      const msg = payload && payload.error ? payload.error.message : `HTTP ${resp.status}`;
      throw new Error(msg);
    }
    return payload;
  }

  const DECISION_TONE = {
    NEEDS: "b-amber",
    RISK: "b-red",
    OK: "b-green",
    APPROVED: "b-blue",
    COMPLEMENTARY: "b-teal",
    RELATED: "b-teal",
  };

  function decisionBadge(decision) {
    let tone = "b-muted";
    for (const prefix of Object.keys(DECISION_TONE)) {
      if (decision.startsWith(prefix)) { tone = DECISION_TONE[prefix]; break; }
    }
    return el("span", { class: `badge ${tone}`, text: decision });
  }

  function gradeBadge(grade) {
    const known = ["A", "B", "C", "D"].includes(grade);
    return el("span", { class: `badge ${known ? `g-${grade}` : "g-unknown"}`, text: grade || "无" });
  }

  function stateBar(rows, colorByState) {
    const max = Math.max(...rows.map((r) => r.n), 1);
    const wrap = el("div", { class: "state-bar" });
    for (const row of rows) {
      const fill = el("i");
      fill.style.width = `${Math.max(4, Math.round((row.n / max) * 100))}%`;
      if (colorByState && colorByState[row.k]) fill.style.background = colorByState[row.k];
      wrap.appendChild(
        el("div", { class: "state-bar-row" }, [
          el("span", { class: "mono", text: row.k }),
          el("span", { class: "n", text: String(row.n) }),
          el("div", { class: "bar" }, [fill]),
        ])
      );
    }
    return wrap;
  }

  function renderState(container, kind, message, retryFn) {
    container.replaceChildren();
    const box = el("div", { class: `state ${kind}` });
    if (kind === "loading") {
      box.textContent = message || "加载中…";
    } else if (kind === "error") {
      box.appendChild(el("div", { text: message || "加载失败" }));
      if (retryFn) {
        const btn = el("button", { class: "btn ghost retry", type: "button", text: "重试" });
        btn.addEventListener("click", retryFn);
        box.appendChild(btn);
      }
    } else if (kind === "empty") {
      box.appendChild(el("div", { text: message || "暂无数据" }));
    }
    container.appendChild(box);
  }

  function fmtTime(ts) {
    if (!ts) return "";
    return ts.replace("T", " ").replace("Z", " UTC");
  }

  // ---------- Overview ----------

  async function loadOverview() {
    const body = $("#overview-body");
    renderState(body, "loading");
    try {
      const data = await api("/api/stats");
      renderOverview(body, data);
      const dbMeta = $("#db-meta");
      const versions = (data.database.schema_version || []).map((v) => `v${v.version}`).join(" + ");
      dbMeta.textContent = `数据库：${data.database.file}（只读，schema ${versions}）`;
    } catch (err) {
      renderState(body, "error", `加载失败：${err.message}`, loadOverview);
    }
  }

  function statCard(label, num, muted) {
    return el("div", { class: `card${muted ? " muted-card" : ""}` }, [
      el("div", { class: "num", text: String(num) }),
      el("div", { class: "label", text: label }),
    ]);
  }

  function tagChips(pairs, toneFn) {
    const wrap = el("div", { class: "tag-row" });
    for (const [key, count] of pairs) {
      const chip = el("span", { class: "summary-chip" }, [
        el("span", { text: key }),
        el("b", { text: String(count) }),
      ]);
      if (toneFn) chip.classList.add(toneFn(key));
      wrap.appendChild(chip);
    }
    return wrap;
  }

  const STATE_COLORS = {
    COMPLETED: "#A6E3A1",
    AUTHORIZED_EXECUTION: "#A6E3A1",
    LOCAL_KNOWLEDGE_RETRIEVAL: "#89B4FA",
    AUTHORIZATION_REVIEW: "#F9E2AF",
    CREATED: "#CBA6F7",
  };

  function renderOverview(body, data) {
    body.replaceChildren();

    const cards = el("div", { class: "cards" }, [
      statCard("资源 Resource", data.resources.total),
      statCard("知识 Knowledge", data.knowledge.total),
      statCard("审核 Review", data.reviews.total),
      statCard("任务 Task", data.tasks.total),
      statCard("证据 Evidence", data.evidence.total),
      statCard("实体别名 Alias", data.entity_alias.total, true),
      statCard(`工具 / Skill`, `${data.tools.total} / ${data.skills.total}`, true),
    ]);
    body.appendChild(cards);

    const gradePairs = Object.entries(data.resources.by_grade).sort((a, b) => a[0].localeCompare(b[0]));
    const queuePairs = Object.entries(data.reviews.by_queue).map(([q, v]) => [q, v.total]);

    body.appendChild(el("div", { class: "grid-2" }, [
      el("div", { class: "box" }, [
        el("h3", { text: "资源质量等级" }),
        el("div", { class: "box-body" }, [tagChips(gradePairs, (g) => `g-${g}`)]),
      ]),
      el("div", { class: "box" }, [
        el("h3", { text: "知识生命周期 / 验证状态" }),
        el("div", { class: "box-body" }, [
          tagChips(Object.entries(data.knowledge.by_lifecycle), () => "b-blue"),
          el("div", { style: "height:10px" }),
          tagChips(Object.entries(data.knowledge.by_verified), (k) =>
            k === "verified" ? "b-green" : k === "unverified" ? "b-amber" : "b-muted"),
        ]),
      ]),
      el("div", { class: "box" }, [
        el("h3", { text: "审核队列" }),
        el("div", { class: "box-body" }, [tagChips(queuePairs, (q) =>
          q === "d5" ? "b-amber" : q === "license" ? "b-red" : "b-blue")]),
      ]),
      el("div", { class: "box" }, [
        el("h3", { text: "任务状态分布" }),
        el("div", { class: "box-body" }, [
          stateBar(
            Object.entries(data.tasks.by_state).map(([k, n]) => ({ k, n })),
            STATE_COLORS
          ),
        ]),
      ]),
    ]));

    const recent = data.recent_reviews || [];
    const list = el("div", { class: "review-list" });
    if (recent.length === 0) {
      renderStateListEmpty(list);
    } else {
      for (const r of recent) list.appendChild(reviewItem(r));
    }
    body.appendChild(el("div", { class: "box" }, [
      el("h3", { text: "最近审核" }),
      el("div", { class: "box-body" }, [list]),
    ]));

    function renderStateListEmpty(list) {
      list.appendChild(el("div", { class: "state empty", text: "暂无审核记录" }));
    }
  }

  function reviewItem(r) {
    return el("div", { class: "review-item" }, [
      el("span", { class: "badge b-muted", text: r.target_type }),
      decisionBadge(r.decision),
      el("span", { class: "rid", title: r.target_id, text: r.target_id }),
      el("span", { class: "conf", text: r.confidence !== null && r.confidence !== undefined ? `conf=${r.confidence}` : "" }),
      el("span", { class: "when", text: fmtTime(r.reviewed_at) }),
      el("span", { class: "reason", text: r.reason || "" }),
    ]);
  }

  // ---------- Resources ----------

  async function loadResources() {
    const body = $("#resources-body");
    const { keyword, grade, limit } = state.resources;
    renderState(body, "loading");
    try {
      const params = new URLSearchParams({ limit: String(limit) });
      if (keyword) params.set("keyword", keyword);
      if (grade) params.set("grade", grade);
      const data = await api(`/api/resources?${params.toString()}`);
      renderResources(body, data);
    } catch (err) {
      renderState(body, "error", `加载失败：${err.message}`, loadResources);
    }
  }

  function renderResources(body, data) {
    body.replaceChildren();
    body.appendChild(el("p", { class: "result-meta" }, [
      el("span", { text: `匹配 ${data.total} / 共 ${data.total} 条资源，本次返回 ${data.returned} 条（limit=${data.limit}）` }),
    ]));
    if (data.total === 0) {
      body.appendChild(el("div", { class: "state empty" }, [
        el("div", { text: data.message || "没有匹配的资源" }),
        el("div", { class: "empty-hint", text: "试试更短的关键词，或切换等级过滤" }),
      ]));
      return;
    }
    const thead = el("thead", {}, [el("tr", {}, [
      el("th", { text: "资源" }),
      el("th", { text: "等级" }),
      el("th", { text: "质量分" }),
      el("th", { text: "类型" }),
      el("th", { text: "来源仓库" }),
      el("th", { text: "状态" }),
      el("th", { text: "最近发现" }),
    ])]);
    const tbody = el("tbody");
    for (const item of data.items) {
      const name = item.name || item.identity || item.resource_id;
      const sub = item.repo_name ? `${item.owner ? item.owner + " / " : ""}${item.repo_name}` : item.identity;
      tbody.appendChild(el("tr", {}, [
        el("td", {}, [
          el("div", { class: "cell-title", title: item.canonical_url || "", text: name }),
          el("div", { class: "cell-sub", text: sub || "" }),
        ]),
        el("td", {}, [gradeBadge(item.quality_grade)]),
        el("td", { class: "num-cell", text: item.quality_final_score !== null ? String(item.quality_final_score) : "-" }),
        el("td", { text: item.resource_type || "-" }),
        el("td", { class: "cell-desc", title: item.canonical_url || "", text: item.repo_name || item.owner || "-" }),
        el("td", {}, [el("span", { class: "badge b-blue", text: item.lifecycle || item.source_status || "-" })]),
        el("td", { class: "num-cell", text: fmtTime(item.last_seen || item.first_seen) }),
      ]));
    }
    const wrap = el("div", { class: "table-wrap" });
    wrap.appendChild(el("table", {}, [thead, tbody]));
    body.appendChild(wrap);
  }

  // ---------- Reviews ----------

  async function loadReviews() {
    const body = $("#reviews-body");
    renderState(body, "loading");
    try {
      const params = new URLSearchParams({ limit: "50" });
      if (state.reviews.queue) params.set("queue", state.reviews.queue);
      const data = await api(`/api/reviews?${params.toString()}`);
      renderReviews(body, data);
    } catch (err) {
      renderState(body, "error", `加载失败：${err.message}`, loadReviews);
    }
  }

  function renderReviews(body, data) {
    body.replaceChildren();
    body.appendChild(el("p", { class: "result-meta", text: `审核记录共 ${data.total} 条` }));
    const queueEntries = Object.entries(data.queues);
    if (queueEntries.length === 0) {
      renderState(body, "empty", "暂无审核记录");
      return;
    }
    for (const [queueKey, queue] of queueEntries) {
      const block = el("div", { class: "queue-block" });
      const head = el("div", { class: "queue-head" }, [
        el("h3", { text: `${queue.label}（${queueKey}）` }),
        el("span", { class: "total", text: `共 ${queue.total} 条，显示 ${queue.returned} 条` }),
      ]);
      for (const [decision, n] of Object.entries(queue.by_decision)) {
        head.appendChild(decisionBadge(`${decision}×${n}`));
      }
      block.appendChild(head);
      const list = el("div", { class: "review-list" });
      if (queue.items.length === 0) {
        list.appendChild(el("div", { class: "state empty", text: "该队列暂无记录" }));
      } else {
        for (const item of queue.items) list.appendChild(reviewItem(item));
      }
      block.appendChild(list);
      body.appendChild(block);
    }
  }

  // ---------- Tasks ----------

  async function loadTasks() {
    const body = $("#tasks-body");
    renderState(body, "loading");
    try {
      const data = await api("/api/tasks?limit=200");
      renderTasks(body, data);
    } catch (err) {
      renderState(body, "error", `加载失败：${err.message}`, loadTasks);
    }
  }

  function renderTasks(body, data) {
    body.replaceChildren();
    body.appendChild(el("p", { class: "result-meta", text: `任务共 ${data.total} 条` }));
    if (data.total === 0) {
      body.appendChild(el("div", { class: "state empty" }, [
        el("div", { text: data.message || "暂无任务" }),
        el("div", { class: "empty-hint", text: "research_task 为空：待 M1 全量数据接入后显示" }),
      ]));
      return;
    }
    body.appendChild(el("div", { class: "summary-chips" }, [
      ...Object.entries(data.by_state).map(([k, n]) =>
        el("span", { class: "summary-chip" }, [el("span", { text: k }), el("b", { text: String(n) })])),
      ...Object.entries(data.by_type).map(([k, n]) =>
        el("span", { class: "summary-chip" }, [el("span", { text: `type:${k}` }), el("b", { text: String(n) })])),
    ]));

    const thead = el("thead", {}, [el("tr", {}, [
      el("th", { text: "状态" }),
      el("th", { text: "类型" }),
      el("th", { text: "任务" }),
      el("th", { text: "目标" }),
      el("th", { text: "任务 ID" }),
      el("th", { text: "更新时间" }),
    ])]);
    const tbody = el("tbody");
    for (const t of data.items) {
      tbody.appendChild(el("tr", {}, [
        el("td", {}, [el("span", { class: "badge", text: t.state })]),
        el("td", { text: t.task_type || "-" }),
        el("td", { class: "cell-title", title: t.objective || "", text: t.title || t.task_id }),
        el("td", { class: "cell-desc", title: t.target || "", text: t.target || "-" }),
        el("td", { class: "mono cell-id", title: t.task_id, text: t.task_id }),
        el("td", { class: "num-cell", text: fmtTime(t.updated_at || t.created_at) }),
      ]));
    }
    const wrap = el("div", { class: "table-wrap" });
    wrap.appendChild(el("table", {}, [thead, tbody]));
    body.appendChild(wrap);
  }

  // ---------- 页签路由 ----------

  const LOADERS = {
    overview: loadOverview,
    resources: loadResources,
    reviews: loadReviews,
    tasks: loadTasks,
  };

  function switchTab(tab, force) {
    if (!TAB_IDS.includes(tab)) tab = "overview";
    state.tab = tab;
    location.hash = `#${tab}`;
    $$(".nav-item").forEach((btn) => btn.classList.toggle("active", btn.dataset.tab === tab));
    $$(".tab-panel").forEach((panel) => panel.classList.toggle("hidden", panel.dataset.panel !== tab));
    const isEmpty = $("#tab-" + tab + " .panel-body").childElementCount === 0;
    if (force || isEmpty) LOADERS[tab]();
  }

  function bindNav() {
    $$(".nav-item").forEach((btn) => {
      btn.addEventListener("click", () => switchTab(btn.dataset.tab));
    });
    $$("[data-refresh]").forEach((btn) => {
      btn.addEventListener("click", () => LOADERS[btn.dataset.refresh]());
    });
    window.addEventListener("hashchange", () => {
      const tab = location.hash.replace("#", "");
      if (tab !== state.tab) switchTab(tab);
    });
  }

  function bindResourceFilters() {
    $("#res-search").addEventListener("click", () => {
      state.resources.keyword = $("#res-keyword").value.trim();
      state.resources.grade = $("#res-grade").value;
      state.resources.limit = parseInt($("#res-limit").value, 10) || 50;
      loadResources();
    });
    $("#res-keyword").addEventListener("keydown", (ev) => {
      if (ev.key === "Enter") $("#res-search").click();
    });
    $("#res-grade").addEventListener("change", () => $("#res-search").click());
  }

  function bindReviewChips() {
    $$("#rev-chips .chip").forEach((chip) => {
      chip.addEventListener("click", () => {
        $$("#rev-chips .chip").forEach((c) => c.classList.toggle("active", c === chip));
        state.reviews.queue = chip.dataset.queue;
        loadReviews();
      });
    });
  }

  bindNav();
  bindResourceFilters();
  bindReviewChips();
  switchTab(location.hash.replace("#", "") || "overview", true);
  if (state.tab !== "overview") loadOverview(); // 后台预热：侧栏 db-meta 与 Overview 数据
})();
