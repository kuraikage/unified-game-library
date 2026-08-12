// Punctuation in IGDB labels is inconsistent ("Point-and-click", "Turn-based strategy (TBS)",
// "Hack and slash/Beat 'em up"), so both the query and the searched text get flattened to plain
// lowercase words before comparing.
function normalize(text) {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, ' ')
    .trim();
}

// Separator-free form, so "coop" matches "co-op" and "scifi" matches "sci-fi".
function condense(text) {
  return text.toLowerCase().replace(/[^a-z0-9]+/g, '');
}

// Everyday names for genres/tags mapped to the wording IGDB actually uses. Keys are normalized
// queries; values are extra phrases to try. Only maps to labels that exist in IGDB's vocabulary —
// an unmatched alias simply finds nothing rather than misleading you.
const SYNONYMS = {
  'sci fi': ['science fiction'],
  scifi: ['science fiction'],
  'space opera': ['science fiction', 'space'],
  fps: ['shooter'],
  'first person shooter': ['shooter'],
  'third person shooter': ['shooter'],
  tps: ['shooter'],
  shooters: ['shooter'],
  rts: ['real time strategy'],
  tbs: ['turn based strategy'],
  'turn based': ['turn based strategy', 'turn based'],
  jrpg: ['role playing'],
  arpg: ['role playing'],
  crpg: ['role playing'],
  'action rpg': ['role playing'],
  rpgs: ['role playing'],
  'deck builder': ['card board game'],
  deckbuilder: ['card board game'],
  deckbuilding: ['card board game'],
  'deck building': ['card board game'],
  'card game': ['card board game'],
  'board game': ['card board game'],
  cards: ['card board game'],
  'beat em up': ['hack and slash beat em up'],
  brawler: ['hack and slash beat em up'],
  'hack n slash': ['hack and slash beat em up'],
  'hack and slash': ['hack and slash beat em up'],
  soulslike: ['souls like'],
  soulsborne: ['souls like'],
  metroidvania: ['metroidvania'],
  roguelite: ['roguelike', 'roguelite'],
  'rogue like': ['roguelike'],
  'rogue lite': ['roguelite', 'roguelike'],
  platformer: ['platform'],
  platformers: ['platform'],
  puzzler: ['puzzle'],
  puzzles: ['puzzle'],
  sim: ['simulator'],
  sims: ['simulator'],
  simulation: ['simulator'],
  sports: ['sport'],
  driving: ['racing'],
  vn: ['visual novel'],
  scary: ['horror'],
  spooky: ['horror'],
  funny: ['comedy'],
  humour: ['comedy'],
  humor: ['comedy'],
  war: ['warfare'],
  military: ['warfare', 'modern warfare'],
  mmo: ['massively multiplayer'],
  mmorpg: ['massively multiplayer', 'mmorpg'],
  'open ended': ['sandbox', 'open world'],
  openworld: ['open world'],
  'story driven': ['story driven', 'story rich'],
  narrative: ['story driven', 'story rich', 'drama'],
  relaxing: ['casual'],
  chill: ['casual'],
  cozy: ['casual'],
  'point n click': ['point and click'],
  'point click': ['point and click'],
  'strategy games': ['strategy'],
  detective: ['detective', 'investigation', 'mystery'],
  zombie: ['zombies'],
};

export function buildHaystack(game, entry) {
  const parts = [game.title, ...(entry?.genres ?? []), ...(entry?.tags ?? [])];
  const joined = parts.join(' ');
  return { normalized: normalize(joined), condensed: condense(joined) };
}

function matchesPhrase(haystack, phrase) {
  const normalizedPhrase = normalize(phrase);
  if (!normalizedPhrase) return false;

  // Exact phrase, ignoring punctuation: "point-and-click" and "point and click" both land here.
  if (haystack.normalized.includes(normalizedPhrase)) return true;

  // Separator-free: "coop" finds "co-op", "scifi" finds "sci-fi".
  if (haystack.condensed.includes(condense(phrase))) return true;

  // Loose word match, so word order and filler words don't matter ("point click").
  // Tokens match at word starts only — and very short ones must match a whole word, otherwise
  // "co" and "op" from "co-op" would hit "combat", "open" and most of the library.
  const words = haystack.normalized.split(' ');
  return normalizedPhrase
    .split(' ')
    .every((token) =>
      token.length >= 3
        ? words.some((word) => word.startsWith(token))
        : words.some((word) => word === token)
    );
}

export function matchesSearch(haystack, term) {
  const normalizedTerm = normalize(term);
  if (!normalizedTerm) return true;

  if (matchesPhrase(haystack, term)) return true;

  const aliases = SYNONYMS[normalizedTerm] ?? SYNONYMS[condense(term)];
  return Boolean(aliases?.some((alias) => matchesPhrase(haystack, alias)));
}
