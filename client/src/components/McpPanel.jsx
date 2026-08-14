import { useEffect, useState } from 'react';
import { api } from '../api';

// Shown last in settings: entirely optional, and only useful once a library exists.
export default function McpPanel() {
  const [info, setInfo] = useState(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState(null);

  useEffect(() => {
    api
      .getMcpInfo()
      .then(setInfo)
      .catch((err) => setError(String(err)));
  }, []);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(info.config);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      setError(`Could not copy: ${err}`);
    }
  }

  return (
    <div className="panel">
      <h2>Ask an AI what to play</h2>
      <p className="hint">
        UGLy ships a small MCP server that lets Claude, Codex or any other MCP client read this
        library and answer questions about it — "what short indie game should I finish this
        weekend?" — and mark games as playing, completed or dropped. It reads the same data you see
        here. It cannot launch or install anything.
      </p>

      {error && <p className="error">{error}</p>}

      {info && !info.available && (
        <p className="hint">
          The MCP server isn't bundled with this build. It ships with the installer — or build it
          yourself with <code>npm run build:mcp</code>.
        </p>
      )}

      {info?.available && (
        <>
          <p className="hint">
            Add this to your MCP client's config, then restart it. In Claude Desktop that's{' '}
            <code>claude_desktop_config.json</code>; for Claude Code, run{' '}
            <code>claude mcp add ugly -- "{info.path}"</code>.
          </p>
          <pre className="snippet">{info.config}</pre>
          <div className="button-row">
            <button type="button" onClick={handleCopy}>
              {copied ? 'Copied!' : 'Copy config'}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
