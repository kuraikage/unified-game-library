/**
 * Grabs the short-lived `webapi_token` from a logged-in Steam web session and hands it to
 * UGLy, which uses it once to read your Family's shared library. Steam's Web API key cannot
 * do this — the family endpoints only accept a session token.
 *
 * Steam has moved this token around between endpoints and response shapes, so the snippet
 * tries the known sources for whichever Steam domain it is run from, and reports what it
 * actually received if none of them yield a token.
 *
 * The token is never stored by the app; it's used for the two API calls and discarded.
 */
const buildScript = (port) => `(async () => {
  const IMPORT_URL = 'http://127.0.0.1:${port}/api/steam/family';
  const host = location.hostname;

  // Same-origin only: fetching steamcommunity.com from the store domain (or vice versa)
  // is cross-origin and gets blocked, so pick sources matching where we're running.
  const sources = host.indexOf('steamcommunity.com') !== -1
    ? [
        'https://steamcommunity.com/chat/clientjstoken',
        'https://steamcommunity.com/pointssummary/ajaxgetasyncconfig',
      ]
    : [
        'https://store.steampowered.com/pointssummary/ajaxgetasyncconfig',
        'https://store.steampowered.com/chat/clientjstoken',
      ];

  // The token has appeared at the top level, under "data", and as "token".
  function extract(obj) {
    if (!obj || typeof obj !== 'object') return null;
    return obj.webapi_token
      || obj.token
      || (obj.data && (obj.data.webapi_token || obj.data.token))
      || null;
  }

  let token = null;
  const attempts = [];
  for (const url of sources) {
    try {
      const res = await fetch(url, { credentials: 'include' });
      const text = await res.text();
      let parsed = null;
      try { parsed = JSON.parse(text); } catch (e) { /* not JSON */ }
      token = extract(parsed);
      attempts.push({ url, status: res.status, body: text.slice(0, 300) });
      if (token) break;
    } catch (e) {
      attempts.push({ url, error: String(e) });
    }
  }

  if (!token) {
    console.group('UGLy: could not find a Steam token');
    attempts.forEach((a) => console.log(a));
    console.groupEnd();
    alert(
      'Could not read your Steam token.\\n\\n' +
      'Make sure you are logged in, then open the browser console (F12) — the responses ' +
      'Steam returned have been logged there, which will show what changed.'
    );
    return;
  }

  const form = document.createElement('form');
  form.method = 'POST';
  form.action = IMPORT_URL;
  const field = document.createElement('input');
  field.type = 'hidden';
  field.name = 'token';
  field.value = token;
  form.appendChild(field);
  document.body.appendChild(form);
  form.submit();
})();`;

export const buildSteamFamilySnippet = (port) => buildScript(port);

export const buildSteamFamilyBookmarklet = (port) =>
  `javascript:${encodeURIComponent(buildScript(port))}`;
