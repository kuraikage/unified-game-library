import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import './App.css';
import { api } from './api';
import { slugify } from './slugify';
import { buildHaystack, matchesSearch } from './search';
import { buildLibraryCsv, downloadCsv } from './exportCsv';
import SettingsPanel from './components/SettingsPanel';
import EpicImport from './components/EpicImport';
import SteamFamilyImport from './components/SteamFamilyImport';
import FilterBar from './components/FilterBar';
import GameGrid from './components/GameGrid';
import GameList from './components/GameList';
import SegmentedControl from './components/SegmentedControl';
import { LibraryIcon, SettingsIcon } from './icons';

const EMPTY_JOB = { running: false, total: 0, completed: 0, error: null };

export default function App() {
  const [loading, setLoading] = useState(true);
  const [settings, setSettings] = useState({
    steamId: '',
    steamConfigured: false,
    igdbConfigured: false,
  });
  const [steamGames, setSteamGames] = useState([]);
  const [familyGames, setFamilyGames] = useState([]);
  const [epicGames, setEpicGames] = useState([]);
  const [epicImportedAt, setEpicImportedAt] = useState(null);
  const [error, setError] = useState(null);
  const [view, setView] = useState('library');
  const [search, setSearch] = useState('');
  const [platform, setPlatform] = useState('all');
  const [viewMode, setViewMode] = useState('grid');
  const [refreshing, setRefreshing] = useState(false);
  const [metadata, setMetadata] = useState({});
  const [metadataJob, setMetadataJob] = useState(EMPTY_JOB);
  const [installed, setInstalled] = useState({});
  const [installedOnly, setInstalledOnly] = useState(false);
  const [notice, setNotice] = useState(null);
  const enrichRequested = useRef(false);

  const loadAll = useCallback(async () => {
    setError(null);
    try {
      const [nextSettings, epic, steam, family, meta, job, installedMap] = await Promise.all([
        api.getSettings(),
        api.getEpicLibrary(),
        api.getSteamLibrary(),
        api.getFamilyLibrary().catch(() => []),
        api.getMetadata(),
        api.getEnrichmentJob(),
        api.getInstalled().catch(() => ({})),
      ]);

      setSettings(nextSettings);
      setEpicGames(epic.games ?? []);
      setEpicImportedAt(epic.importedAt ?? null);
      setSteamGames(steam ?? []);
      setFamilyGames(family ?? []);
      setMetadata(meta ?? {});
      setMetadataJob(job ?? EMPTY_JOB);
      setInstalled(installedMap ?? {});

      if (!nextSettings.steamConfigured && (epic.games ?? []).length === 0) {
        setView('settings');
      }
      return nextSettings;
    } catch (err) {
      setError(String(err));
      return null;
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadAll();
  }, [loadAll]);

  // Rust pushes these, so the UI reacts without polling.
  useEffect(() => {
    const unlisten = [];
    api.onEnrichmentProgress((job) => {
      setMetadataJob(job);
      if (!job.running) api.getMetadata().then(setMetadata).catch(() => {});
    }).then((fn) => unlisten.push(fn));

    api.onEpicImported(() => {
      loadAll();
      setView('library');
    }).then((fn) => unlisten.push(fn));

    api.onFamilyImported((count) => {
      loadAll();
      setNotice(`Imported ${count} family-shared games.`);
      setTimeout(() => setNotice(null), 5000);
    }).then((fn) => unlisten.push(fn));

    return () => unlisten.forEach((fn) => fn());
  }, [loadAll]);

  const allGames = useMemo(() => {
    // A family game you also own would arrive from both sources; owned wins.
    const owned = new Set(steamGames.map((g) => g.id));
    const shared = familyGames
      .filter((g) => !owned.has(g.id))
      .map((g) => ({ ...g, shared: true }));
    return [...steamGames, ...shared, ...epicGames];
  }, [steamGames, familyGames, epicGames]);

  // Any game with no cached lookup gets fetched automatically. Rust filters to just the
  // missing ones, so passing the whole list is cheap.
  useEffect(() => {
    if (loading || !settings.igdbConfigured || metadataJob.running || allGames.length === 0) return;
    const missing = allGames.filter((g) => !metadata[slugify(g.title)]);
    if (missing.length === 0) {
      enrichRequested.current = false;
      return;
    }
    if (enrichRequested.current) return;
    enrichRequested.current = true;

    api
      .enrichMetadata(allGames.map((g) => g.title))
      .then(setMetadataJob)
      .catch((err) => setError(String(err)));
  }, [loading, settings.igdbConfigured, metadataJob.running, allGames, metadata]);

  async function handleRefreshSteam() {
    setRefreshing(true);
    setError(null);
    try {
      setSteamGames(await api.refreshSteamLibrary());
      // Installed matches are derived from what's in the library, so they have to be
      // recomputed once new games land — otherwise nothing shows as installed.
      setInstalled(await api.getInstalled().catch(() => ({})));
      enrichRequested.current = false;
    } catch (err) {
      setError(String(err));
    } finally {
      setRefreshing(false);
    }
  }

  // After saving credentials for the first time there's nothing in the library yet, so
  // fetch straight away rather than leaving the user on an empty grid.
  async function handleSettingsSaved() {
    const next = await loadAll();
    if (next?.steamConfigured) {
      await handleRefreshSteam();
    }
  }

  const games = useMemo(() => {
    const term = search.trim();
    return [...allGames]
      .sort((a, b) => a.title.localeCompare(b.title))
      .filter((g) => {
        if (platform !== 'all' && g.platform !== platform) return false;
        if (installedOnly && !installed[g.id]) return false;
        if (!term) return true;
        return matchesSearch(buildHaystack(g, metadata[slugify(g.title)]), term);
      });
  }, [allGames, search, platform, metadata, installedOnly, installed]);

  const installedCount = useMemo(
    () => allGames.filter((g) => installed[g.id]).length,
    [allGames, installed]
  );

  async function handleLaunch(game) {
    setNotice(null);
    try {
      await api.launchGame(game.id);
      setNotice(`Launching ${game.title}...`);
      setTimeout(() => setNotice(null), 4000);
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleInstall(game) {
    setNotice(null);
    try {
      await api.installGame(game);
      setNotice(
        game.platform === 'steam'
          ? `Opening Steam to install ${game.title}...`
          : `Opening the Epic store for ${game.title}...`
      );
      setTimeout(() => setNotice(null), 4000);
    } catch (err) {
      setError(String(err));
    }
  }

  function handleExportCsv() {
    const stamp = new Date().toISOString().slice(0, 10);
    downloadCsv(`ugly-library-${stamp}.csv`, buildLibraryCsv(games, metadata));
  }

  const missingMetadataCount = useMemo(
    () => allGames.filter((g) => !metadata[slugify(g.title)]).length,
    [allGames, metadata]
  );

  return (
    <div className="app">
      <header className="app-header">
        <div className="brand">
          <img
            src="/icon.png"
            alt=""
            className="brand-icon"
            onError={(e) => (e.target.style.display = 'none')}
          />
          <div>
            <h1>UGLy</h1>
            <p className="tagline">Unified Game Library. All your games. One place.</p>
          </div>
        </div>
        <nav>
          <SegmentedControl
            ariaLabel="Main navigation"
            size="lg"
            value={view}
            onChange={setView}
            options={[
              { value: 'library', label: 'Library', icon: <LibraryIcon /> },
              { value: 'settings', label: 'Settings', icon: <SettingsIcon /> },
            ]}
          />
        </nav>
      </header>

      {error && <p className="error">{error}</p>}
      {notice && <p className="notice">{notice}</p>}

      {loading ? (
        <p className="loading">Loading your library...</p>
      ) : (
        <main key={view}>
          {view === 'settings' && (
            <div className="settings-stack">
              <SettingsPanel
                steamId={settings.steamId}
                steamConfigured={settings.steamConfigured}
                igdbConfigured={settings.igdbConfigured}
                onSaved={handleSettingsSaved}
              />

              <SteamFamilyImport count={familyGames.length} />

              <EpicImport
                compact={Boolean(epicImportedAt)}
                onImported={() => {
                  loadAll();
                  setView('library');
                }}
              />

              <div className="panel">
                <h2>Library data</h2>
                <p className="hint">
                  {steamGames.length} Steam · {familyGames.length} family-shared ·{' '}
                  {epicGames.length} Epic
                  {epicImportedAt ? <> · Epic imported {new Date(epicImportedAt).toLocaleString()}</> : null}
                </p>
                <div className="button-row">
                  <button type="button" onClick={handleRefreshSteam} disabled={refreshing}>
                    {refreshing ? 'Refreshing Steam...' : 'Refresh Steam'}
                  </button>
                </div>
                <p className="hint">
                  {!settings.igdbConfigured
                    ? 'Add IGDB credentials above to fetch genres, tags and artwork.'
                    : metadataJob.running
                      ? `Fetching genres & art... ${metadataJob.completed}/${metadataJob.total}`
                      : missingMetadataCount > 0
                        ? `${missingMetadataCount} games waiting on genre/art lookup.`
                        : 'Genres and artwork are up to date. New games are fetched automatically.'}
                </p>
                {metadataJob.error && <p className="error">{metadataJob.error}</p>}
              </div>
            </div>
          )}

          {view === 'library' && (
            <>
              <FilterBar
                search={search}
                onSearchChange={setSearch}
                platform={platform}
                onPlatformChange={setPlatform}
                viewMode={viewMode}
                onViewModeChange={setViewMode}
                installedOnly={installedOnly}
                onInstalledOnlyChange={setInstalledOnly}
                installedCount={installedCount}
                count={games.length}
                onExport={handleExportCsv}
              />
              {metadataJob.running && (
                <p className="hint">
                  Fetching genres &amp; art... {metadataJob.completed}/{metadataJob.total}
                </p>
              )}
              {viewMode === 'grid' ? (
                <GameGrid
                  key="grid"
                  games={games}
                  metadata={metadata}
                  installed={installed}
                  onLaunch={handleLaunch}
                  onInstall={handleInstall}
                />
              ) : (
                <GameList
                  key="list"
                  games={games}
                  metadata={metadata}
                  installed={installed}
                  onLaunch={handleLaunch}
                  onInstall={handleInstall}
                />
              )}
            </>
          )}
        </main>
      )}
    </div>
  );
}
