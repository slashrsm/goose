import init from './goose_dashboard_ui.js';
init().catch((err) => {
  console.error('Goose dashboard WASM failed to load', err);
  document.body.innerHTML = '<pre style="color:#f31260;padding:2rem">Failed to load WASM dashboard UI. Rebuild with scripts/build-dashboard-ui.sh</pre>';
});
