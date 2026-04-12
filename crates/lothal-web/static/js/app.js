/**
 * Lothal Property Intelligence — Client-side JS
 *
 * Responsibilities:
 * 1. Initialize Chart.js instances from data-chart-config attributes
 * 2. Handle chart updates from htmx swaps
 * 3. WebSocket connection for real-time readings (Phase 6)
 */

// ---------------------------------------------------------------------------
// Chart.js initialization
// ---------------------------------------------------------------------------

// Store chart instances so we can destroy them on re-render.
const chartInstances = {};

function initCharts() {
  document.querySelectorAll('.lothal-chart').forEach(canvas => {
    const configStr = canvas.getAttribute('data-chart-config');
    if (!configStr || configStr === '{}') return;

    // Destroy existing instance if it exists (htmx re-render).
    const id = canvas.id;
    if (chartInstances[id]) {
      chartInstances[id].destroy();
    }

    try {
      const config = JSON.parse(configStr);
      chartInstances[id] = new Chart(canvas, config);
    } catch (e) {
      console.error('Failed to parse chart config for', id, e);
    }
  });
}

// Initialize charts on page load.
document.addEventListener('DOMContentLoaded', initCharts);

// Re-initialize charts after htmx swaps new content in.
document.addEventListener('htmx:afterSwap', function(event) {
  // Small delay to let the DOM settle.
  requestAnimationFrame(initCharts);
});

// ---------------------------------------------------------------------------
// WebSocket for real-time readings (Phase 6)
// ---------------------------------------------------------------------------

let ws = null;
let reconnectTimer = null;

function connectWebSocket() {
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
  const url = `${protocol}//${location.host}/ws/readings`;

  ws = new WebSocket(url);

  ws.onopen = () => {
    console.log('[lothal] WebSocket connected');
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  };

  ws.onmessage = (event) => {
    try {
      const reading = JSON.parse(event.data);
      handleRealtimeReading(reading);
    } catch (e) {
      // Ignore non-JSON messages.
    }
  };

  ws.onclose = () => {
    console.log('[lothal] WebSocket closed, reconnecting in 5s');
    reconnectTimer = setTimeout(connectWebSocket, 5000);
  };

  ws.onerror = () => {
    ws.close();
  };
}

function handleRealtimeReading(reading) {
  // Update live power display if on energy page.
  const livePower = document.getElementById('live-power-value');
  if (livePower && reading.kind === 'electric_watts') {
    livePower.textContent = Math.round(reading.value).toLocaleString();
  }
}

// Connect WebSocket when on pages that need it.
document.addEventListener('DOMContentLoaded', () => {
  // Only connect if we're on a page with real-time elements.
  if (document.getElementById('live-power-value')) {
    connectWebSocket();
  }
});

// ---------------------------------------------------------------------------
// Utility: format numbers
// ---------------------------------------------------------------------------

function formatNumber(n, decimals = 1) {
  return n.toLocaleString('en-US', {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });
}
