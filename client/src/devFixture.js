/**
 * Lets the UI run in a plain browser (`npm run dev --prefix client`) for layout work,
 * where the Tauri `invoke` bridge doesn't exist. Dev-only — `import.meta.env.DEV` is
 * statically false in production builds, so bundlers drop this entire module.
 */
const PLATFORMS = ['steam', 'epic'];

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
  getMetadata: async () => ({}),
  getEnrichmentJob: async () => ({ running: false, total: 0, completed: 0, error: null }),
  enrichMetadata: async () => ({ running: false, total: 0, completed: 0, error: null }),
  getInstalled: async () => installed,
  launchGame: async () => {},
  installGame: async () => {},
  openExternal: async (url) => window.open(url, '_blank'),
  bookmarkletPort: async () => 43117,
  onEpicImported: async () => () => {},
  onFamilyImported: async () => () => {},
  onEnrichmentProgress: async () => () => {},
};
