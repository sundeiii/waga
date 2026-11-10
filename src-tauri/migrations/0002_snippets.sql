-- Command snippets. Contents are user data; they are never written to the activity log.

CREATE TABLE snippets (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    command      TEXT NOT NULL,
    category     TEXT NOT NULL DEFAULT 'custom',
    tags         TEXT NOT NULL DEFAULT '[]',
    favorite     INTEGER NOT NULL DEFAULT 0,
    use_count    INTEGER NOT NULL DEFAULT 0,
    last_used_at TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE INDEX idx_snippets_category ON snippets(category);

-- Starter set: read-only diagnostics only (nothing here changes a system).
INSERT INTO snippets (id, title, description, command, category, tags, created_at, updated_at) VALUES
('seed-linux-1', 'Disk usage by filesystem', 'Human-readable usage of mounted filesystems', 'df -h', 'linux', '["disk"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-linux-2', 'Largest directories here', 'Top 15 biggest entries in the current directory', 'du -sh -- * .[!.]* 2>/dev/null | sort -rh | head -n 15', 'linux', '["disk"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-linux-3', 'Failed systemd units', 'Services that failed to start', 'systemctl --failed', 'linux', '["systemd"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-linux-4', 'Follow a service log', 'Replace nginx with the unit you need', 'journalctl -u nginx -f', 'linux', '["logs","systemd"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-net-1', 'Listening ports', 'TCP/UDP sockets and owning processes', 'ss -tulpn', 'networking', '["ports"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-net-2', 'IP addresses', 'Brief view of interfaces and addresses', 'ip -br addr', 'networking', '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-net-3', 'DNS lookup', 'Short answer for a hostname', 'dig +short example.com', 'networking', '["dns"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-docker-1', 'Running containers', 'Name, image, status and ports', 'docker ps --format "table {{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}"', 'docker', '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-docker-2', 'Container resource usage', 'One-shot CPU/memory per container', 'docker stats --no-stream', 'docker', '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-git-1', 'Compact history', 'Last 20 commits as a graph', 'git log --oneline --graph --decorate -n 20', 'git', '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-git-2', 'Status, short', 'Branch and changed files', 'git status -sb', 'git', '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-diag-1', 'Top processes by CPU', 'Ten busiest processes', 'ps aux --sort=-%cpu | head -n 11', 'diagnostics', '["cpu"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-diag-2', 'Memory overview', 'Free/used memory in human units', 'free -h', 'diagnostics', '["memory"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-diag-3', 'Uptime and load', 'How long the system has been up', 'uptime', 'diagnostics', '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-ps-1', 'Top processes (PowerShell)', 'Ten processes using the most CPU time', 'Get-Process | Sort-Object CPU -Descending | Select-Object -First 10 Name, Id, CPU, WS', 'powershell', '["cpu"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-ps-2', 'Listening TCP ports (PowerShell)', 'Local listeners with owning process id', 'Get-NetTCPConnection -State Listen | Sort-Object LocalPort | Select-Object LocalAddress, LocalPort, OwningProcess', 'powershell', '["ports"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-ps-3', 'Free disk space (PowerShell)', 'Size and free space per drive', 'Get-PSDrive -PSProvider FileSystem | Select-Object Name, @{n="UsedGB";e={[math]::Round($_.Used/1GB,1)}}, @{n="FreeGB";e={[math]::Round($_.Free/1GB,1)}}', 'powershell', '["disk"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-sec-1', 'Recent logins', 'Last 15 logins on this host', 'last -n 15', 'security', '["auth"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
('seed-sec-2', 'Failed SSH logins', 'Recent authentication failures (systemd journal)', 'journalctl -u ssh -u sshd --since "24 hours ago" | grep -i "failed"', 'security', '["auth","ssh"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
