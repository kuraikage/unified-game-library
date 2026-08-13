import { CompletedIcon, DroppedIcon, PlayingIcon } from './icons';

/**
 * The three explicit states. "Backlog" is deliberately absent — it's the default,
 * represented by having no status at all, so untouched games store nothing.
 */
export const STATUSES = [
  { value: 'playing', label: 'Playing', short: 'playing', Icon: PlayingIcon },
  { value: 'completed', label: 'Completed', short: 'completed', Icon: CompletedIcon },
  { value: 'dropped', label: 'Dropped', short: 'dropped', Icon: DroppedIcon },
];

export const statusMeta = (value) => STATUSES.find((s) => s.value === value) ?? null;
