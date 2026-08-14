/**
 * Lets the UI run in a plain browser (`npm run dev --prefix client`) for layout work,
 * where the Tauri `invoke` bridge doesn't exist. Dev-only — `import.meta.env.DEV` is
 * statically false in production builds, so bundlers drop this entire module.
 */
import { slugify } from './slugify';

const PLATFORMS = ['steam', 'epic'];

// Subscribers to the simulated enrichment progress events.
const progressHandlers = new Set();

// Stands in for the SQLite metadata cache so enriched games stop counting as missing.
const cachedMetadata = {};

// Play status, keyed by slug — mirrors the game_status table.
const statuses = {};

const SAMPLE_GENRES = ['Adventure', 'Indie', 'Role-playing (RPG)', 'Strategy', 'Shooter'];
const SAMPLE_TAGS = ['Action', 'Fantasy', 'Science fiction', 'roguelike', 'Open world'];

// Sized to match a real library so performance work is measured against the real thing.
const SAMPLE_SIZE = Number(import.meta.env.VITE_FIXTURE_SIZE ?? 900);

const sampleGames = Array.from({ length: SAMPLE_SIZE }, (_, i) => ({
  id: `${PLATFORMS[i % 2]}-${1000 + i}`,
  platform: PLATFORMS[i % 2],
  title: `Sample Game ${String(i + 1).padStart(2, '0')}`,
  playtimeMinutes: i % 2 === 0 ? i * 37 : null,
  coverUrl: null,
}));

const installed = Object.fromEntries(
  sampleGames.slice(0, 7).map((g) => [
    g.id,
    {
      platform: g.platform,
      launchId: '1',
      title: g.title,
      installDir: null,
      namespace: null,
      catalogItemId: null,
    },
  ])
);

export const devApi = {
  getSettings: async () => ({ steamId: '76561190000000000', steamConfigured: true, igdbConfigured: true }),
  saveSettings: async () => ({ steamId: '', steamConfigured: true, igdbConfigured: true }),
  clearCredentials: async () => ({ steamId: '', steamConfigured: false, igdbConfigured: false }),
  getSteamLibrary: async () => sampleGames.filter((g) => g.platform === 'steam'),
  refreshSteamLibrary: async () => sampleGames.filter((g) => g.platform === 'steam'),
  getFamilyLibrary: async () =>
    Array.from({ length: 4 }, (_, i) => ({
      id: `steam-fam-${i}`,
      platform: 'steam',
      title: `Family Shared ${i + 1}`,
      playtimeMinutes: null,
      coverUrl: null,
    })),
  getEpicLibrary: async () => ({
    games: sampleGames.filter((g) => g.platform === 'epic'),
    importedAt: Date.now(),
  }),
  importEpicLibrary: async () => ({ games: [], importedAt: Date.now() }),
  getMetadata: async () => ({ ...cachedMetadata }),
  getEnrichmentJob: async () => ({ running: false, total: 0, completed: 0, error: null }),

  // Simulates a real background pass — progresses over time and fills the cache as it
  // goes, so the status strip behaves exactly as it does against the Rust backend.
  // Steam tags need no credentials and finish in seconds, so the fixture just no-ops.
  enrichSteamTags: async () => ({ running: false, total: 0, completed: 0, error: null }),
  onSteamProgress: async () => () => {},

  enrichMetadata: async () => {
    const titles = sampleGames.map((g) => g.title);
    const pending = titles.filter((t) => !cachedMetadata[slugify(t)]?.igdb);
    const total = pending.length;
    let completed = 0;

    const tick = () => {
      const step = Math.ceil(total / 12);
      for (const title of pending.slice(completed, completed + step)) {
        cachedMetadata[slugify(title)] = {
          matchedName: title,
          genres: [SAMPLE_GENRES[title.length % SAMPLE_GENRES.length]],
          tags: [SAMPLE_TAGS[title.length % SAMPLE_TAGS.length]],
          coverUrl: null,
          notFound: false,
          fetchedAt: Date.now(),
          // Mirrors the merged shape Rust returns; `igdb` is what drives the auto-enrich
          // check, so the fixture is wrong without it.
          igdb: true,
          steam: false,
          shortDescription: null,
          reviewPercent: null,
          reviewCount: null,
          releasedAt: null,
        };
      }
      completed = Math.min(total, completed + step);
      const running = completed < total;
      progressHandlers.forEach((h) => h({ running, total, completed, error: null }));
      if (running) setTimeout(tick, 600);
    };

    setTimeout(tick, 400);
    return { running: true, total, completed: 0, error: null };
  },
  getStatuses: async () => ({ ...statuses }),
  setGameStatus: async (slug, status) => {
    if (status) {
      statuses[slug] = {
        status,
        updatedAt: Date.now(),
        completedAt: status === 'completed' ? (statuses[slug]?.completedAt ?? Date.now()) : null,
      };
    } else {
      delete statuses[slug];
    }
    return { ...statuses };
  },

  getInstalled: async () => installed,
  launchGame: async () => {},
  installGame: async () => {},
  openExternal: async (url) => window.open(url, '_blank'),
  bookmarkletPort: async () => 43117,
  getMcpInfo: async () => ({
    available: true,
    path: 'C:\\Users\\you\\AppData\\Local\\UGLy\\ugly-mcp.exe',
    config: JSON.stringify(
      {
        mcpServers: {
          ugly: { command: 'C:\\Users\\you\\AppData\\Local\\UGLy\\ugly-mcp.exe', args: [] },
        },
      },
      null,
      2
    ),
  }),
  onEpicImported: async () => () => {},
  onFamilyImported: async () => () => {},
  onEnrichmentProgress: async (handler) => {
    progressHandlers.add(handler);
    return () => progressHandlers.delete(handler);
  },
};
