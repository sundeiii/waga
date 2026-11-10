-- Sync support: change timestamps on every syncable row and tombstones for deletions.
ALTER TABLE port_forwards ADD COLUMN updated_at TEXT;
UPDATE port_forwards SET updated_at = created_at;

CREATE TABLE sync_tombstones (
    kind       TEXT NOT NULL,   -- server | snippet | forward | host_ca | known_host
    id         TEXT NOT NULL,   -- known_host: "host:port"
    deleted_at TEXT NOT NULL,
    PRIMARY KEY (kind, id)
);

-- Per-installation identity (not synced).
CREATE TABLE device (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
