const buildScript = (port) => `(async () => {
  const BASE = 'https://accounts.epicgames.com/account/v2/payment/ajaxGetOrderHistory';
  const IMPORT_URL = 'http://127.0.0.1:${port}/api/epic/import';
  const titleKeys = ['title', 'description', 'offerTitle', 'name', 'itemTitle'];
  const imageKeys = ['image', 'keyImage', 'thumbnail', 'imageUrl', 'coverUrl'];
  const idKeys = ['offerId', 'id', 'catalogItemId'];
  const COUNT = 50; // requested page size (the server may cap this lower — we paginate regardless)
  const MAX_PAGES = 200; // safety cap on number of requests

  function pick(obj, keys) {
    for (const k of keys) if (obj && obj[k]) return obj[k];
    return null;
  }

  const found = new Map();
  let nextPageToken = null;
  let pagesFetched = 0;
  let ordersSeen = 0;

  while (pagesFetched < MAX_PAGES) {
    let url = \`\${BASE}?count=\${COUNT}&sortDir=DESC&sortBy=DATE&locale=en-US\`;
    if (nextPageToken) url += \`&nextPageToken=\${encodeURIComponent(nextPageToken)}\`;

    const res = await fetch(url, { credentials: 'include' });
    if (!res.ok) {
      console.error('Request failed on page', pagesFetched + 1, res.status);
      break;
    }
    const data = await res.json();
    pagesFetched += 1;

    const orders = data.orders || [];
    if (orders.length === 0) break;
    ordersSeen += orders.length;

    for (const order of orders) {
      if (order.orderType && order.orderType !== 'PURCHASE') continue; // skip refunds etc.
      for (const item of order.items || []) {
        if (item.giftRecipient) continue; // gifted to someone else, not yours
        if (item.namespace === 'ue') continue; // Unreal Engine Marketplace/Fab asset, not a game
        const title = pick(item, titleKeys);
        if (!title || typeof title !== 'string') continue;
        const offerId = pick(item, idKeys) || title;
        if (found.has(offerId)) continue;
        found.set(offerId, { title, offerId, image: pick(item, imageKeys) });
      }
    }

    const oldestOrder = orders[orders.length - 1];
    const nextToken = oldestOrder && oldestOrder.createdAtMillis
      ? new Date(oldestOrder.createdAtMillis).toISOString()
      : null;
    if (!nextToken || nextToken === nextPageToken) break; // no cursor available or stuck, stop
    nextPageToken = nextToken;
  }

  const games = [...found.values()];
  console.log(\`Found \${games.length} games across \${ordersSeen} orders (\${pagesFetched} page(s)).\`);
  console.table(games.map((g) => ({ title: g.title, offerId: g.offerId })));

  if (games.length === 0) {
    alert('No games found — see the browser console for details.');
    console.warn('No games found. Open the Network tab, inspect the ajaxGetOrderHistory response shape, and report it back so the snippet field-mapping can be fixed.');
    return;
  }

  // Epic's Content Security Policy blocks fetch/XHR to localhost from this page, but it does not
  // block navigation — so hand the data to the app by POSTing a real form submission instead.
  // This also avoids clipboard permission/user-gesture limits entirely.
  try {
    const form = document.createElement('form');
    form.method = 'POST';
    form.action = IMPORT_URL;
    // Same tab on purpose: by now the original click gesture has expired (we awaited network
    // calls), so a _blank target gets caught by popup blockers. The result page links back.
    const field = document.createElement('input');
    field.type = 'hidden';
    field.name = 'data';
    field.value = JSON.stringify(games);
    form.appendChild(field);
    document.body.appendChild(form);
    form.submit();
    document.body.removeChild(form);
    return;
  } catch (e) {
    console.warn('Could not hand off to the app directly:', e);
  }

  // Fall back to clipboard, e.g. if the app isn't running or the handoff above was blocked.
  const json = JSON.stringify(games, null, 2);
  let copied = false;
  // 'copy' is a DevTools console-only utility (not available in page scripts) —
  // most reliable when this runs from a console paste rather than the bookmarklet.
  if (typeof copy === 'function') {
    try {
      copy(json);
      copied = true;
    } catch (e) {
      // ignore, fall back below
    }
  }
  if (!copied) {
    try {
      await navigator.clipboard.writeText(json);
      copied = true;
    } catch (e) {
      // ignore, fall back below
    }
  }
  if (!copied) {
    try {
      const textarea = document.createElement('textarea');
      textarea.value = json;
      textarea.style.position = 'fixed';
      textarea.style.opacity = '0';
      document.body.appendChild(textarea);
      textarea.focus();
      textarea.select();
      copied = document.execCommand('copy');
      document.body.removeChild(textarea);
    } catch (e) {
      // ignore, fall back below
    }
  }
  if (copied) {
    alert(\`Could not auto-import, but copied \${games.length} games to your clipboard — paste them into the Epic Import box in the app.\`);
  } else {
    alert('Could not auto-import or copy automatically — open the browser console to copy the result manually.');
    console.warn('Could not auto-copy. Right-click the array printed below and choose "Copy object", or select it manually:');
    console.log(json);
  }
})();`;

/** Raw script, for pasting into the browser's DevTools console. */
export const buildSnippet = (port) => buildScript(port);

/** Same script as a `javascript:` URL, for saving as a browser bookmark. */
export const buildBookmarklet = (port) => `javascript:${encodeURIComponent(buildScript(port))}`;
