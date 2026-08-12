import { useEffect, useState } from 'react';
import { api } from '../api';
import ExternalLink from './ExternalLink';
import { buildSnippet, buildBookmarklet } from '../epicSnippet';

export default function EpicImport({ onImported, compact = false }) {
  const [pasted, setPasted] = useState('');
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState(null);
  const [copied, setCopied] = useState(null);
  const [port, setPort] = useState(null);

  useEffect(() => {
    api.bookmarkletPort().then(setPort).catch(() => setPort(null));
  }, []);

  async function copy(kind) {
    if (!port) return;
    const text = kind === 'bookmarklet' ? buildBookmarklet(port) : buildSnippet(port);
    try {
      await navigator.clipboard.writeText(text);
      setCopied(kind);
      setTimeout(() => setCopied(null), 2500);
    } catch {
      setError('Could not copy to the clipboard.');
    }
  }

  async function handleImport() {
    setImporting(true);
    setError(null);
    try {
      await api.importEpicLibrary(pasted);
      setPasted('');
      onImported();
    } catch (err) {
      setError(String(err));
    } finally {
      setImporting(false);
    }
  }

  return (
    <div className="panel">
      <h2>{compact ? 'Re-import Epic library' : 'Connect Epic'}</h2>
      <p>
        Epic has no official library API, so this reads your own purchase history in your own
        logged-in browser and hands it straight to UGLy. Nothing is automated on Epic's side and no
        login details are ever stored.
      </p>

      <ol className="steps">
        <li>
          Click <strong>Copy bookmarklet</strong> below.
        </li>
        <li>
          In your browser, create a new bookmark (Ctrl+Shift+O → Add), and paste the copied text as
          the bookmark's <strong>URL</strong>. Name it anything, e.g. "Import to UGLy".
          <span className="hint"> One-time setup.</span>
        </li>
        <li>
          Go to{' '}
          <ExternalLink href="https://www.epicgames.com/account/transactions">
            epicgames.com/account/transactions
          </ExternalLink>{' '}
          and make sure you're logged in.
        </li>
        <li>Click the bookmark. Your library lands here automatically — this page updates itself.</li>
      </ol>

      <div className="button-row">
        <button type="button" onClick={() => copy('bookmarklet')} disabled={!port}>
          {copied === 'bookmarklet' ? 'Copied!' : 'Copy bookmarklet'}
        </button>
      </div>

      <details>
        <summary>Prefer the DevTools console? Or need to paste the result manually?</summary>
        <ol className="steps">
          <li>
            On the transactions page press F12, open the <strong>Console</strong> tab, paste the
            snippet below and press Enter.
          </li>
          <li>
            If it can't reach UGLy it copies the result to your clipboard instead — paste that
            below and click Import.
          </li>
        </ol>
        <div className="button-row">
          <button type="button" className="secondary" onClick={() => copy('snippet')} disabled={!port}>
            {copied === 'snippet' ? 'Copied!' : 'Copy console snippet'}
          </button>
        </div>
        <textarea
          value={pasted}
          onChange={(e) => setPasted(e.target.value)}
          placeholder="Paste the JSON copied from the console here"
          rows={6}
        />
        <div className="button-row">
          <button type="button" onClick={handleImport} disabled={importing || !pasted.trim()}>
            {importing ? 'Importing...' : 'Import'}
          </button>
        </div>
      </details>

      {error && <p className="error">{error}</p>}
    </div>
  );
}
