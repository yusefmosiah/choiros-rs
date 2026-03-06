# EC2 Deployment Patterns for Containerized Rust Apps

**Date:** 2026-02-01
**Purpose:** Research deployment strategies for ChoirOS on EC2 with focus on systemd vs containers, blue-green deployments, health checks, log aggregation, cost optimization, and security hardening

---

## Executive Summary

This document analyzes deployment patterns for Rust applications on EC2, comparing systemd-native and containerized approaches. Key findings:

- **Current State:** ChoirOS uses systemd + Caddy on Ubuntu 22.04 (single EC2 instance)
- **Recommended Path:** Hybrid approach - systemd for reliability, containers for future scaling
- **Cost Optimization:** t3.small for dev ($0.02/hr), c5.large for prod ($0.102/hr)
- **Log Aggregation:** Loki + Grafana (cost-effective) vs CloudWatch Logs ($0.50/GB ingestion)
- **Security Hardening:** VPC isolation, least-privilege IAM, security group whitelisting

---

## 1. systemd vs Containers on EC2

### 1.1 systemd-Native Deployment

**Pattern:** Deploy compiled Rust binaries as systemd services

#### Architecture
```
┌─────────────────────────────────────────────────────────────┐
│                        EC2 Instance                          │
│  ┌───────────────────────────────────────────────────────┐ │
│  │  systemd                                               │ │
│  │  ├── choiros-backend.service (port 8080)              │ │
│  │  ├── choiros-frontend.service (port 5173)             │ │
│  │  └── caddy.service (port 80/443)                      │ │
│  └───────────────────────────────────────────────────────┘ │
│  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐  │
│  │  SQLite DB    │  │  Logs         │  │  Config       │  │
│  │  /data/events │  │  /var/log     │  │  /etc/choiros │  │
│  └───────────────┘  └───────────────┘  └───────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

#### Pros
- **Simplicity:** Minimal infrastructure, easy to debug
- **Performance:** Direct hardware access, no container overhead
- **Resource Efficiency:** Lower memory/CPU usage (~100MB base vs 500MB+ container)
- **Fast Iteration:** Direct binary replacement, no image building
- **Native Integration:** Uses OS tools (journalctl, systemctl)

#### Cons
- **Environment Drift:** Hard to replicate across environments
- **Dependency Management:** System-level deps must be managed manually
- **Scaling:** Horizontal scaling requires complex orchestration
- **Rollback:** Manual process unless implemented (two-binary pattern)
- **Isolation:** Less isolation between services

#### Implementation Pattern

**systemd Service File:**
```ini
# /etc/systemd/system/choiros-backend.service
[Unit]
Description=ChoirOS Backend API
After=network.target
Wants=network.target

[Service]
Type=simple
User=choiros
Group=choiros
WorkingDirectory=/opt/choiros
ExecStart=/opt/choiros/bin/sandbox
Environment="RUST_LOG=info"
Environment="DATABASE_URL=/opt/choiros/data/events.db"
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

**Two-Binary Rollback Pattern:**
```bash
#!/bin/bash
# deploy-safe.sh - Zero-downtime deployment with rollback

DEPLOY_DIR="/opt/choiros"
BACKUP_DIR="${DEPLOY_DIR}/backups"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)

# Create backup
mkdir -p "${BACKUP_DIR}"
cp "${DEPLOY_DIR}/bin/sandbox" "${BACKUP_DIR}/sandbox-${TIMESTAMP}"

# Deploy new binary
cp ./target/release/sandbox "${DEPLOY_DIR}/bin/sandbox.new"

# Health check on new binary
"${DEPLOY_DIR}/bin/sandbox.new" --health-check || {
  echo "Health check failed, rolling back..."
  cp "${BACKUP_DIR}/sandbox-${TIMESTAMP}" "${DEPLOY_DIR}/bin/sandbox"
  exit 1
}

# Atomic swap
mv "${DEPLOY_DIR}/bin/sandbox" "${DEPLOY_DIR}/bin/sandbox.old"
mv "${DEPLOY_DIR}/bin/sandbox.new" "${DEPLOY_DIR}/bin/sandbox"

# Graceful reload
systemctl reload choiros-backend || systemctl restart choiros-backend

echo "Deployment successful, backup: ${BACKUP_DIR}/sandbox-${TIMESTAMP}"
```

---

### 1.2 Containerized Deployment

**Pattern:** Deploy via Docker/Podman containers

#### Architecture
```
┌─────────────────────────────────────────────────────────────┐
│                        EC2 Instance                          │
│  ┌───────────────────────────────────────────────────────┐ │
│  │  Docker/Podman                                         │ │
│  │  ├── choiros-backend:latest (port 8080)               │ │
│  │  ├── choiros-frontend:latest (port 5173)              │ │
│  │  └── caddy:latest (port 80/443)                       │ │
│  └───────────────────────────────────────────────────────┘ │
│  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐  │
│  │  Named Vol    │  │  Named Vol    │  │  Network      │ │
│  │  choir-data   │  │  choir-logs   │  │  choir-net    │ │
│  └───────────────┘  └───────────────┘  └───────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

#### Pros
- **Environment Consistency:** Same image across dev/staging/prod
- **Easy Rollback:** Image tags provide instant rollback
- **Isolation:** Process + filesystem + network isolation
- **Scalability:** Easy to move to ECS/EKS later
- **Dependency Management:** All deps bundled in image

#### Cons
- **Resource Overhead:** ~400MB memory overhead per container
- **Complexity:** Additional layer to debug and monitor
- **Image Management:** Need to manage registry, pruning, security scanning
- **Slower Iteration:** Build + push + pull cycle

#### Implementation Pattern

**Dockerfile (Multi-stage for Rust):**
```dockerfile
# Build stage
FROM rust:1.75-bullseye AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin sandbox

# Runtime stage
FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/sandbox /app/sandbox
COPY --from=builder /app/sandbox/static /app/static
EXPOSE 8080
CMD ["/app/sandbox"]
```

**Docker Compose:**
```yaml
version: '3.8'
services:
  backend:
    build: .
    ports:
      - "8080:8080"
    volumes:
      - choir-data:/data
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3
    environment:
      - RUST_LOG=info
      - DATABASE_URL=/data/events.db

  caddy:
    image: caddy:latest
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile
      - caddy-data:/data
      - caddy-config:/config
    depends_on:
      - backend

volumes:
  choir-data:
  caddy-data:
  caddy-config:
```

---

### 1.3 Comparison Matrix

| Criterion | systemd | Docker | Podman |
|-----------|---------|--------|--------|
| **Memory Overhead** | ~100MB | ~500MB | ~450MB |
| **Startup Time** | 1-2s | 3-5s | 3-5s |
| **Deployment Speed** | Binary copy (seconds) | Image build/push/pull (minutes) | Same as Docker |
| **Rollback Speed** | Manual or script | Image tag swap | Same as Docker |
| **Security Isolation** | Process only | Multi-level (user, namespace, cgroups) | Rootless containers |
| **Debuggability** | Native OS tools | Container-specific tools | Same as Docker |
| **Learning Curve** | Low | Medium | Medium |
| **Migration Path to ECS/EKS** | Requires containerization | Direct migration | Direct migration |

---

### 1.4 Recommendation

**For ChoirOS (Current Phase - Pre-MVP):**
- **Use systemd for primary deployment**
  - Faster iteration, simpler debugging, lower cost
  - Implement two-binary rollback pattern for safety
  - Add CI pre-build to reduce server build time

**Transition Criteria:**
1. When you have real user traffic noticing downtime
2. When builds take too long on server (>5 min)
3. When you need horizontal scaling
4. When you want consistent environments across teams

**Containerization Path:**
1. Start with Docker Compose for local development
2. Build images in CI, keep systemd for prod (hybrid approach)
3. Transition to full container deployment when scaling needs arise

---

## 2. Blue-Green Deployments

### 2.1 Architecture

**Pattern:** Run two identical environments (blue and green), switch traffic after health checks

```
                        ┌─────────────────────┐
                        │    Load Balancer    │
                        │  (ALB / Caddy)     │
                        └──────────┬──────────┘
                                   │
                                   │ Switch
                                   ▼
                        ┌─────────────────────┐
                        │   Current Active    │
                        │   (Green: v2.0)     │
                        └─────────────────────┘
                                   │
                                   │ Switch
                                   ▼
                        ┌─────────────────────┐
                        │   Previous Active   │
                        │   (Blue: v1.9)      │
                        └─────────────────────┘
```

### 2.2 Implementation Strategies

#### Strategy A: Two EC2 Instances with ALB

**Architecture:**
- 2 EC2 instances (blue, green)
- Application Load Balancer routes traffic
- Deploy to inactive instance, health check, switch traffic

**Cost:** ~$146/month (2 × t3.small, 24/7)

**Pros:**
- True zero-downtime
- Instant rollback
- Health checks prevent bad deploys

**Cons:**
- Double infrastructure cost
- More complex to manage
- Overkill for low-traffic apps

#### Strategy B: Single EC2 with Dual Services

**Architecture:**
```
EC2 Instance:
  ├── choiros-backend-blue.service (port 8080)
  ├── choiros-backend-green.service (port 8081)
  └── Caddy (routes to active port)
```

**Cost:** Same as single instance (~$73/month for t3.small)

**Pros:**
- No additional infrastructure cost
- Fast rollback (port swap)
- Easy to implement

**Cons:**
- Brief downtime during port switch (<1s)
- Limited by instance resources
- Concurrent deploys not supported

**Implementation:**
```bash
#!/bin/bash
# blue-green-deploy.sh

ACTIVE_COLOR=$1  # "blue" or "green"
INACTIVE_COLOR=$([ "$ACTIVE_COLOR" = "blue" ] && echo "green" || echo "blue")

# Deploy to inactive color
PORT=$([ "$INACTIVE_COLOR" = "blue" ] && echo "8080" || echo "8081")

systemctl stop choiros-backend-${INACTIVE_COLOR}
cp ./target/release/sandbox /opt/choiros/bin/sandbox-${INACTIVE_COLOR}
systemctl start choiros-backend-${INACTIVE_COLOR}

# Health check
for i in {1..30}; do
  if curl -f http://localhost:${PORT}/health > /dev/null 2>&1; then
    echo "Health check passed"
    break
  fi
  echo "Waiting for health check ($i/30)..."
  sleep 2
done

# Switch Caddy config
sed -i "s/localhost:8080/localhost:${PORT}/" /etc/caddy/Caddyfile
systemctl reload caddy

# Stop old service
sleep 5
systemctl stop choiros-backend-${ACTIVE_COLOR}

echo "Deployed to ${INACTIVE_COLOR}, switched traffic"
```

#### Strategy C: Container-Based Blue/Green

**Architecture:**
```
Docker Compose:
  ├── backend-blue (port 8080)
  ├── backend-green (port 8081)
  └── caddy (routes to active container)
```

**Implementation:**
```bash
#!/bin/bash
# container-blue-green.sh

ACTIVE=$1  # "blue" or "green"
INACTIVE=$([ "$ACTIVE" = "blue" ] && echo "green" || echo "blue")

# Build and deploy to inactive
docker build -t choir:latest .
docker stop choir-backend-${ACTIVE}
docker run -d --name choir-backend-${INACTIVE} -p 8081:8080 \
  -v choir-data:/data choir:latest

# Health check
until docker exec choir-backend-${INACTIVE} wget -q -O /dev/null http://localhost:8080/health; do
  echo "Waiting for container to be healthy..."
  sleep 2
done

# Switch Caddy routing
docker exec caddy sh -c "sed -i 's/8080/8081/g' /etc/caddy/Caddyfile && caddy reload"

# Clean up
docker stop choir-backend-${ACTIVE}
```

### 2.3 Health Check Strategies

#### Application Health Checks

**Rust Implementation (actix-web):**
```rust
use actix_web::{get, web, HttpResponse, Responder};

#[get("/health")]
async fn health_check() -> impl Responder {
    // Check database connectivity
    if let Err(e) = check_db_connection().await {
        return HttpResponse::ServiceUnavailable()
            .json(format!("Database unavailable: {}", e));
    }

    // Check critical dependencies
    if let Err(e) = check_dependencies().await {
        return HttpResponse::ServiceUnavailable()
            .json(format!("Dependency check failed: {}", e));
    }

    HttpResponse::Ok().json(json!({
        "status": "healthy",
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": Utc::now().to_rfc3339(),
    }))
}
```

#### Load Balancer Health Checks

**ALB Health Check Configuration:**
```
Health Check Path: /health
Healthy Threshold: 3
Unhealthy Threshold: 2
Interval: 30 seconds
Timeout: 10 seconds
Matcher: HTTP 200
```

#### systemd Health Checks

**systemd Service with HealthCheck:**
```ini
[Service]
ExecStart=/opt/choiros/bin/sandbox
ExecStartPost=/opt/choiros/scripts/health-check.sh
Restart=on-failure
RestartSec=10

[Service]
ExecStart=/opt/choiros/bin/sandbox
ExecStartPost=/bin/sleep 10
```

### 2.4 Rollback Strategies

#### Instant Rollback (Blue/Green)
```bash
# Rollback to previous color
./blue-green-deploy.sh blue  # If currently on green
```

#### Two-Binary Rollback (systemd)
```bash
#!/bin/bash
# rollback.sh
BACKUP_DIR="/opt/choiros/backups"

# Get latest backup
LATEST_BACKUP=$(ls -t "${BACKUP_DIR}/sandbox-" | head -n1)

if [ -z "$LATEST_BACKUP" ]; then
  echo "No backups found"
  exit 1
fi

# Restore
cp "${BACKUP_DIR}/${LATEST_BACKUP}" /opt/choiros/bin/sandbox
systemctl restart choiros-backend

echo "Rolled back to ${LATEST_BACKUP}"
```

#### Container Image Rollback
```bash
# List available images
docker images choir --format "table {{.Tag}}\t{{.CreatedAt}}"

# Rollback to specific tag
docker stop choir-backend
docker run -d --name choir-backend -p 8080:8080 \
  -v choir-data:/data choir:1.9.0
```

### 2.5 Cost-Benefit Analysis

| Strategy | Infrastructure Cost | Complexity | Downtime | Rollback Time |
|----------|---------------------|------------|----------|---------------|
| **Single Instance (current)** | $73/mo (t3.small) | Low | 10-30s | Manual/Script |
| **Two-Binary Pattern** | $73/mo (t3.small) | Low | <5s | <10s |
| **Single EC2 + Blue/Green** | $73/mo (t3.small) | Medium | <1s | <5s |
| **Two EC2s + ALB** | $146/mo (2×t3.small) + $20/mo (ALB) | High | 0s | <1s |

**Recommendation:**
- **Current:** Two-binary pattern (no cost increase, fast rollback)
- **Growth Phase:** Single EC2 + blue/green (same cost, true zero-downtime)
- **Production:** Two EC2s + ALB (only when SLA requirements demand it)

---

## 3. Health Checks and Auto-Recovery

### 3.1 Health Check Layers

```
┌─────────────────────────────────────────────────────────────┐
│                    Health Check Layers                        │
│                                                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Layer 1: Application Health                         │   │
│  │  ├── /health endpoint (actix-web)                   │   │
│  │  ├── Database connectivity                          │   │
│  │  └── Critical dependencies (LLM, external APIs)     │   │
│  └─────────────────────────────────────────────────────┘   │
│                          ▼                                  │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Layer 2: System Health                             │   │
│  │  ├── systemd service status                         │   │
│  │  ├── Resource usage (CPU, memory, disk)              │   │
│  │  └── Process health                                 │   │
│  └─────────────────────────────────────────────────────┘   │
│                          ▼                                  │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Layer 3: Infrastructure Health                      │   │
│  │  ├── EC2 instance status                            │   │
│  │  ├── Load balancer health (if applicable)            │   │
│  │  └── Network connectivity                            │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Application-Level Health Checks

#### Comprehensive Health Check Endpoint

```rust
// src/health.rs
use actix_web::{get, web, HttpResponse, Responder};
use serde::Serialize;
use sqlx::SqlitePool;
use std::time::Duration;

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    timestamp: String,
    checks: HealthChecks,
}

#[derive(Serialize)]
struct HealthChecks {
    database: CheckResult,
    database_latency_ms: Option<u64>,
    memory_usage_mb: Option<f64>,
    cpu_usage_percent: Option<f64>,
    disk_usage_percent: Option<f64>,
}

#[derive(Serialize)]
struct CheckResult {
    status: String,
    message: Option<String>,
}

#[get("/health")]
pub async fn health_check(pool: web::Data<SqlitePool>) -> impl Responder {
    let checks = HealthChecks {
        database: check_database(&pool).await,
        database_latency_ms: Some(measure_db_latency(&pool).await),
        memory_usage_mb: Some(get_memory_usage()),
        cpu_usage_percent: Some(get_cpu_usage()),
        disk_usage_percent: Some(get_disk_usage()),
    };

    let overall_status = if checks.database.status == "ok" {
        "healthy"
    } else {
        "unhealthy"
    };

    HttpResponse::Ok().json(HealthResponse {
        status: overall_status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: Utc::now().to_rfc3339(),
        checks,
    })
}

async fn check_database(pool: &SqlitePool) -> CheckResult {
    match sqlx::query("SELECT 1").fetch_one(pool).await {
        Ok(_) => CheckResult {
            status: "ok".to_string(),
            message: None,
        },
        Err(e) => CheckResult {
            status: "error".to_string(),
            message: Some(format!("Database check failed: {}", e)),
        },
    }
}

async fn measure_db_latency(pool: &SqlitePool) -> u64 {
    let start = std::time::Instant::now();
    let _ = sqlx::query("SELECT 1").fetch_one(pool).await;
    start.elapsed().as_millis() as u64
}

fn get_memory_usage() -> f64 {
    // Read /proc/meminfo or use sysinfo crate
    use sysinfo::{System, SystemExt};
    let mut sys = System::new_all();
    sys.refresh_all();
    (sys.used_memory() as f64) / (1024.0 * 1024.0)
}

fn get_cpu_usage() -> f64 {
    use sysinfo::{System, SystemExt};
    let mut sys = System::new_all();
    sys.refresh_all();
    sys.global_cpu_info().cpu_usage()
}

fn get_disk_usage() -> f64 {
    use sysinfo::{DiskExt, System, SystemExt};
    let mut sys = System::new_all();
    sys.refresh_disks();
    let disk = sys.disks().first()?;
    (disk.total_space() - disk.available_space()) as f64 / disk.total_space() as f64 * 100.0
}
```

#### Dependency Health Checks

```rust
// Check external services (LLM, APIs)
async fn check_dependencies() -> CheckResult {
    let mut checks = vec![];

    // Check BAML/LLM availability
    match check_llm_connection().await {
        Ok(_) => checks.push(("LLM", "ok")),
        Err(e) => checks.push(("LLM", &format!("error: {}", e))),
    }

    // Check other external services
    // ...

    let all_ok = checks.iter().all(|(_, status)| *status == "ok");
    CheckResult {
        status: if all_ok { "ok" } else { "degraded" }.to_string(),
        message: if all_ok {
            None
        } else {
            Some(format!("Dependency issues: {:?}", checks))
        },
    }
}
```

### 3.3 System-Level Health Checks

#### systemd Health Monitor

```bash
#!/bin/bash
# scripts/health-monitor.sh

# Check service status
check_service() {
    local service=$1
    if systemctl is-active --quiet "$service"; then
        echo "✓ $service is running"
        return 0
    else
        echo "✗ $service is not running"
        return 1
    fi
}

# Check resource usage
check_resources() {
    local cpu=$(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | cut -d'%' -f1)
    local mem=$(free -m | awk 'NR==2{printf "%.1f", $3*100/$2 }')
    local disk=$(df -h / | awk 'NR==2 {print $5}' | cut -d'%' -f1)

    echo "CPU: ${cpu}% | Memory: ${mem}% | Disk: ${disk}%"

    if (( $(echo "$cpu > 80" | bc -l) )); then
        echo "⚠️  High CPU usage"
        return 1
    fi

    if (( $(echo "$mem > 80" | bc -l) )); then
        echo "⚠️  High memory usage"
        return 1
    fi

    if (( disk > 80 )); then
        echo "⚠️  High disk usage"
        return 1
    fi

    return 0
}

# Main loop
main() {
    local failed=0

    check_service "choiros-backend" || failed=1
    check_service "choiros-frontend" || failed=1
    check_service "caddy" || failed=1
    check_resources || failed=1

    if [ $failed -eq 1 ]; then
        echo "Health check failed!"
        exit 1
    else
        echo "All health checks passed"
        exit 0
    fi
}

main "$@"
```

#### Automated Recovery with systemd

```ini
# /etc/systemd/system/choiros-backend.service
[Unit]
Description=ChoirOS Backend API
After=network.target
Wants=network.target

[Service]
Type=simple
User=choiros
Group=choiros
WorkingDirectory=/opt/choiros
ExecStart=/opt/choiros/bin/sandbox
Environment="RUST_LOG=info"
Environment="DATABASE_URL=/opt/choiros/data/events.db"

# Auto-restart on failure
Restart=always
RestartSec=10
StartLimitInterval=60
StartLimitBurst=5

# Health check
ExecStartPost=/opt/choiros/scripts/health-check.sh
TimeoutStartSec=60

# Resource limits
LimitNOFILE=65536
MemoryMax=2G
CPUQuota=200%

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=choiros-backend

[Install]
WantedBy=multi-user.target
```

```bash
#!/bin/bash
# /opt/choiros/scripts/health-check.sh
# Runs after service starts

MAX_RETRIES=30
RETRY_INTERVAL=2

for i in $(seq 1 $MAX_RETRIES); do
    if curl -f http://localhost:8080/health > /dev/null 2>&1; then
        echo "Health check passed"
        exit 0
    fi
    echo "Health check attempt $i/$MAX_RETRIES failed, retrying..."
    sleep $RETRY_INTERVAL
done

echo "Health check failed after $MAX_RETRIES attempts"
exit 1
```

### 3.4 Infrastructure-Level Health Checks

#### AWS CloudWatch Alarms

```bash
#!/bin/bash
# setup-cloudwatch-alarms.sh

AWS_REGION="us-east-1"
INSTANCE_ID=$(ec2-metadata -i | cut -d' ' -f2)

# High CPU alarm
aws cloudwatch put-metric-alarm \
  --alarm-name "choiros-high-cpu" \
  --alarm-description "Alert on CPU > 80% for 5 minutes" \
  --metric-name CPUUtilization \
  --namespace AWS/EC2 \
  --statistic Average \
  --period 300 \
  --evaluation-periods 1 \
  --threshold 80 \
  --comparison-operator GreaterThanThreshold \
  --dimensions Name=InstanceId,Value=$INSTANCE_ID \
  --region $AWS_REGION

# High memory alarm (custom metric requires CloudWatch agent)
aws cloudwatch put-metric-alarm \
  --alarm-name "choiros-high-memory" \
  --alarm-description "Alert on Memory > 80% for 5 minutes" \
  --metric-name MemoryUtilization \
  --namespace CWAgent \
  --statistic Average \
  --period 300 \
  --evaluation-periods 1 \
  --threshold 80 \
  --comparison-operator GreaterThanThreshold \
  --dimensions Name=InstanceId,Value=$INSTANCE_ID \
  --region $AWS_REGION

# Status check alarm
aws cloudwatch put-metric-alarm \
  --alarm-name "choiros-status-check" \
  --alarm-description "Alert on EC2 status check failure" \
  --metric-name StatusCheckFailed \
  --namespace AWS/EC2 \
  --statistic Maximum \
  --period 60 \
  --evaluation-periods 1 \
  --threshold 1 \
  --comparison-operator GreaterThanOrEqualToThreshold \
  --dimensions Name=InstanceId,Value=$INSTANCE_ID \
  --region $AWS_REGION
```

#### Auto Scaling with EC2

```bash
#!/bin/bash
# setup-auto-scaling.sh

# Create launch template
aws ec2 create-launch-template \
  --launch-template-name choiros-template \
  --image-id ami-0c55b159cbfafe1f0 \
  --instance-type t3.small \
  --key-name choiros-keypair \
  --security-group-ids sg-0123456789abcdef0 \
  --user-data file://user-data.sh

# Create auto scaling group
aws autoscaling create-auto-scaling-group \
  --auto-scaling-group-name choiros-asg \
  --launch-template "LaunchTemplateName=choiros-template,Version=\$Default" \
  --min-size 2 \
  --max-size 4 \
  --desired-capacity 2 \
  --target-group-arns arn:aws:elasticloadbalancing:us-east-1:123456789012:targetgroup/choiros-tg/abc123 \
  --vpc-zone-identifier "subnet-12345,subnet-67890"

# Scale up policy
aws autoscaling put-scaling-policy \
  --auto-scaling-group-name choiros-asg \
  --policy-name scale-up \
  --scaling-adjustment 1 \
  --adjustment-type ChangeInCapacity \
  --cooldown 300

# Scale down policy
aws autoscaling put-scaling-policy \
  --auto-scaling-group-name choiros-asg \
  --policy-name scale-down \
  --scaling-adjustment -1 \
  --adjustment-type ChangeInCapacity \
  --cooldown 600
```

### 3.5 Alerting and Notification

#### AlertManager Integration

```yaml
# alertmanager.yml
global:
  resolve_timeout: 5m

route:
  group_by: ['alertname']
  group_wait: 10s
  group_interval: 10s
  repeat_interval: 12h
  receiver: 'default'

  routes:
    - match:
        severity: critical
      receiver: 'critical-alerts'

receivers:
  - name: 'default'
    webhook_configs:
      - url: 'http://slack-webhook-url'

  - name: 'critical-alerts'
    webhook_configs:
      - url: 'http://slack-webhook-url'
    email_configs:
      - to: 'ops@example.com'
```

#### Prometheus Rules

```yaml
# prometheus-rules.yml
groups:
  - name: choiros-alerts
    interval: 30s
    rules:
      - alert: HighRequestLatency
        expr: histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[5m])) > 0.5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High request latency"

      - alert: HighErrorRate
        expr: rate(http_requests_total{status=~"5.."}[5m]) > 0.05
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "High error rate"

      - alert: ServiceDown
        expr: up{job="choiros-backend"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Service is down"

      - alert: HighMemoryUsage
        expr: process_resident_memory_bytes{job="choiros-backend"} / 1024 / 1024 > 1024
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "High memory usage (>1GB)"
```

---

## 4. Log Aggregation (Without CloudWatch $$$)

### 4.1 Cost Comparison

| Solution | Ingestion Cost | Storage Cost | Total/mo (10GB) | Setup Complexity |
|----------|----------------|--------------|-----------------|------------------|
| **CloudWatch Logs** | $0.50/GB | $0.03/GB | $5.30 | Low |
| **Loki + Grafana** | $0 | $0 | $0 | Medium |
| **ELK Stack** | $0 | $0 | $0 | High |
| **Vector + S3** | $0 | $0.023/GB | $0.23 | Medium |
| **Fluentd + S3** | $0 | $0.023/GB | $0.23 | Medium |

**Recommendation:** Loki + Grafana for cost-effectiveness and ease of use

### 4.2 Loki + Grafana Architecture

```
┌──────────────┐      ┌──────────────┐      ┌──────────────┐
│   EC2 Apps   │      │   Grafana    │      │   Alerts     │
│              │─────▶│  Dashboard   │◀─────│  (Slack/    │
│  systemd     │      │  (Logs UI)   │      │   Email)    │
│  journald    │      └──────────────┘      └──────────────┘
└──────┬───────┘               │
       │                       │
       │ Loki Push API         │
       ▼                       │
┌──────────────┐      ┌──────────────┐
│     Loki     │◀─────│  Promtail    │
│   (Logs)     │      │  (Logs)     │
└──────────────┘      └──────────────┘
```

### 4.3 Implementation

#### Install Loki and Promtail

```bash
#!/bin/bash
# install-loki.sh

# Create directories
sudo mkdir -p /opt/loki /opt/promtail /var/log/loki

# Download Loki
cd /tmp
curl -s -L https://github.com/grafana/loki/releases/download/v2.9.4/loki-linux-amd64.zip -o loki.zip
unzip loki.zip
sudo mv loki-linux-amd64 /usr/local/bin/loki
chmod +x /usr/local/bin/loki

# Download Promtail
curl -s -L https://github.com/grafana/loki/releases/download/v2.9.4/promtail-linux-amd64.zip -o promtail.zip
unzip promtail.zip
sudo mv promtail-linux-amd64 /usr/local/bin/promtail
chmod +x /usr/local/bin/promtail

# Create Loki config
sudo tee /opt/loki/config.yml > /dev/null <<EOF
auth_enabled: false

server:
  http_listen_port: 3100

common:
  path_prefix: /loki
  storage:
    filesystem:
      chunks_directory: /loki/chunks
      rules_directory: /loki/rules
  replication_factor: 1
  ring:
    instance_addr: 127.0.0.1
    kvstore:
      store: inmemory

schema_config:
  configs:
    - from: 2020-10-24
      store: boltdb-shipper
      object_store: filesystem
      schema: v11
      index:
        prefix: index_
        period: 24h

ruler:
  alertmanager_url: http://localhost:9093

limits_config:
  enforce_metric_name: false
  reject_old_samples: true
  reject_old_samples_max_age: 168h
EOF

# Create Promtail config
sudo tee /opt/promtail/config.yml > /dev/null <<EOF
server:
  http_listen_port: 9080
  grpc_listen_port: 0

positions:
  filename: /tmp/positions.yaml

clients:
  - url: http://localhost:3100/loki/api/v1/push

scrape_configs:
  - job_name: systemd
    journal:
      max_age: 12h
      labels:
        job: systemd
        host: \${hostname}
    relabel_configs:
      - source_labels: ['__journal__systemd_unit']
        target_label: 'unit'
      - source_labels: ['__journal_priority_keyword']
        target_label: 'level'

  - job_name: choiros-logs
    static_configs:
      - targets:
          - localhost
        labels:
          job: choiros
          __path__: /opt/choiros/logs/*.log
EOF

# Create systemd services
sudo tee /etc/systemd/system/loki.service > /dev/null <<EOF
[Unit]
Description=Loki log aggregation system
After=network-online.target

[Service]
User=loki
Group=loki
ExecStart=/usr/local/bin/loki -config.file=/opt/loki/config.yml
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

sudo tee /etc/systemd/system/promtail.service > /dev/null <<EOF
[Unit]
Description=Promtail log shipper
After=network-online.target

[Service]
User=promtail
Group=promtail
ExecStart=/usr/local/bin/promtail -config.file=/opt/promtail/config.yml
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

# Create users
sudo useradd -r -s /bin/false loki || true
sudo useradd -r -s /bin/false promtail || true

# Set permissions
sudo chown -R loki:loki /opt/loki /var/log/loki
sudo chown -R promtail:promtail /opt/promtail

# Enable and start
sudo systemctl enable loki promtail
sudo systemctl start loki promtail

echo "Loki and Promtail installed successfully"
```

#### Install Grafana

```bash
#!/bin/bash
# install-grafana.sh

# Add Grafana repo
sudo apt-get install -y apt-transport-https software-properties-common
sudo wget -q -O /usr/share/keyrings/grafana.key https://packages.grafana.com/gpg.key
echo "deb [signed-by=/usr/share/keyrings/grafana.key] https://packages.grafana.com/oss/deb stable main" | sudo tee /etc/apt/sources.list.d/grafana.list

# Install Grafana
sudo apt-get update
sudo apt-get install -y grafana

# Enable and start
sudo systemctl enable grafana-server
sudo systemctl start grafana-server

echo "Grafana installed. Access at http://localhost:3000 (admin/admin)"
```

#### Configure Grafana Data Source

```bash
#!/bin/bash
# configure-loki-datasource.sh

GRAFANA_URL="http://localhost:3000"
GRAFANA_USER="admin"
GRAFANA_PASS="admin"

# Add Loki datasource
curl -X POST "${GRAFANA_URL}/api/datasources" \
  -H "Content-Type: application/json" \
  -u "${GRAFANA_USER}:${GRAFANA_PASS}" \
  -d '{
    "name": "Loki",
    "type": "loki",
    "url": "http://localhost:3100",
    "access": "proxy",
    "isDefault": true
  }'

echo "Loki datasource added to Grafana"
```

### 4.4 Log Query Examples

#### Query systemd logs
```logql
{job="systemd", unit="choiros-backend.service"} |= "error"
```

#### Query application logs
```logql
{job="choiros"} | logfmt | level="error"
```

#### Query by time range
```logql
{job="choiros"} | line_format "{{.timestamp}} {{.message}}" > 24h
```

#### Query with metrics
```logql
count_over_time({job="choiros"} | logfmt | level="error" [5m])
```

### 4.5 Log Retention and Archival

#### Retention Policy in Loki
```yaml
# loki-config.yml
limits_config:
  retention_period: 720h  # 30 days
```

#### Archival to S3
```yaml
# loki-config.yml
schema_config:
  configs:
    - from: 2020-10-24
      store: boltdb-shipper
      object_store: s3
      schema: v11
      index:
        prefix: index_
        period: 24h

storage_config:
  s3:
    endpoint: s3.amazonaws.com
    bucketnames: choiros-logs
    access_key_id: ${AWS_ACCESS_KEY_ID}
    secret_access_key: ${AWS_SECRET_ACCESS_KEY}
    region: us-east-1
```

### 4.6 Alternative: Vector + S3

For long-term archival and cost optimization:

```
┌──────────────┐      ┌──────────────┐      ┌──────────────┐
│   EC2 Apps   │      │    Vector    │      │     S3       │
│  systemd     │─────▶│  (Shipper)   │─────▶│  (Archive)   │
│  journald    │      └──────────────┘      └──────────────┘
└──────────────┘               │
                               │
                               ▼
                        ┌──────────────┐
                        │    Loki      │
                        │  (Recent)    │
                        └──────────────┘
```

**Vector Configuration:**
```toml
# vector.toml
[sources.journald]
type = "journald"
include_units = ["choiros-backend.service", "choiros-frontend.service"]

[transforms.parse_logs]
type = "remap"
inputs = ["journald"]
source = '''
. = parse_syslog!(.message)
.timestamp = parse_timestamp!(.timestamp, "%+") ?? now()
'''

[sinks.loki]
type = "loki"
inputs = ["parse_logs"]
endpoint = "http://localhost:3100"
encoding.codec = "json"

[sinks.s3]
type = "aws_s3"
inputs = ["parse_logs"]
bucket = "choiros-logs-archive"
region = "us-east-1"
key_prefix = "logs/%Y/%m/%d/"
encoding.codec = "json"
compression = "gzip"
batch.max_events = 1000
batch.timeout_secs = 60
```

**Cost with Vector + S3:**
- S3 Standard: $0.023/GB/month
- For 100GB logs: $2.30/month
- With 30-day retention: $2.30/month
- **CloudWatch equivalent:** $50/month

---

## 5. Cost Optimization for Dev/Staging

### 5.1 Instance Type Comparison

| Instance | vCPU | Memory | Hourly Cost | Monthly Cost | Suitable For |
|----------|------|--------|-------------|--------------|--------------|
| **t3.nano** | 2 | 0.5GB | $0.0042 | $3.03 | Development |
| **t3.micro** | 2 | 1GB | $0.0084 | $6.05 | Development |
| **t3.small** | 2 | 2GB | $0.0168 | $12.10 | Staging |
| **t3.medium** | 2 | 4GB | $0.0336 | $24.20 | Staging |
| **c5.large** | 2 | 4GB | $0.102 | $73.44 | Production |
| **c5.xlarge** | 4 | 8GB | $0.204 | $146.88 | Production |

### 5.2 Optimization Strategies

#### Strategy A: Scheduled Start/Stop

**Use Case:** Development environments not needed 24/7

```bash
#!/bin/bash
# schedule-dev-start-stop.sh

INSTANCE_ID="i-0123456789abcdef0"

# Stop at 10 PM UTC (6 PM EST)
aws ec2 create-scheduled-instance \
  --scheduled-instance-id $INSTANCE_ID \
  --start-time 2024-02-01T14:00:00Z \
  --end-time 2024-02-01T22:00:00Z

# Alternative: Use Lambda + EventBridge
# Lambda function to stop instance:
python3 <<EOF
import boto3
import logging

logger = logging.getLogger()
logger.setLevel(logging.INFO)

ec2 = boto3.client('ec2')

def lambda_handler(event, context):
    instances = ['i-0123456789abcdef0']
    ec2.stop_instances(InstanceIds=instances)
    logger.info(f"Stopped instances: {instances}")
    return {'statusCode': 200, 'body': f"Stopped {instances}"}
EOF
```

**Cost Savings:**
- 24/7 t3.small: $12.10/month
- 9 AM - 10 PM (13 hours/day, 5 days/week): $7.04/month
- **Savings:** 42% ($5.06/month)

#### Strategy B: Spot Instances

**Use Case:** Fault-tolerant workloads, can be interrupted

```bash
#!/bin/bash
# launch-spot-instance.sh

aws ec2 request-spot-instances \
  --spot-price "0.008" \
  --instance-count 1 \
  --type "one-time" \
  --launch-specification "{
    \"ImageId\": \"ami-0c55b159cbfafe1f0\",
    \"InstanceType\": \"t3.small\",
    \"KeyName\": \"choiros-keypair\",
    \"SecurityGroupIds\": [\"sg-0123456789abcdef0\"],
    \"IamInstanceProfile\": {\"Name\": \"choiros-profile\"}
  }"
```

**Cost Savings:**
- On-demand t3.small: $0.0168/hour
- Spot t3.small: ~$0.0084/hour (50% off)
- **Savings:** 50%

#### Strategy C: Reserved Instances

**Use Case:** Production workloads, predictable usage

```bash
#!/bin/bash
# purchase-reserved-instance.sh

aws ec2 purchase-reserved-instances-offering \
  --reserved-instances-offering-id $(aws ec2 describe-reserved-instances-offerings \
    --instance-type c5.large \
    --product-description "Linux/UNIX" \
    --offering-type "All Upfront" \
    --instance-count 1 \
    --query "ReservedInstancesOfferings[0].ReservedInstancesOfferingId" \
    --output text) \
  --instance-count 1
```

**Cost Savings:**
- On-demand c5.large: $0.102/hour = $73.44/month
- 1-year All Upfront: ~$680 ($56.67/month)
- **Savings:** 23%

#### Strategy D: Auto Scaling Group with Scale-to-Zero

**Use Case:** Staging environments, low traffic

```bash
#!/bin/bash
# setup-scale-to-zero-asg.sh

aws autoscaling create-auto-scaling-group \
  --auto-scaling-group-name choiros-staging \
  --launch-template "LaunchTemplateName=choiros-template,Version=\$Default" \
  --min-size 0 \
  --max-size 2 \
  --desired-capacity 0 \
  --target-group-arns arn:aws:elasticloadbalancing:us-east-1:123456789012:targetgroup/choiros-staging-tg/abc123 \
  --default-cooldown 300

# Scale up on demand
aws autoscaling put-scaling-policy \
  --auto-scaling-group-name choiros-staging \
  --policy-name scale-on-demand \
  --scaling-adjustment 1 \
  --adjustment-type ChangeInCapacity \
  --cooldown 300

# Scale down after inactivity
aws autoscaling put-scaling-policy \
  --auto-scaling-group-name choiros-staging \
  --policy-name scale-to-zero \
  --scaling-adjustment -1 \
  --adjustment-type ChangeInCapacity \
  --cooldown 1800
```

**Cost Savings:**
- Staging 24/7: $12.10/month
- Staging 10 hours/day (when needed): $5.04/month
- **Savings:** 58%

### 5.3 Cost Monitoring

#### CloudWatch Cost Alarms

```bash
#!/bin/bash
# setup-cost-alarms.sh

# Alert if monthly cost > $50
aws cloudwatch put-metric-alarm \
  --alarm-name "choiros-monthly-cost-alert" \
  --alarm-description "Alert when monthly cost exceeds $50" \
  --metric-name EstimatedCharges \
  --namespace AWS/Billing \
  --statistic Maximum \
  --period 21600 \
  --evaluation-periods 1 \
  --threshold 50 \
  --comparison-operator GreaterThanThreshold \
  --dimensions Name=Currency,Value=USD \
  --alarm-actions arn:aws:sns:us-east-1:123456789012:cost-alerts
```

#### Daily Cost Report

```bash
#!/bin/bash
# daily-cost-report.sh

# Get current month's cost
COST=$(aws ce get-cost-and-usage \
  --time-start $(date -d "$(date +%Y-%m-01)" +%Y-%m-%d) \
  --time-end $(date -d "$(date +%Y-%m-%d) + 1 day" +%Y-%m-%d) \
  --granularity MONTHLY \
  --metrics BlendedCost \
  --query "ResultsByTime[0].Total.BlendedCost.Amount" \
  --output text)

echo "Current month's cost: \$${COST}"

# Breakdown by service
aws ce get-cost-and-usage \
  --time-start $(date -d "$(date +%Y-%m-01)" +%Y-%m-%d) \
  --time-end $(date -d "$(date +%Y-%m-%d) + 1 day" +%Y-%m-%d) \
  --granularity MONTHLY \
  --metrics BlendedCost \
  --group-by Type=DIMENSION,Key=SERVICE \
  --output table
```

### 5.4 Recommended Cost Optimization Strategy

**Development:**
- Use t3.nano/t3.micro
- Schedule start/stop (9 AM - 10 PM UTC, 5 days/week)
- Use spot instances if fault-tolerant
- **Estimated Cost:** $3-6/month

**Staging:**
- Use t3.small
- Auto scaling with scale-to-zero
- Start on demand (pre-deploy hook)
- **Estimated Cost:** $5-12/month

**Production:**
- Use c5.large or c5.xlarge
- Reserved instances for predictable workloads
- Auto scaling group (2-4 instances)
- **Estimated Cost:** $100-200/month

**Total Estimated Cost:** $108-218/month (vs $240-320/month without optimization)

---

## 6. Security Hardening

### 6.1 Security Checklist

#### Infrastructure Security

- [ ] **VPC Isolation**: Separate dev/staging/prod VPCs
- [ ] **Security Groups**: Whitelist-only, principle of least privilege
- [ ] **IAM Roles**: Instance roles vs access keys
- [ ] **SSH Access**: Key-based only, disable password auth
- [ ] **Bastion Host**: Jump host for SSH access
- [ ] **Multi-factor Authentication**: AWS root account + IAM users
- [ ] **Security Hub**: Enable for compliance monitoring

#### Application Security

- [ ] **HTTPS/TLS**: Valid certificates (Let's Encrypt or ACM)
- [ ] **CORS**: Restrict to trusted origins
- [ ] **Rate Limiting**: Implement at application and network level
- [ ] **Input Validation**: Validate all user inputs
- [ ] **SQL Injection Prevention**: Use parameterized queries (SQLx handles this)
- [ ] **Secrets Management**: AWS Secrets Manager or Parameter Store
- [ ] **Dependency Scanning**: `cargo audit` and Snyk

#### OS Security

- [ ] **Minimal OS Image**: Use minimal Ubuntu/Amazon Linux 2
- [ ] **Automatic Updates**: `unattended-upgrades`
- [ ] **Firewall**: `ufw` (in addition to security groups)
- [ ] **Fail2ban**: Block repeated SSH failures
- [ ] **Log Monitoring**: Detect intrusion attempts
- [ ] **File Integrity Monitoring**: AIDE or similar

#### Data Security

- [ ] **Encryption at Rest**: EBS encryption, S3 bucket policies
- [ ] **Encryption in Transit**: TLS for all connections
- [ ] **Database Encryption**: SQLite encryption or use encrypted volume
- [ ] **Backup Encryption**: Encrypt backups before uploading
- [ ] **Access Logs**: Monitor database and API access

### 6.2 Implementation Guide

#### VPC Configuration

```bash
#!/bin/bash
# setup-vpc.sh

# Create VPC
VPC_ID=$(aws ec2 create-vpc \
  --cidr-block 10.0.0.0/16 \
  --tag-specifications "ResourceType=vpc,Tags=[{Key=Name,Value=choiros-prod-vpc}]" \
  --query "Vpc.VpcId" \
  --output text)

# Enable DNS support
aws ec2 modify-vpc-attribute --vpc-id $VPC_ID --enable-dns-support "{\"Value\":true}"
aws ec2 modify-vpc-attribute --vpc-id $VPC_ID --enable-dns-hostnames "{\"Value\":true}"

# Create subnets
PUBLIC_SUBNET_1=$(aws ec2 create-subnet \
  --vpc-id $VPC_ID \
  --cidr-block 10.0.1.0/24 \
  --availability-zone us-east-1a \
  --tag-specifications "ResourceType=subnet,Tags=[{Key=Name,Value=public-us-east-1a}]" \
  --query "Subnet.SubnetId" \
  --output text)

PRIVATE_SUBNET_1=$(aws ec2 create-subnet \
  --vpc-id $VPC_ID \
  --cidr-block 10.0.2.0/24 \
  --availability-zone us-east-1a \
  --tag-specifications "ResourceType=subnet,Tags=[{Key=Name,Value=private-us-east-1a}]" \
  --query "Subnet.SubnetId" \
  --output text)

# Create internet gateway
IGW_ID=$(aws ec2 create-internet-gateway \
  --tag-specifications "ResourceType=internet-gateway,Tags=[{Key=Name,Value=choiros-igw}]" \
  --query "InternetGateway.InternetGatewayId" \
  --output text)

aws ec2 attach-internet-gateway --internet-gateway-id $IGW_ID --vpc-id $VPC_ID

# Create route table
ROUTE_TABLE_ID=$(aws ec2 create-route-table \
  --vpc-id $VPC_ID \
  --tag-specifications "ResourceType=route-table,Tags=[{Key=Name,Value=choiros-public-rt}]" \
  --query "RouteTable.RouteTableId" \
  --output text)

aws ec2 create-route \
  --route-table-id $ROUTE_TABLE_ID \
  --destination-cidr-block 0.0.0.0/0 \
  --gateway-id $IGW_ID

aws ec2 associate-route-table --route-table-id $ROUTE_TABLE_ID --subnet-id $PUBLIC_SUBNET_1

echo "VPC setup complete:"
echo "VPC: $VPC_ID"
echo "Public subnet: $PUBLIC_SUBNET_1"
echo "Private subnet: $PRIVATE_SUBNET_1"
echo "Internet gateway: $IGW_ID"
echo "Route table: $ROUTE_TABLE_ID"
```

#### Security Groups

```bash
#!/bin/bash
# setup-security-groups.sh

VPC_ID="vpc-0123456789abcdef0"

# Web server security group (allow HTTP/HTTPS from anywhere)
WEB_SG=$(aws ec2 create-security-group \
  --group-name choiros-web-sg \
  --description "ChoirOS web server security group" \
  --vpc-id $VPC_ID \
  --tag-specifications "ResourceType=security-group,Tags=[{Key=Name,Value=choiros-web-sg}]" \
  --query "GroupId" \
  --output text)

aws ec2 authorize-security-group-ingress \
  --group-id $WEB_SG \
  --protocol tcp \
  --port 80 \
  --cidr 0.0.0.0/0

aws ec2 authorize-security-group-ingress \
  --group-id $WEB_SG \
  --protocol tcp \
  --port 443 \
  --cidr 0.0.0.0/0

# Backend security group (allow from web SG only)
BACKEND_SG=$(aws ec2 create-security-group \
  --group-name choiros-backend-sg \
  --description "ChoirOS backend security group" \
  --vpc-id $VPC_ID \
  --tag-specifications "ResourceType=security-group,Tags=[{Key=Name,Value=choiros-backend-sg}]" \
  --query "GroupId" \
  --output text)

aws ec2 authorize-security-group-ingress \
  --group-id $BACKEND_SG \
  --protocol tcp \
  --port 8080 \
  --source-group $WEB_SG

# SSH security group (allow from specific IP only)
SSH_SG=$(aws ec2 create-security-group \
  --group-name choiros-ssh-sg \
  --description "ChoirOS SSH security group" \
  --vpc-id $VPC_ID \
  --tag-specifications "ResourceType=security-group,Tags=[{Key=Name,Value=choiros-ssh-sg}]" \
  --query "GroupId" \
  --output text)

aws ec2 authorize-security-group-ingress \
  --group-id $SSH_SG \
  --protocol tcp \
  --port 22 \
  --cidr YOUR_IP_ADDRESS/32

echo "Security groups created:"
echo "Web SG: $WEB_SG"
echo "Backend SG: $BACKEND_SG"
echo "SSH SG: $SSH_SG"
```

#### IAM Roles

```bash
#!/bin/bash
# setup-iam-role.sh

# Create trust policy
cat > trust-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": {
        "Service": "ec2.amazonaws.com"
      },
      "Action": "sts:AssumeRole"
    }
  ]
}
EOF

# Create IAM role
aws iam create-role \
  --role-name choiros-ec2-role \
  --assume-role-policy-document file://trust-policy.json

# Attach policy for S3 access (logs)
aws iam attach-role-policy \
  --role-name choiros-ec2-role \
  --policy-arn arn:aws:iam::aws:policy/AmazonS3FullAccess

# Attach policy for CloudWatch (metrics)
aws iam attach-role-policy \
  --role-name choiros-ec2-role \
  --policy-arn arn:aws:iam::aws:policy/CloudWatchAgentServerPolicy

# Attach custom policy for Secrets Manager
cat > secrets-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "secretsmanager:GetSecretValue",
        "secretsmanager:DescribeSecret"
      ],
      "Resource": "arn:aws:secretsmanager:*:*:secret:choiros/*"
    }
  ]
}
EOF

aws iam put-role-policy \
  --role-name choiros-ec2-role \
  --policy-name choiros-secrets-policy \
  --policy-document file://secrets-policy.json

# Create instance profile
aws iam create-instance-profile \
  --instance-profile-name choiros-ec2-profile

aws iam add-role-to-instance-profile \
  --instance-profile-name choiros-ec2-profile \
  --role-name choiros-ec2-role

echo "IAM role created: choiros-ec2-role"
echo "Instance profile created: choiros-ec2-profile"
```

#### Hardening Script

```bash
#!/bin/bash
# harden-ec2.sh

# Disable password authentication
sudo sed -i 's/#PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/sshd_config
sudo systemctl restart sshd

# Disable root login
sudo sed -i 's/#PermitRootLogin yes/PermitRootLogin no/' /etc/ssh/sshd_config
sudo systemctl restart sshd

# Install fail2ban
sudo apt-get update
sudo apt-get install -y fail2ban

sudo tee /etc/fail2ban/jail.local > /dev/null <<EOF
[DEFAULT]
bantime = 3600
findtime = 600
maxretry = 5

[sshd]
enabled = true
port = ssh
filter = sshd
logpath = /var/log/auth.log
maxretry = 3
EOF

sudo systemctl enable fail2ban
sudo systemctl start fail2ban

# Setup automatic security updates
sudo apt-get install -y unattended-upgrades

sudo tee /etc/apt/apt.conf.d/50unattended-upgrades > /dev/null <<EOF
Unattended-Upgrade::Allowed-Origins {
    "\${distro_id}:\${distro_codename}";
    "\${distro_id}:\${distro_codename}-security";
};
Unattended-Upgrade::AutoFixInterruptedDpkg "true";
Unattended-Upgrade::Remove-Unused-Dependencies "true";
Unattended-Upgrade::Automatic-Reboot "false";
EOF

sudo tee /etc/apt/apt.conf.d/20auto-upgrades > /dev/null <<EOF
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Download-Upgradeable-Packages "1";
APT::Periodic::AutocleanInterval "7";
APT::Periodic::Unattended-Upgrade "1";
EOF

# Setup UFW firewall
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow ssh
sudo ufw allow http
sudo ufw allow https
sudo ufw --force enable

# Install and configure AIDE (file integrity monitoring)
sudo apt-get install -y aide
sudo aide --init
sudo mv /var/lib/aide/aide.db.new /var/lib/aide/aide.db

# Create cron job for daily AIDE checks
(crontab -l 2>/dev/null; echo "0 5 * * * /usr/bin/aide --check | /usr/bin/mail -s 'AIDE Report' admin@example.com") | crontab -

echo "Server hardening complete"
```

#### Secrets Management

```rust
// src/secrets.rs
use aws_config::BehaviorVersion;
use aws_sdk_secretsmanager::{Client, Error};

pub async fn get_secret(secret_name: &str) -> Result<String, Error> {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Client::new(&config);

    let response = client.get_secret_value()
        .secret_id(secret_name)
        .send()
        .await?;

    let secret = response.secret_string().unwrap_or("");
    Ok(secret.to_string())
}

// Usage:
// let db_url = get_secret("choiros/database_url").await?;
```

**Secrets Manager Setup:**
```bash
#!/bin/bash
# store-secrets.sh

# Store database URL
aws secretsmanager create-secret \
  --name choiros/database_url \
  --secret-string "file:///data/events.db"

# Store API keys
aws secretsmanager create-secret \
  --name choiros/bedrock_api_key \
  --secret-string "your-api-key-here"

# Store CORS origins
aws secretsmanager create-secret \
  --name choiros/cors_origins \
  --secret-string '["https://choiros.com","https://app.choiros.com"]'
```

### 6.3 Security Monitoring

#### CloudTrail (Audit Logs)

```bash
#!/bin/bash
# setup-cloudtrail.sh

# Create S3 bucket for logs
aws s3 mb s3://choiros-cloudtrail-logs

# Enable CloudTrail
aws cloudtrail create-trail \
  --name choiros-trail \
  --s3-bucket-name choiros-cloudtrail-logs \
  --include-global-service-events \
  --is-multi-region-trail

# Enable logging
aws cloudtrail start-logging --name choiros-trail

echo "CloudTrail enabled"
```

#### Security Hub

```bash
#!/bin/bash
# setup-security-hub.sh

# Enable Security Hub
aws securityhub enable-security-hub

# Subscribe to CIS AWS Foundations Benchmark
aws securityhub subscribe \
  --product-arn arn:aws:securityhub:us-east-1::product/cis-aws-foundations-benchmark/cis-aws-foundations-benchmark

echo "Security Hub enabled"
```

#### GuardDuty

```bash
#!/bin/bash
# setup-guardduty.sh

# Enable GuardDuty
aws guardduty create-detector \
  --enable

echo "GuardDuty enabled"
```

### 6.4 Compliance Checklist

**SOC 2 Type II:**
- [ ] Access control (multi-factor authentication)
- [ ] Change management (git workflow, approval processes)
- [ ] System monitoring (CloudTrail, CloudWatch)
- [ ] Incident response (playbook, testing)
- [ ] Risk assessment (regular security reviews)
- [ ] Vendor management (third-party risk assessment)

**HIPAA (if applicable):**
- [ ] Encryption at rest (EBS, S3)
- [ ] Encryption in transit (TLS 1.2+)
- [ ] Access logging (CloudTrail, audit logs)
- [ ] Business associate agreements (BAAs)
- [ ] Risk analysis (annual)
- [ ] Contingency plan (backup and recovery)

**PCI DSS (if applicable):**
- [ ] Firewall configuration
- [ ] Default passwords removed
- [ ] Data encryption
- [ ] Regular vulnerability scanning
- [ ] Secure coding practices
- [ ] Incident response plan

---

## 7. Deployment Architecture Diagrams

### 7.1 Current Architecture (systemd)

```
                              ┌─────────────────────────────────┐
                              │         CloudFlare DNS           │
                              └──────────────┬──────────────────┘
                                             │
                                             │ DNS
                                             ▼
                              ┌─────────────────────────────────┐
                              │   EC2 Instance (Ubuntu 22.04)   │
                              │  ┌─────────────────────────────┐│
                              │  │      Caddy (port 80/443)    ││
                              │  │      Reverse Proxy          ││
                              │  └──────────────┬──────────────┘│
                              │                 │               │
                              │     ┌───────────┴───────────┐  │
                              │     │                       │  │
                              │     ▼                       ▼  │
                              │  ┌─────────┐          ┌─────────┐│
                              │  │Backend  │          │Frontend ││
                              │  │ :8080   │          │ :5173   ││
                              │  └─────────┘          └─────────┘│
                              │     │                       │    │
                              │     └───────────┬───────────┘    │
                              │                 │                │
                              │                 ▼                │
                              │         ┌──────────────┐        │
                              │         │  SQLite DB   │        │
                              │         │  /data/events │        │
                              │         └──────────────┘        │
                              └─────────────────────────────────┘
```

### 7.2 Recommended Architecture (Blue/Green)

```
                              ┌─────────────────────────────────┐
                              │         CloudFlare DNS           │
                              └──────────────┬──────────────────┘
                                             │
                                             │ DNS
                                             ▼
                              ┌─────────────────────────────────┐
                              │   Application Load Balancer     │
                              └──────────────┬──────────────────┘
                                             │
                            ┌────────────────┼────────────────┐
                            │                │                │
                            ▼                ▼                ▼
                   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
                   │ EC2 - Blue   │  │ EC2 - Green  │  │   Standby    │
                   │  (Active)    │  │  (Inactive)  │              │
                   │  v2.0        │  │  v2.1        │              │
                   └──────────────┘  └──────────────┘  └──────────────┘
```

### 7.3 Production Architecture (Auto Scaling)

```
                              ┌─────────────────────────────────┐
                              │         CloudFlare DNS           │
                              └──────────────┬──────────────────┘
                                             │
                                             │ DNS
                                             ▼
                              ┌─────────────────────────────────┐
                              │   Application Load Balancer     │
                              └──────────────┬──────────────────┘
                                             │
                            ┌────────────────┼────────────────┐
                            │                │                │
                            ▼                ▼                ▼
                   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
                   │   EC2 - 1    │  │   EC2 - 2    │  │   EC2 - 3    │
                   │  c5.large    │  │  c5.large    │  │  (Autoscale) │
                   │   v2.0       │  │   v2.0       │  │              │
                   └──────────────┘  └──────────────┘  └──────────────┘
                            │                │                │
                            └────────────────┼────────────────┘
                                             │
                                             ▼
                              ┌─────────────────────────────────┐
                              │    Amazon RDS / Aurora          │
                              │    PostgreSQL / MySQL           │
                              └─────────────────────────────────┘

                              ┌─────────────────────────────────┐
                              │    S3 (Static Assets, Backups) │
                              └─────────────────────────────────┘

                              ┌─────────────────────────────────┐
                              │    CloudWatch (Metrics)        │
                              │    CloudTrail (Audit Logs)    │
                              └─────────────────────────────────┘
```

### 7.4 Monitoring Architecture (Loki + Grafana)

```
                              ┌─────────────────────────────────┐
                              │         CloudFlare DNS           │
                              └─────────────────────────────────┘
                                             │
                                             ▼
                              ┌─────────────────────────────────┐
                              │   EC2 Instance (Ubuntu 22.04)   │
                              │                                 │
                              │  ┌─────────────────────────────┐│
                              │  │  systemd (journald)          ││
                              │  │  └─ choiros-backend.service ││
                              │  │  └─ choiros-frontend.service││
                              │  │  └─ caddy.service           ││
                              │  └──────────────┬──────────────┘│
                              │                 │               │
                              │                 ▼               │
                              │         ┌──────────────┐        │
                              │         │  Promtail    │        │
                              │         │  (Log Shipper)│        │
                              │         └──────┬───────┘        │
                              └────────────────┼────────────────┘
                                               │
                                               │ Loki API
                                               ▼
                              ┌─────────────────────────────────┐
                              │            Loki                │
                              │        (Log Aggregation)        │
                              └──────────────┬──────────────────┘
                                             │
                                             │ Query API
                                             ▼
                              ┌─────────────────────────────────┐
                              │           Grafana               │
                              │        (Dashboard UI)           │
                              └─────────────────────────────────┘
```

---

## 8. Cost Estimates

### 8.1 Deployment Options Comparison

| Option | Monthly Cost | Setup Complexity | Downtime | Rollback Time |
|--------|--------------|------------------|----------|---------------|
| **Current (systemd)** | $73 | Low | 10-30s | Manual |
| **Two-Binary Pattern** | $73 | Low | <5s | <10s |
| **Blue/Green (Single EC2)** | $73 | Medium | <1s | <5s |
| **Blue/Green (2 EC2s + ALB)** | $166 | High | 0s | <1s |
| **Auto Scaling (3 EC2s + ALB)** | $240 | High | 0s | <1s |

### 8.2 Infrastructure Costs

**Development:**
- t3.nano (9 AM - 10 PM, 5 days/week): $3.03/month
- EIP (optional): $3.60/month
- CloudWatch (basic): Free tier
- **Total: ~$6-7/month**

**Staging:**
- t3.small (auto scale to zero): $5-12/month
- EIP (optional): $3.60/month
- S3 (logs): $0.23/month
- **Total: ~$9-16/month**

**Production:**
- c5.large (reserved): $56.67/month
- ALB: $20/month
- RDS (db.t3.small): $15/month
- S3 (backups): $0.46/month
- CloudWatch (metrics): Free tier
- **Total: ~$92/month**

**Total Monthly Cost:** ~$107-115/month

### 8.3 Log Aggregation Costs

| Solution | Monthly Cost (10GB) | Monthly Cost (100GB) |
|----------|---------------------|----------------------|
| **CloudWatch Logs** | $5.30 | $53.00 |
| **Loki + Grafana** | $0 | $0 |
| **Vector + S3** | $0.23 | $2.30 |
| **ELK Stack** | $0 | $0 |

**Recommendation:** Loki + Grafana (no additional cost)

### 8.4 Security Services Costs

| Service | Monthly Cost |
|---------|--------------|
| **GuardDuty** | $0.30/million events |
| **Security Hub** | Free |
| **CloudTrail** | Free (S3 storage only) |
| **Secrets Manager** | $0.40/secret/month + $0.05/10K API calls |
| **Total:** ~$1-2/month |

---

## 9. Recommendations

### 9.1 Short-Term (Next 1-3 months)

1. **Implement Two-Binary Rollback Pattern**
   - No additional cost
   - Fast rollback (<10s)
   - Easy to implement

2. **Add Health Check Endpoint**
   - `/health` endpoint with comprehensive checks
   - systemd health monitoring
   - PagerDuty/Slack alerts

3. **Setup Loki + Grafana for Logs**
   - Replace or supplement journald
   - Cost-effective alternative to CloudWatch
   - Better query capabilities

4. **Security Hardening**
   - VPC isolation (separate dev/staging/prod)
   - Security group whitelisting
   - IAM roles (no access keys)
   - Enable CloudTrail and GuardDuty

### 9.2 Medium-Term (3-6 months)

1. **Implement Blue/Green Deployment**
   - Single EC2 with dual services
   - No additional cost
   - True zero-downtime

2. **Cost Optimization**
   - Schedule dev instances (9 AM - 10 PM)
   - Auto scaling for staging
   - Reserved instances for production

3. **Move to Production RDS**
   - Replace SQLite with PostgreSQL
   - Automated backups
   - Better performance and reliability

4. **Enhanced Monitoring**
   - Prometheus + AlertManager
   - Grafana dashboards
   - Uptime monitoring

### 9.3 Long-Term (6-12 months)

1. **Containerization**
   - Docker images for all services
   - Build in CI/CD pipeline
   - Store in ECR or GHCR

2. **Multi-AZ Deployment**
   - Multiple availability zones
   - Higher availability
   - Better disaster recovery

3. **Auto Scaling Group**
   - 2-4 instances
   - Scale based on CPU/memory
   - Handle traffic spikes

4. **Advanced Security**
   - AWS WAF for DDoS protection
   - AWS Shield Standard
   - Certificate Manager for TLS

---

## 10. Next Steps

### 10.1 Immediate Actions

1. **Review Current Deployment**
   ```bash
   ssh ubuntu@3.83.131.245
   sudo systemctl status choiros-backend
   sudo systemctl status choiros-frontend
   sudo systemctl status caddy
   ```

2. **Implement Two-Binary Rollback**
   - Copy script from section 1.2
   - Test rollback functionality
   - Add to CI/CD pipeline

3. **Add Health Check Endpoint**
   - Implement `/health` endpoint
   - Add systemd health monitoring
   - Setup alerts

4. **Security Audit**
   - Review security groups
   - Enable CloudTrail
   - Setup GuardDuty

### 10.2 Implementation Timeline

**Week 1-2:**
- [ ] Implement two-binary rollback
- [ ] Add health check endpoint
- [ ] Setup basic monitoring (Loki + Grafana)

**Week 3-4:**
- [ ] Security hardening
- [ ] Setup CloudTrail and GuardDuty
- [ ] Implement automated backups

**Month 2:**
- [ ] Blue/green deployment
- [ ] Cost optimization (scheduled instances)
- [ ] Move to RDS

**Month 3-6:**
- [ ] Containerization
- [ ] Multi-AZ deployment
- [ ] Auto scaling

---

## Appendix A: Example Configurations

### systemd Service Files

```ini
# /etc/systemd/system/choiros-backend.service
[Unit]
Description=ChoirOS Backend API
After=network.target
Wants=network.target

[Service]
Type=simple
User=choiros
Group=choiros
WorkingDirectory=/opt/choiros
ExecStart=/opt/choiros/bin/sandbox
Environment="RUST_LOG=info"
Environment="DATABASE_URL=/opt/choiros/data/events.db"
Restart=always
RestartSec=10
StartLimitInterval=60
StartLimitBurst=5
LimitNOFILE=65536
MemoryMax=2G
CPUQuota=200%
StandardOutput=journal
StandardError=journal
SyslogIdentifier=choiros-backend

[Install]
WantedBy=multi-user.target
```

### Caddyfile

```
choiros.com {
    reverse_proxy localhost:8080
    encode gzip
    log {
        output file /var/log/caddy/choiros-access.log
        format json
    }

    tls {
        dns cloudflare YOUR_API_TOKEN
    }
}

app.choiros.com {
    reverse_proxy localhost:5173
    encode gzip
    log {
        output file /var/log/caddy/app-choiros-access.log
        format json
    }

    tls {
        dns cloudflare YOUR_API_TOKEN
    }
}
```

### Prometheus Configuration

```yaml
# prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'choiros-backend'
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: '/metrics'

  - job_name: 'node-exporter'
    static_configs:
      - targets: ['localhost:9100']

  - job_name: 'caddy'
    static_configs:
      - targets: ['localhost:2019']
```

---

## Appendix B: Troubleshooting

### Common Issues

**Issue: Service fails to start**
```bash
# Check service status
sudo systemctl status choiros-backend

# View logs
sudo journalctl -u choiros-backend -n 100

# Check for port conflicts
sudo netstat -tlnp | grep 8080
```

**Issue: Health check failing**
```bash
# Test health endpoint manually
curl http://localhost:8080/health

# Check database connectivity
sqlite3 /opt/choiros/data/events.db "SELECT 1;"

# Check resource usage
top -bn1 | head -20
free -h
df -h
```

**Issue: Deployment rollback needed**
```bash
# List backups
ls -lt /opt/choiros/backups/

# Restore specific backup
cp /opt/choiros/backups/sandbox-20240201-120000 /opt/choiros/bin/sandbox
sudo systemctl restart choiros-backend
```

**Issue: Logs not appearing in Grafana**
```bash
# Check Promtail status
sudo systemctl status promtail

# Check Promtail logs
sudo journalctl -u promtail -n 100

# Test Loki connection
curl http://localhost:3100/ready
```

---

## Appendix C: References

### AWS Documentation
- [EC2 Instance Types](https://aws.amazon.com/ec2/instance-types/)
- [Security Groups](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ec2-security-groups.html)
- [IAM Best Practices](https://docs.aws.amazon.com/IAM/latest/UserGuide/best-practices.html)
- [Cost Explorer](https://docs.aws.amazon.com/awsaccountbilling/latest/aboutv2/ce-what-is.html)

### Rust Deployment
- [Systemd Service Configuration](https://www.freedesktop.org/software/systemd/man/systemd.service.html)
- [Rocket Deployment](https://rocket.rs/v0.4/guide/overview/#deployment)
- [Actix Web Deployment](https://actix.rs/docs/server/)

### Monitoring & Logging
- [Loki Documentation](https://grafana.com/docs/loki/latest/)
- [Prometheus Best Practices](https://prometheus.io/docs/practices/)
- [Grafana Dashboards](https://grafana.com/grafana/dashboards/)

### Security
- [CIS AWS Foundations Benchmark](https://www.cisecurity.org/benchmark/amazon_web_services)
- [AWS Security Best Practices](https://docs.aws.amazon.com/whitepapers/latest/security-best-practices-for-aws-accounts-workloads/welcome.html)
- [Linux Security Hardening](https://linux-audit.com/linux-system-hardening-checklist/)

---

**Document Version:** 1.0
**Last Updated:** 2026-02-01
**Maintained By:** ChoirOS Team
**Next Review:** 2026-05-01
