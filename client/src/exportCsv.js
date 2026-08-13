import { slugify } from './slugify';

// RFC 4180: wrap in quotes and double any embedded quotes. Game titles routinely contain
// commas, quotes and colons, so every field goes through this.
function escapeField(value) {
  const text = value === null || value === undefined ? '' : String(value);
  return `"${text.replace(/"/g, '""')}"`;
}

function formatPlaytimeHours(minutes) {
  if (minutes === null || minutes === undefined) return '';
  return (minutes / 60).toFixed(1);
}

export function buildLibraryCsv(games, metadata, statuses = {}, installed = {}) {
  const headers = [
    'Title',
    'Platform',
    'Status',
    'Installed',
    'Playtime (hours)',
    'Genres',
    'Tags',
    'Cover URL',
  ];
  const rows = games.map((game) => {
    const slug = slugify(game.title);
    const entry = metadata[slug];
    return [
      game.title,
      game.platform,
      // "backlog" is the absence of a status; spell it out so the column is never blank.
      statuses[slug]?.status ?? 'backlog',
      installed[game.id] ? 'yes' : 'no',
      formatPlaytimeHours(game.playtimeMinutes),
      (entry?.genres ?? []).join('; '),
      (entry?.tags ?? []).join('; '),
      game.coverUrl ?? entry?.coverUrl ?? '',
    ]
      .map(escapeField)
      .join(',');
  });

  return [headers.map(escapeField).join(','), ...rows].join('\r\n');
}

export function downloadCsv(filename, csv) {
  // The BOM makes Excel open UTF-8 correctly — game titles are full of ™, ®, and accents.
  const blob = new Blob(['﻿', csv], { type: 'text/csv;charset=utf-8;' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}
