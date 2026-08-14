import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import './App.css';
import { api } from './api';
import { slugify } from './slugify';
import { buildHaystack, matchesSearch } from './search';
import { buildLibraryCsv, downloadCsv } from './exportCsv';
import SettingsPanel from './components/SettingsPanel';
import EpicImport from './components/EpicImport';
import SteamFamilyImport from './components/SteamFamilyImport';
import McpPanel from './components/McpPanel';
import FilterBar from './components/FilterBar';
import GameGrid from './components/GameGrid';
import GameList from './components/GameList';
import GameDetail from './components/GameDetail';
import { coverSources } from './covers';
import SegmentedControl from './components/SegmentedControl';
import StatusStrip from './components/StatusStrip';
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
  const [statuses, setStatuses] = useState({});
  const [statusFilter, setStatusFilter] = useState('all');
  const [notice, setNotice] = useState(null);
  const [detailGame, setDetailGame] = useState(null);
  const enrichRequested = useRef(false);

  const loadAll = useCallback(async () => {
    setError(null);
    try {
      const [nextSettings, epic, steam, family, meta, job, installedMap, statusMap] =
        await Promise.all([
          api.getSettings(),
          api.getEpicLibrary(),
          api.getSteamLibrary(),
          api.getFamilyLibrary().catch(() => []),
          api.getMetadata(),
          api.getEnrichmentJob(),
          api.getInstalled().catch(() => ({})),
          api.getStatuses().catch(() => ({})),
        ]);

      setSettings(nextSettings);
      setEpicGames(epic.games ?? []);
      setEpicImportedAt(epic.importedAt ?? null);
      setSteamGames(steam ?? []);
      setFamilyGames(family ?? []);
      setMetadata(meta ?? {});
      setMetadataJob(job ?? EMPTY_JOB);
      setInstalled(installedMap ?? {});
      setStatuses(statusMap ?? {});

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

    // The Steam pass gets no progress bar — it's seconds, not minutes — so this only
    // picks up the tags once it's finished.
    api.onSteamProgress((job) => {
      if (job.running) return;
      if (job.error) {
        console.warn('Steam tag lookup failed:', job.error);
        return;
      }
      api.getMetadata().then(setMetadata).catch(() => {});
    }).then((fn) => unlisten.push(fn));

    // Newly resolved appids mean more games can be tagged, so run the (cheap, batched)
    // tag pass again to pick them up.
    api.onAppidProgress((job) => {
      if (job.running) return;
      api.enrichSteamTags().catch(() => {});
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

  // Asked for once per session; Rust decides what actually needs fetching and returns
  // immediately when nothing does.
  //
  // Deliberately NOT gated on "does anything look missing from here?". Rust also re-fetches
  // rows whose cached shape predates the current tag schema, and those rows look present
  // from this side — gating on appearance would mean such a pass could never start.
  useEffect(() => {
    if (loading || !settings.igdbConfigured || allGames.length === 0) return;
    if (enrichRequested.current) return;
    enrichRequested.current = true;

    api
      .enrichMetadata()
      .then(setMetadataJob)
      .catch((err) => setError(String(err)));
  }, [loading, settings.igdbConfigured, allGames.length]);

  // Steam tags are a handful of batched requests for the whole library, so they run with
  // no progress UI. No credentials needed, which is why this isn't gated on settings the
  // way the IGDB pass is.
  //
  // Keyed on the library size rather than latched to first load, so refreshing Steam or
  // importing Epic games gets the new titles tagged straight away instead of at next
  // launch. Rust returns immediately when there is nothing outstanding, so re-asking is
  // cheap and a running pass can't be started twice.
  useEffect(() => {
    if (loading || allGames.length === 0) return;

    // Both return as soon as they start; results arrive on their progress events.
    api.enrichSteamTags().catch((err) => console.warn('Steam tag lookup failed:', err));
    // Finds Steam appids for Epic-only games so they get Steam tags too. Slow, cached
    // permanently, and shrinks to nothing after the first pass.
    api.resolveEpicAppids().catch((err) => console.warn('Steam appid lookup failed:', err));
  }, [loading, allGames.length]);

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

  // Manual trigger for the metadata pass. Clears the "already asked" latch so the
  // automatic effect can also retry after a failure.
  async function handleEnrichNow() {
    setError(null);
    enrichRequested.current = false;
    try {
      setMetadataJob(await api.enrichMetadata());
    } catch (err) {
      setError(String(err));
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
        if (statusFilter !== 'all') {
          const current = statuses[slugify(g.title)]?.status ?? null;
          // "backlog" is the absence of a status rather than a stored value.
          if (statusFilter === 'backlog' ? current !== null : current !== statusFilter) return false;
        }
        if (!term) return true;
        return matchesSearch(buildHaystack(g, metadata[slugify(g.title)]), term);
      });
  }, [allGames, search, platform, metadata, installedOnly, installed, statusFilter, statuses]);

  // Optimistic update: the button reflects the new state immediately and Rust returns the
  // authoritative map, so a rejected value corrects itself.
  const handleStatusChange = useCallback(async (game, next) => {
    const slug = slugify(game.title);
    setStatuses((prev) => {
      const copy = { ...prev };
      if (next) copy[slug] = { ...(copy[slug] ?? {}), status: next };
      else delete copy[slug];
      return copy;
    });
    try {
      setStatuses(await api.setGameStatus(slug, next));
    } catch (err) {
      setError(String(err));
      setStatuses(await api.getStatuses().catch(() => ({})));
    }
  }, []);

  const installedCount = useMemo(
    () => allGames.filter((g) => installed[g.id]).length,
    [allGames, installed]
  );

  // These are passed to every card, so they must keep a stable identity or memo()
  // on GameCard would never prevent a re-render.
  const handleLaunch = useCallback(async (game) => {
    setNotice(null);
    try {
      await api.launchGame(game.id);
      setNotice(`Launching ${game.title}...`);
      setTimeout(() => setNotice(null), 4000);
    } catch (err) {
      setError(String(err));
    }
  }, []);

  const handleInstall = useCallback(async (game) => {
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
  }, []);

  function handleExportCsv() {
    const stamp = new Date().toISOString().slice(0, 10);
    downloadCsv(`ugly-library-${stamp}.csv`, buildLibraryCsv(games, metadata, statuses, installed));
  }

  // Counts games IGDB has never resolved, which is what the "fetch genres & art" button
  // acts on — Steam tags don't make a game enriched for that purpose.
  const missingMetadataCount = useMemo(
    () => allGames.filter((g) => !metadata[slugify(g.title)]?.igdb).length,
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
              {/* Library data leads: it's what you come here to act on day to day,
                  whereas the credential panels are set-once. */}
              <div className="panel panel-wide">
                <h2>Library data</h2>
                <div className="stat-row">
                  <span className="stat">
                    <strong>{steamGames.length}</strong> Steam
                  </span>
                  <span className="stat">
                    <strong>{familyGames.length}</strong> family-shared
                  </span>
                  <span className="stat">
                    <strong>{epicGames.length}</strong> Epic
                  </span>
                  <span className="stat total">
                    <strong>{allGames.length}</strong> total
                  </span>
                  {epicImportedAt && (
                    <span className="stat muted">
                      Epic imported {new Date(epicImportedAt).toLocaleString()}
                    </span>
                  )}
                </div>
                <div className="button-row">
                  <button type="button" onClick={handleRefreshSteam} disabled={refreshing}>
                    {refreshing ? 'Refreshing Steam...' : 'Refresh Steam'}
                  </button>
                  {/* Escape hatch: enrichment normally starts on its own, but if it stalls
                      or errored there needs to be a way to kick it off by hand. */}
                  {settings.igdbConfigured && missingMetadataCount > 0 && (
                    <button
                      type="button"
                      className="secondary"
                      onClick={handleEnrichNow}
                      disabled={metadataJob.running}
                    >
                      {metadataJob.running ? 'Fetching...' : `Fetch genres & art (${missingMetadataCount})`}
                    </button>
                  )}
                </div>
                <p className="hint">
                  {!settings.igdbConfigured
                    ? 'Add IGDB credentials above to fetch genres, tags and artwork.'
                    : metadataJob.running
                      ? `Looking up genres, tags and artwork on IGDB — ${metadataJob.completed} of ${metadataJob.total} done. This runs in the background; you can keep using the app.`
                      : missingMetadataCount > 0
                        ? `${missingMetadataCount} games have no genres, tags or artwork yet. This normally starts on its own — if it hasn't, use the button above.`
                        : 'Genres and artwork are up to date. New games are fetched automatically.'}
                </p>
                {metadataJob.error && <p className="error">{metadataJob.error}</p>}
              </div>

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

              <McpPanel />
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
                statusFilter={statusFilter}
                onStatusFilterChange={setStatusFilter}
                count={games.length}
                onExport={handleExportCsv}
              />
              <StatusStrip
                refreshing={refreshing}
                job={metadataJob}
                missingCount={missingMetadataCount}
                igdbConfigured={settings.igdbConfigured}
                onEnrich={handleEnrichNow}
                onDismissError={() => setMetadataJob({ ...metadataJob, error: null })}
              />
              {/* No `key` here on purpose: forcing a remount threw away and rebuilt
                  every card, which is the bulk of the cost when switching views. */}
              {viewMode === 'grid' ? (
                <GameGrid
                  games={games}
                  metadata={metadata}
                  installed={installed}
                  statuses={statuses}
                  onLaunch={handleLaunch}
                  onInstall={handleInstall}
                  onStatusChange={handleStatusChange}
                  onOpen={setDetailGame}
                />
              ) : (
                <GameList
                  games={games}
                  metadata={metadata}
                  installed={installed}
                  statuses={statuses}
                  onLaunch={handleLaunch}
                  onInstall={handleInstall}
                  onStatusChange={handleStatusChange}
                />
              )}
            </>
          )}
        </main>
      )}

      {detailGame && (
        <GameDetail
          game={detailGame}
          entry={metadata[slugify(detailGame.title)]}
          coverSources={coverSources(detailGame, metadata[slugify(detailGame.title)])}
          installed={installed[detailGame.id] ?? null}
          status={statuses[slugify(detailGame.title)]?.status ?? null}
          onClose={() => setDetailGame(null)}
          onLaunch={handleLaunch}
          onInstall={handleInstall}
          onStatusChange={handleStatusChange}
        />
      )}
    </div>
  );
}
