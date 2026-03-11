# Implementing ADR-0025: Go Admin Dashboard

Date: 2026-03-11
Kind: Guide
Status: Active
Priority: 1
Requires: [ADR-0025]

## Narrative Summary (1-minute read)

Build a Go admin dashboard that reads the hypervisor's SQLite DB and Linux
system stats, serves a server-rendered HTML page. Five files, one dependency
(go-sqlite3), deployable in an afternoon.

## What To Do Next

1. Scaffold Go module.
2. Implement queries, system stats, handler, template.
3. Test locally against a copy of the hypervisor DB.
4. Add nix build and deploy to Node B.

---

## Step 1: Scaffold

```bash
mkdir -p admin-dashboard/templates
cd admin-dashboard
go mod init github.com/nicholasgasior/choiros/admin-dashboard
go get github.com/mattn/go-sqlite3
```

## Step 2: main.go

```go
package main

import (
    "flag"
    "log"
    "net/http"
)

func main() {
    dbPath := flag.String("db", "./data/hypervisor.db", "path to hypervisor SQLite DB")
    listen := flag.String("listen", "127.0.0.1:9091", "listen address")
    flag.Parse()

    db, err := openDB(*dbPath)
    if err != nil {
        log.Fatalf("open db: %v", err)
    }
    defer db.Close()

    srv := &Server{db: db}
    http.HandleFunc("/admin/", srv.handleDashboard)
    http.HandleFunc("/admin/api/stats", srv.handleStatsJSON)

    log.Printf("admin dashboard listening on %s", *listen)
    log.Fatal(http.ListenAndServe(*listen, nil))
}
```

## Step 3: queries.go

SQLite queries against the existing schema. All read-only.

```go
package main

import (
    "database/sql"
    _ "github.com/mattn/go-sqlite3"
)

func openDB(path string) (*sql.DB, error) {
    return sql.Open("sqlite3", path+"?mode=ro&_journal_mode=wal")
}

type DashboardStats struct {
    // Users
    TotalUsers       int
    RecentRegistrations int // last 7d

    // Sessions
    ActiveSessions24h int
    RecentLogins     []AuditEntry

    // VMs
    RunningVMs   int
    StoppedVMs   int
    FailedVMs    int

    // Runtimes
    ActiveRuntimes int
    PortMap        []PortEntry

    // System (filled separately)
    System SystemStats
}

type AuditEntry struct {
    UserID    string
    Username  string
    Event     string
    IP        string
    CreatedAt int64
}

type PortEntry struct {
    UserID     string
    BranchName string
    Port       int
    State      string
}

func queryStats(db *sql.DB) (*DashboardStats, error) {
    s := &DashboardStats{}

    // Total users
    db.QueryRow("SELECT COUNT(*) FROM users").Scan(&s.TotalUsers)

    // Recent registrations (last 7 days)
    db.QueryRow(`SELECT COUNT(*) FROM users
        WHERE created_at > unixepoch() - 604800`).Scan(&s.RecentRegistrations)

    // Active sessions (logins in last 24h)
    db.QueryRow(`SELECT COUNT(*) FROM audit_log
        WHERE event = 'login' AND created_at > unixepoch() - 86400`).Scan(&s.ActiveSessions24h)

    // Recent logins (last 20)
    rows, err := db.Query(`
        SELECT a.user_id, COALESCE(u.username, '?'), a.event,
               COALESCE(a.ip, ''), a.created_at
        FROM audit_log a
        LEFT JOIN users u ON u.id = a.user_id
        WHERE a.event = 'login'
        ORDER BY a.created_at DESC LIMIT 20`)
    if err == nil {
        defer rows.Close()
        for rows.Next() {
            var e AuditEntry
            rows.Scan(&e.UserID, &e.Username, &e.Event, &e.IP, &e.CreatedAt)
            s.RecentLogins = append(s.RecentLogins, e)
        }
    }

    // VM states
    db.QueryRow(`SELECT COUNT(*) FROM user_vms WHERE state = 'running'`).Scan(&s.RunningVMs)
    db.QueryRow(`SELECT COUNT(*) FROM user_vms WHERE state = 'stopped'`).Scan(&s.StoppedVMs)
    db.QueryRow(`SELECT COUNT(*) FROM user_vms WHERE state = 'failed'`).Scan(&s.FailedVMs)

    // Active runtimes
    db.QueryRow(`SELECT COUNT(*) FROM branch_runtimes WHERE state = 'running'`).Scan(&s.ActiveRuntimes)

    // Port map
    rows, err = db.Query(`
        SELECT user_id, branch_name, port, state
        FROM branch_runtimes
        WHERE state = 'running'
        ORDER BY port`)
    if err == nil {
        defer rows.Close()
        for rows.Next() {
            var p PortEntry
            rows.Scan(&p.UserID, &p.BranchName, &p.Port, &p.State)
            s.PortMap = append(s.PortMap, p)
        }
    }

    return s, nil
}
```

## Step 4: system.go

```go
package main

import (
    "bufio"
    "fmt"
    "os"
    "strconv"
    "strings"
    "syscall"
)

type SystemStats struct {
    MemTotalMB  uint64
    MemAvailMB  uint64
    MemUsedPct  float64
    LoadAvg1    float64
    LoadAvg5    float64
    LoadAvg15   float64
    DiskTotalGB float64
    DiskFreeGB  float64
    DiskUsedPct float64
}

func readSystemStats(dataPath string) SystemStats {
    s := SystemStats{}

    // Memory from /proc/meminfo
    if f, err := os.Open("/proc/meminfo"); err == nil {
        defer f.Close()
        scanner := bufio.NewScanner(f)
        for scanner.Scan() {
            line := scanner.Text()
            if strings.HasPrefix(line, "MemTotal:") {
                s.MemTotalMB = parseMemKB(line) / 1024
            } else if strings.HasPrefix(line, "MemAvailable:") {
                s.MemAvailMB = parseMemKB(line) / 1024
            }
        }
        if s.MemTotalMB > 0 {
            s.MemUsedPct = float64(s.MemTotalMB-s.MemAvailMB) / float64(s.MemTotalMB) * 100
        }
    }

    // Load average from /proc/loadavg
    if data, err := os.ReadFile("/proc/loadavg"); err == nil {
        parts := strings.Fields(string(data))
        if len(parts) >= 3 {
            s.LoadAvg1, _ = strconv.ParseFloat(parts[0], 64)
            s.LoadAvg5, _ = strconv.ParseFloat(parts[1], 64)
            s.LoadAvg15, _ = strconv.ParseFloat(parts[2], 64)
        }
    }

    // Disk from statfs
    var stat syscall.Statfs_t
    if err := syscall.Statfs(dataPath, &stat); err == nil {
        s.DiskTotalGB = float64(stat.Blocks*uint64(stat.Bsize)) / 1e9
        s.DiskFreeGB = float64(stat.Bavail*uint64(stat.Bsize)) / 1e9
        s.DiskUsedPct = (1 - s.DiskFreeGB/s.DiskTotalGB) * 100
    }

    return s
}

func parseMemKB(line string) uint64 {
    fields := strings.Fields(line)
    if len(fields) >= 2 {
        v, _ := strconv.ParseUint(fields[1], 10, 64)
        return v
    }
    return 0
}
```

## Step 5: handlers.go

```go
package main

import (
    "database/sql"
    "encoding/json"
    "html/template"
    "net/http"
)

type Server struct {
    db       *sql.DB
    dataPath string
}

var tmpl = template.Must(template.ParseFiles("templates/dashboard.html"))

func (s *Server) handleDashboard(w http.ResponseWriter, r *http.Request) {
    stats, err := queryStats(s.db)
    if err != nil {
        http.Error(w, err.Error(), 500)
        return
    }
    stats.System = readSystemStats(s.dataPath)
    w.Header().Set("Content-Type", "text/html; charset=utf-8")
    tmpl.Execute(w, stats)
}

func (s *Server) handleStatsJSON(w http.ResponseWriter, r *http.Request) {
    stats, err := queryStats(s.db)
    if err != nil {
        http.Error(w, err.Error(), 500)
        return
    }
    stats.System = readSystemStats(s.dataPath)
    w.Header().Set("Content-Type", "application/json")
    json.NewEncoder(w).Encode(stats)
}
```

## Step 6: templates/dashboard.html

Minimal, functional. No CSS framework.

```html
<!DOCTYPE html>
<html>
<head>
  <title>ChoirOS Admin</title>
  <meta http-equiv="refresh" content="30">
  <style>
    body { font-family: monospace; max-width: 900px; margin: 2em auto; }
    table { border-collapse: collapse; width: 100%; margin: 1em 0; }
    th, td { border: 1px solid #ccc; padding: 4px 8px; text-align: left; }
    th { background: #f5f5f5; }
    .metric { display: inline-block; margin: 0.5em 1em; }
    .metric .value { font-size: 2em; font-weight: bold; }
    .metric .label { color: #666; }
    h2 { margin-top: 2em; border-bottom: 1px solid #ccc; }
  </style>
</head>
<body>
  <h1>ChoirOS Admin Dashboard</h1>

  <h2>System</h2>
  <div class="metric">
    <div class="value">{{printf "%.0f" .System.MemUsedPct}}%</div>
    <div class="label">Memory ({{.System.MemAvailMB}} / {{.System.MemTotalMB}} MB avail)</div>
  </div>
  <div class="metric">
    <div class="value">{{printf "%.1f" .System.LoadAvg1}}</div>
    <div class="label">Load (1m / 5m: {{printf "%.1f" .System.LoadAvg5}} / 15m: {{printf "%.1f" .System.LoadAvg15}})</div>
  </div>
  <div class="metric">
    <div class="value">{{printf "%.0f" .System.DiskUsedPct}}%</div>
    <div class="label">Disk ({{printf "%.0f" .System.DiskFreeGB}} / {{printf "%.0f" .System.DiskTotalGB}} GB free)</div>
  </div>

  <h2>Users</h2>
  <div class="metric">
    <div class="value">{{.TotalUsers}}</div>
    <div class="label">Total users</div>
  </div>
  <div class="metric">
    <div class="value">{{.RecentRegistrations}}</div>
    <div class="label">Registered (7d)</div>
  </div>

  <h2>VMs</h2>
  <div class="metric">
    <div class="value">{{.RunningVMs}}</div>
    <div class="label">Running</div>
  </div>
  <div class="metric">
    <div class="value">{{.StoppedVMs}}</div>
    <div class="label">Stopped</div>
  </div>
  <div class="metric">
    <div class="value">{{.FailedVMs}}</div>
    <div class="label">Failed</div>
  </div>

  <h2>Sessions (24h)</h2>
  <div class="metric">
    <div class="value">{{.ActiveSessions24h}}</div>
    <div class="label">Logins</div>
  </div>

  {{if .RecentLogins}}
  <h2>Recent Logins</h2>
  <table>
    <tr><th>User</th><th>IP</th><th>Time</th></tr>
    {{range .RecentLogins}}
    <tr><td>{{.Username}}</td><td>{{.IP}}</td><td>{{.CreatedAt}}</td></tr>
    {{end}}
  </table>
  {{end}}

  {{if .PortMap}}
  <h2>Active Runtimes</h2>
  <table>
    <tr><th>Port</th><th>Branch</th><th>User</th><th>State</th></tr>
    {{range .PortMap}}
    <tr><td>{{.Port}}</td><td>{{.BranchName}}</td><td>{{.UserID}}</td><td>{{.State}}</td></tr>
    {{end}}
  </table>
  {{end}}
</body>
</html>
```

## Step 7: Nix Build

Add to `flake.nix`:

```nix
admin-dashboard = pkgs.buildGoModule {
  pname = "choir-admin";
  version = "0.1.0";
  src = ./admin-dashboard;
  vendorHash = "sha256-PLACEHOLDER";
  CGO_ENABLED = 1;  # required for go-sqlite3
};
```

## Step 8: Systemd Unit

Add to `nix/hosts/ovh-node.nix`:

```nix
systemd.services.choir-admin = {
  description = "ChoirOS Admin Dashboard";
  after = [ "hypervisor.service" ];
  wantedBy = [ "multi-user.target" ];
  serviceConfig = {
    ExecStart = "${pkgs.choir-admin}/bin/choir-admin --db /opt/choiros/data/hypervisor.db --data-path /data --listen 127.0.0.1:9091";
    Restart = "on-failure";
    ReadOnlyPaths = [ "/opt/choiros/data/hypervisor.db" "/data" "/proc" ];
    DynamicUser = true;
  };
};
```

## Step 9: Caddy Route

```
handle /admin/* {
    reverse_proxy 127.0.0.1:9091
}
```

## Step 10: Deploy and Verify

```bash
# Build locally to verify
cd admin-dashboard && go build -o choir-admin . && ./choir-admin --db ../data/hypervisor.db

# Push, CI deploys to Node B
git push origin main

# Verify
curl -s http://localhost:9091/admin/api/stats | jq .
```

## Testing

Local testing against a copy of the hypervisor DB:

```bash
# Copy DB from server
scp node-b:/opt/choiros/data/hypervisor.db ./test-hypervisor.db

# Run dashboard locally (system stats will show macOS values or fail
# gracefully — /proc doesn't exist on macOS)
cd admin-dashboard
go run . --db ../test-hypervisor.db --listen 127.0.0.1:9091
open http://localhost:9091/admin/
```

## What NOT to Do

- Don't add a JS framework or build step
- Don't write to the hypervisor DB
- Don't add authentication beyond IP allowlist in Phase 1
- Don't add historical time-series storage yet
- Don't over-design the UI — functional monospace is fine
