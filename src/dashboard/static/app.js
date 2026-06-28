(function () {
  const $ = (id) => document.getElementById(id);

  const charts = {
    rps: echarts.init($("chart-rps"), null, { renderer: "canvas" }),
    users: echarts.init($("chart-users"), null, { renderer: "canvas" }),
    rt: echarts.init($("chart-rt"), null, { renderer: "canvas" }),
  };

  function lineOption(name, xs, ys, color) {
    return {
      animation: false,
      grid: { left: 40, right: 12, top: 16, bottom: 28 },
      xAxis: { type: "category", data: xs, axisLabel: { color: "#8b9bb4" }, axisLine: { lineStyle: { color: "#2d3a4d" } } },
      yAxis: { type: "value", axisLabel: { color: "#8b9bb4" }, splitLine: { lineStyle: { color: "#2d3a4d" } } },
      series: [{
        name,
        type: "line",
        showSymbol: false,
        data: ys,
        lineStyle: { width: 2, color },
        areaStyle: { color: color + "33" },
      }],
      tooltip: { trigger: "axis" },
    };
  }

  function fmt(n, digits) {
    if (n === undefined || n === null || Number.isNaN(n)) return "—";
    if (typeof n === "number" && !Number.isInteger(n)) return n.toFixed(digits ?? 2);
    return String(n);
  }

  function setConn(mode) {
    const el = $("conn");
    el.className = "conn " + mode;
    el.textContent = mode;
  }

  function setPhase(phase) {
    const el = $("phase");
    el.textContent = phase || "Unknown";
    el.className = "badge phase-" + String(phase || "idle").toLowerCase();
  }

  function renderTables(data) {
    const reqBody = $("tbl-requests");
    reqBody.innerHTML = (data.requests || []).map((r) => `<tr>
      <td>${escapeHtml(r.method)}</td>
      <td>${escapeHtml(r.name)}</td>
      <td>${r.success_count + r.fail_count}</td>
      <td>${r.fail_count}</td>
      <td>${fmt(r.requests_per_second)}</td>
      <td>${fmt(r.failures_per_second)}</td>
      <td>${fmt(r.response_time_average, 1)}</td>
      <td>${r.response_time_minimum}</td>
      <td>${r.response_time_maximum}</td>
      <td>${r.p50}</td>
      <td>${r.p95}</td>
      <td>${r.p99}</td>
    </tr>`).join("");

    $("tbl-transactions").innerHTML = (data.transactions || []).map((t) => `<tr>
      <td>${escapeHtml(t.scenario)}</td>
      <td>${escapeHtml(t.name)}</td>
      <td>${t.times_run}</td>
      <td>${t.fails}</td>
      <td>${fmt(t.transactions_per_second)}</td>
      <td>${fmt(t.fail_per_second)}</td>
      <td>${fmt(t.response_time_average, 1)}</td>
      <td>${t.response_time_minimum}</td>
      <td>${t.response_time_maximum}</td>
    </tr>`).join("");

    $("tbl-scenarios").innerHTML = (data.scenarios || []).map((s) => `<tr>
      <td>${escapeHtml(s.name)}</td>
      <td>${s.users}</td>
      <td>${s.times_run}</td>
      <td>${fmt(s.scenarios_per_second)}</td>
      <td>${fmt(s.response_time_average, 1)}</td>
      <td>${s.response_time_minimum}</td>
      <td>${s.response_time_maximum}</td>
    </tr>`).join("");

    $("tbl-errors").innerHTML = (data.top_errors || []).map((e) => `<tr>
      <td>${e.count}</td>
      <td style="white-space:normal;font-family:var(--sans)">${escapeHtml(e.message)}</td>
    </tr>`).join("");
  }

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function applySummary(data) {
    setPhase(data.phase);
    $("elapsed").textContent = (data.elapsed_secs || 0) + "s";
    $("hosts").textContent = (data.hosts || []).join(", ");
    $("kpi-users").textContent = data.active_users ?? 0;
    $("kpi-rps").textContent = fmt(data.requests_per_second);
    $("kpi-fail").textContent = fmt(data.fail_percent, 2) + "%";
    $("kpi-p95").textContent = data.p95 ?? "—";
    $("kpi-reqs").textContent = data.total_requests ?? 0;
    const errCount = (data.top_errors || []).reduce((a, e) => a + (e.count || 0), 0);
    $("kpi-errs").textContent = errCount;

    const ts = data.timeseries || {};
    const xs = (ts.elapsed_secs || []).map(String);
    charts.rps.setOption(lineOption("RPS", xs, ts.requests_per_second || [], "#3d9cf0"), true);
    charts.users.setOption(lineOption("Users", xs, ts.users || [], "#3dd68c"), true);
    charts.rt.setOption(lineOption("Avg RT", xs, ts.average_response_time_ms || [], "#f5a524"), true);

    renderTables(data);
  }

  document.querySelectorAll(".tab").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll(".tab").forEach((b) => b.classList.remove("active"));
      document.querySelectorAll(".panel").forEach((p) => p.classList.remove("active"));
      btn.classList.add("active");
      const panel = document.getElementById("panel-" + btn.dataset.tab);
      if (panel) panel.classList.add("active");
    });
  });

  window.addEventListener("resize", () => {
    Object.values(charts).forEach((c) => c.resize());
  });

  let pollTimer = null;
  function startPolling() {
    if (pollTimer) return;
    setConn("polling");
    const tick = async () => {
      try {
        const res = await fetch("/api/v1/summary");
        if (!res.ok) throw new Error("bad status");
        applySummary(await res.json());
      } catch (e) {
        setConn("disconnected");
      }
    };
    tick();
    pollTimer = setInterval(tick, 1000);
  }

  function startSse() {
    if (!window.EventSource) {
      startPolling();
      return;
    }
    const es = new EventSource("/api/v1/stream");
    es.addEventListener("summary", (ev) => {
      setConn("live");
      try {
        applySummary(JSON.parse(ev.data));
      } catch (e) { /* ignore parse errors */ }
    });
    es.onerror = () => {
      es.close();
      startPolling();
    };
  }

  startSse();
})();
