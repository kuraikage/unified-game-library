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

const SAMPLE_GENRES = ['Adventure', 'Indie', 'Role-playing (RPG)', 'Strategy', 'Shooter'];
const SAMPLE_TAGS = ['Action', 'Fantasy', 'Science fiction', 'roguelike', 'Open world'];

const sampleGames = Array.from({ length: 60 }, (_, i) => ({
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
  enrichMetadata: async (titles) => {
    const pending = titles.filter((t) => !cachedMetadata[slugify(t)]);
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
  getInstalled: async () => installed,
  launchGame: async () => {},
  installGame: async () => {},
  openExternal: async (url) => window.open(url, '_blank'),
  bookmarkletPort: async () => 43117,
  onEpicImported: async () => () => {},
  onFamilyImported: async () => () => {},
  onEnrichmentProgress: async (handler) => {
    progressHandlers.add(handler);
    return () => progressHandlers.delete(handler);
  },
};
