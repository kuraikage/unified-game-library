import { useState } from 'react';
import { api } from '../api';
import ExternalLink from './ExternalLink';

export default function SettingsPanel({ steamId: initialSteamId, steamConfigured, igdbConfigured, onSaved }) {
  const [steamApiKey, setSteamApiKey] = useState('');
  const [steamId, setSteamId] = useState(initialSteamId ?? '');
  const [igdbClientId, setIgdbClientId] = useState('');
  const [igdbClientSecret, setIgdbClientSecret] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(null);
  const [saved, setSaved] = useState(false);

  async function handleSubmit(e) {
    e.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await api.saveSettings({ steamApiKey, steamId, igdbClientId, igdbClientSecret });
      // Secrets are never read back, so clear the inputs once they're stored.
      setSteamApiKey('');
      setIgdbClientId('');
      setIgdbClientSecret('');
      setSaved(true);
      setTimeout(() => setSaved(false), 2500);
      onSaved();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="panel">
      <h2>Connections</h2>
      <p className="hint">
        Keys are stored in the Windows Credential Manager and are never sent to this window — only
        whether they're set. Leave a field blank to keep the saved value.
      </p>

      <form onSubmit={handleSubmit} className="form">
        <div className="field-head">
          <strong>Steam</strong>
          <span className={`status-dot ${steamConfigured ? 'ok' : 'off'}`}>
            {steamConfigured ? 'Connected' : 'Not connected'}
          </span>
        </div>
        <p className="hint">
          Get a Web API key from{' '}
          <ExternalLink href="https://steamcommunity.com/dev/apikey">
            steamcommunity.com/dev/apikey
          </ExternalLink>{' '}
          and your SteamID64 from{' '}
          <ExternalLink href="https://steamid.io">
            steamid.io
          </ExternalLink>
          .
        </p>
        <label>
          Steam Web API Key
          <input
            type="password"
            value={steamApiKey}
            onChange={(e) => setSteamApiKey(e.target.value)}
            placeholder={steamConfigured ? '•••••••• (saved)' : 'Paste your key'}
            autoComplete="off"
          />
        </label>
        <label>
          SteamID64
          <input
            type="text"
            value={steamId}
            onChange={(e) => setSteamId(e.target.value)}
            placeholder="e.g. 76561198000000000"
          />
        </label>

        <hr />

        <div className="field-head">
          <strong>IGDB</strong>
          <span className={`status-dot ${igdbConfigured ? 'ok' : 'off'}`}>
            {igdbConfigured ? 'Connected' : 'Not connected'}
          </span>
        </div>
        <p className="hint">
          Optional — provides genres, tags and artwork. Register a free app at the{' '}
          <ExternalLink href="https://dev.twitch.tv/console/apps">
            Twitch developer console
          </ExternalLink>
          .
        </p>
        <label>
          IGDB (Twitch) Client ID
          <input
            type="password"
            value={igdbClientId}
            onChange={(e) => setIgdbClientId(e.target.value)}
            placeholder={igdbConfigured ? '•••••••• (saved)' : 'Optional'}
            autoComplete="off"
          />
        </label>
        <label>
          IGDB (Twitch) Client Secret
          <input
            type="password"
            value={igdbClientSecret}
            onChange={(e) => setIgdbClientSecret(e.target.value)}
            placeholder={igdbConfigured ? '•••••••• (saved)' : 'Optional'}
            autoComplete="off"
          />
        </label>

        {error && <p className="error">{error}</p>}
        <div className="button-row">
          <button type="submit" disabled={saving}>
            {saving ? 'Saving...' : saved ? 'Saved!' : 'Save'}
          </button>
        </div>
      </form>
    </div>
  );
}
