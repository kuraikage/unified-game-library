import { useEffect, useState } from 'react';
import { api } from '../api';
import ExternalLink from './ExternalLink';
import { buildSteamFamilyBookmarklet } from '../steamFamilySnippet';

export default function SteamFamilyImport({ count }) {
  const [port, setPort] = useState(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState(null);

  useEffect(() => {
    api.bookmarkletPort().then(setPort).catch(() => setPort(null));
  }, []);

  async function copy() {
    if (!port) return;
    try {
      await navigator.clipboard.writeText(buildSteamFamilyBookmarklet(port));
      setCopied(true);
      setTimeout(() => setCopied(false), 2500);
    } catch {
      setError('Could not copy to the clipboard.');
    }
  }

  return (
    <div className="panel">
      <h2>Steam Family library</h2>
      <p>
        Steam's official API only returns games your own account owns — family-shared games
        aren't included. Fetching them needs a short-lived token from your logged-in Steam
        session, which this bookmarklet reads and hands over. The token is used once and never
        stored.
      </p>

      <ol className="steps">
        <li>Click <strong>Copy bookmarklet</strong> and save it as a browser bookmark (one-time setup).</li>
        <li>
          Make sure you're logged in at{' '}
          <ExternalLink href="https://store.steampowered.com">store.steampowered.com</ExternalLink>{' '}
          (or{' '}
          <ExternalLink href="https://steamcommunity.com">steamcommunity.com</ExternalLink>).
        </li>
        <li>Click the bookmark. Shared games appear here automatically.</li>
      </ol>
      <p className="hint">
        If it reports it can't read the token, open the browser console (F12) — the snippet logs
        exactly what Steam returned, which usually shows they've moved the endpoint again.
      </p>

      <div className="button-row">
        <button type="button" onClick={copy} disabled={!port}>
          {copied ? 'Copied!' : 'Copy bookmarklet'}
        </button>
      </div>

      <p className="hint">
        {count > 0
          ? `${count} family-shared games in your library.`
          : 'No family-shared games imported yet.'}{' '}
        The token expires after about a day, so re-run the bookmarklet when you want to refresh.
      </p>
      {error && <p className="error">{error}</p>}
    </div>
  );
}
