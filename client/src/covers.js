const STEAM_CDN = 'https://cdn.cloudflare.steamstatic.com/steam/apps';

/**
 * Ordered cover-art candidates for a game, best first.
 *
 * Steam's own portrait art is preferred where it exists — it's the artwork the store
 * and library use, so it's the most recognisable. IGDB's cover is portrait too and
 * covers Epic games. The stored `coverUrl` is the landscape header, kept last as a
 * fallback for older Steam titles that never got portrait art.
 */
export function coverSources(game, metadataEntry) {
  const steamAppId = game.id.startsWith('steam-') ? game.id.slice('steam-'.length) : null;

  return [
    steamAppId ? `${STEAM_CDN}/${steamAppId}/library_600x900.jpg` : null,
    metadataEntry?.coverUrl ?? null,
    game.coverUrl ?? null,
  ];
}
