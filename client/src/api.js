import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { devApi } from './devFixture.js';

const tauriApi = {
  // Returns only whether credentials exist — the Steam key and IGDB secret stay in Rust
  // (OS keychain) and are never sent to this webview.
  getSettings: () => invoke('get_settings'),

  // Blank values mean "leave unchanged", so the form can show empty secret fields.
  saveSettings: ({ steamApiKey = '', steamId = '', igdbClientId = '', igdbClientSecret = '' }) =>
    invoke('save_settings', {
      steamApiKey,
      steamId,
      igdbClientId,
      igdbClientSecret,
    }),
  clearCredentials: () => invoke('clear_credentials'),

  getSteamLibrary: () => invoke('get_steam_library'),
  refreshSteamLibrary: () => invoke('refresh_steam_library'),

  getFamilyLibrary: () => invoke('get_family_library'),
  getEpicLibrary: () => invoke('get_epic_library'),
  importEpicLibrary: (data) => invoke('import_epic_library', { data }),

  getMetadata: () => invoke('get_metadata'),
  getEnrichmentJob: () => invoke('get_enrichment_job'),
  enrichMetadata: (titles) => invoke('enrich_metadata', { titles }),

  getStatuses: () => invoke('get_statuses'),
  // `status` of null clears it, returning the game to the implicit backlog.
  setGameStatus: (slug, status) => invoke('set_game_status', { slug, status }),

  getInstalled: () => invoke('get_installed'),
  launchGame: (gameId) => invoke('launch_game', { gameId }),
  installGame: (game) =>
    invoke('install_game', { gameId: game.id, platform: game.platform, title: game.title }),
  // External links go through Rust, which only permits https — the webview itself has no
  // opener permission, so it cannot open arbitrary targets.
  openExternal: (url) => invoke('open_external', { url }),

  bookmarkletPort: () => invoke('bookmarklet_port'),

  // Pushed from Rust: the bookmarklet landing in the background, and lookup progress.
  onEpicImported: (handler) => listen('epic-imported', (event) => handler(event.payload)),
  onFamilyImported: (handler) => listen('family-imported', (event) => handler(event.payload)),
  onEnrichmentProgress: (handler) => listen('enrichment-progress', (event) => handler(event.payload)),
};

// Sample data for doing UI work in a plain browser, where there is no invoke bridge.
// Opt-in ONLY, via `VITE_UI_FIXTURE=1 npm run dev --prefix client`.
//
// Deliberately NOT auto-detected from "is Tauri missing?": that check can be false at
// module-evaluation time inside the real app, which would silently replace the user's
// library with fake games. An explicit flag cannot do that.
export const api =
  import.meta.env.DEV && import.meta.env.VITE_UI_FIXTURE === '1' ? devApi : tauriApi;
