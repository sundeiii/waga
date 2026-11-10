-- Saved local port forwards (ssh -L) per server profile.
CREATE TABLE port_forwards (
    id          TEXT PRIMARY KEY,
    server_id   TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    label       TEXT NOT NULL DEFAULT '',
    local_port  INTEGER NOT NULL CHECK (local_port BETWEEN 0 AND 65535),
    remote_host TEXT NOT NULL,
    remote_port INTEGER NOT NULL CHECK (remote_port BETWEEN 1 AND 65535),
    auto_start  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

CREATE INDEX idx_port_forwards_server ON port_forwards(server_id);

-- Trusted SSH certificate authorities for host certificates (@cert-authority).
CREATE TABLE host_cas (
    id          TEXT PRIMARY KEY,
    pattern     TEXT NOT NULL,
    public_key  TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    comment     TEXT NOT NULL DEFAULT '',
    added_at    TEXT NOT NULL
);
