import SegmentedControl from './SegmentedControl';
import { DownloadIcon, DriveIcon, GridIcon, ListIcon, SearchIcon } from '../icons';
import { STATUSES } from '../gameStatus';

export default function FilterBar({
  search,
  onSearchChange,
  platform,
  onPlatformChange,
  viewMode,
  onViewModeChange,
  installedOnly,
  onInstalledOnlyChange,
  installedCount,
  statusFilter,
  onStatusFilterChange,
  count,
  onExport,
}) {
  return (
    <div className="filter-bar">
      {/* Search gets its own row; the filter capsules sit beneath it so neither has to
          fight for width once there are four of them plus the count and export. */}
      <div className="filter-row search-row">
        <div className="search-wrap">
        <span className="search-icon">
          <SearchIcon />
        </span>
        <input
          type="text"
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder="Search by title, genre or tag..."
          className="search"
        />
          {search && (
            <button type="button" className="search-clear" onClick={() => onSearchChange('')} aria-label="Clear search">
              ×
            </button>
          )}
        </div>
        <span className="count">{count} games</span>
        <button
          type="button"
          className="pill-button"
          onClick={onExport}
          disabled={count === 0}
          title="Export the games currently shown to CSV"
        >
          <DownloadIcon />
          <span>Export</span>
        </button>
      </div>

      <div className="filter-row">
      <SegmentedControl
        ariaLabel="Filter by platform"
        value={platform}
        onChange={onPlatformChange}
        options={[
          { value: 'all', label: 'All' },
          { value: 'steam', label: 'Steam' },
          { value: 'epic', label: 'Epic' },
        ]}
      />

      <SegmentedControl
        ariaLabel="View mode"
        value={viewMode}
        onChange={onViewModeChange}
        options={[
          { value: 'grid', label: 'Grid', icon: <GridIcon /> },
          { value: 'list', label: 'List', icon: <ListIcon /> },
        ]}
      />

      <SegmentedControl
        ariaLabel="Installed filter"
        value={installedOnly ? 'installed' : 'all'}
        onChange={(v) => onInstalledOnlyChange(v === 'installed')}
        options={[
          { value: 'all', label: 'All games' },
          {
            value: 'installed',
            label: `Installed${installedCount ? ` (${installedCount})` : ''}`,
            icon: <DriveIcon />,
          },
        ]}
      />

      <SegmentedControl
        ariaLabel="Filter by play status"
        value={statusFilter}
        onChange={onStatusFilterChange}
        options={[
          { value: 'all', label: 'Any' },
          { value: 'backlog', label: 'Backlog' },
          ...STATUSES.map((s) => ({ value: s.value, label: s.label, icon: <s.Icon /> })),
        ]}
      />

      </div>
    </div>
  );
}
