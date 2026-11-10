-- Yoru schema v1. Secrets are NEVER stored here: passwords and key
-- passphrases live in Windows Credential Manager (see credentials module).

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE servers (
    id                TEXT PRIMARY KEY,
    name              TEXT NOT NULL,
    host              TEXT NOT NULL,
    port              INTEGER NOT NULL DEFAULT 22 CHECK (port BETWEEN 1 AND 65535),
    username          TEXT NOT NULL,
    auth_method       TEXT NOT NULL CHECK (auth_method IN ('password', 'key', 'agent')),
    key_path          TEXT,
    profile_kind      TEXT NOT NULL DEFAULT 'linux',
    group_name        TEXT,
    tags              TEXT NOT NULL DEFAULT '[]',
    notes             TEXT NOT NULL DEFAULT '',
    favorite          INTEGER NOT NULL DEFAULT 0,
    auto_reconnect    INTEGER NOT NULL DEFAULT 0,
    keepalive         INTEGER NOT NULL DEFAULT 1,
    jump_host_id      TEXT REFERENCES servers(id) ON DELETE SET NULL,
    os_detected       TEXT,
    last_connected_at TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);

CREATE INDEX idx_servers_name ON servers(name COLLATE NOCASE);

-- Trusted SSH host keys. One host:port may have several trusted keys
-- (different algorithms); any presented key not in this set for a
-- host:port that already has entries is treated as a CHANGED key.
CREATE TABLE known_hosts (
    host        TEXT NOT NULL,
    port        INTEGER NOT NULL,
    algorithm   TEXT NOT NULL,
    public_key  TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    added_at    TEXT NOT NULL,
    PRIMARY KEY (host, port, public_key)
);

CREATE TABLE activity (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    ts          TEXT NOT NULL,
    server_id   TEXT,
    server_name TEXT,
    kind        TEXT NOT NULL,
    message     TEXT NOT NULL
);

CREATE INDEX idx_activity_ts ON activity(ts DESC);
