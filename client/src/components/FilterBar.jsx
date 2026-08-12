import SegmentedControl from './SegmentedControl';
import { DownloadIcon, DriveIcon, GridIcon, ListIcon, SearchIcon } from '../icons';

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
  count,
  onExport,
}) {
  return (
    <div className="filter-bar">
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
  );
}
