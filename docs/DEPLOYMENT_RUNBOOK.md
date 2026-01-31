# ChoirOS Deployment Runbook

**Version:** 1.0  
**Date:** 2026-01-31  
**Target:** Ubuntu 22.04 LTS on AWS c5.large  
**Domain:** TBD (configure below)

---

## Quick Start

```bash
# 1. Provision server (c5.large, Ubuntu 22.04 LTS)
# 2. SSH in with root or ubuntu user
# 3. Run setup script
wget https://raw.githubusercontent.com/yourusername/choiros-rs/main/scripts/deploy-server.sh
chmod +x deploy-server.sh
sudo ./deploy-server.sh

# 4. Deploy the app
./deploy.sh
```

---

## Table of Contents

1. [Pre-Deployment Checklist](#pre-deployment-checklist)
2. [Server Provisioning](#server-provisioning)
3. [Security Hardening](#security-hardening)
4. [Application Setup](#application-setup)
5. [CI/CD Configuration](#cicd-configuration)
6. [First Deployment](#first-deployment)
7. [Monitoring & Maintenance](#monitoring--maintenance)
8. [Rollback Procedures](#rollback-procedures)
9. [Troubleshooting](#troubleshooting)

---

## Pre-Deployment Checklist

Before you begin, ensure you have:

- [ ] **Domain name** registered (or plan to use IP initially)
- [ ] **AWS account** with IAM permissions for EC2
- [ ] **GitHub repository** access (admin to add secrets)
- [ ] **SSH key pair** generated for server access
- [ ] **GitHub SSH key** added to server (for pulling code)

### Required Secrets

Add these to GitHub Settings > Secrets and Variables > Actions:

```
SSH_HOST          # Server IP or domain (e.g., 52.XX.XX.XX or choiros.example.com)
SSH_USER          # Server user (e.g., ubuntu)
SSH_KEY           # Private SSH key contents (full key, including BEGIN/END)
DEPLOY_PATH       # Where to deploy (e.g., /opt/choiros)
```

---

## Server Provisioning

### Step 1: Launch EC2 Instance

**AWS Console:**
1. Go to EC2 Dashboard → Instances → Launch Instance
2. **Name:** choiros-production
3. **AMI:** Ubuntu Server 22.04 LTS (HVM), SSD Volume Type
4. **Instance type:** c5.large (2 vCPU, 4GB RAM)
5. **Key pair:** Select or create new (download .pem file)
6. **Network settings:**
   - VPC: Default or your VPC
   - Subnet: Public subnet
   - Auto-assign public IP: Enable
   - Security group: Create new
     - SSH (22): My IP only (initially)
     - HTTP (80): Anywhere
     - HTTPS (443): Anywhere
7. **Storage:** 20GB gp3 (default is fine)
8. **Advanced:**
   - User data: Leave blank (we'll script everything)
9. **Launch instance**

### Step 2: Connect to Server

```bash
# Set proper permissions on key
chmod 400 ~/Downloads/choiros-key.pem

# SSH into server (replace with your IP)
ssh -i ~/Downloads/choiros-key.pem ubuntu@YOUR_SERVER_IP

# Verify connection
echo "Connected to $(hostname)"
```

---

## Security Hardening

### Step 3: Initial Security Setup

Run these commands on the server:

```bash
#!/bin/bash
# Run as root or with sudo

# Update system
sudo apt update && sudo apt upgrade -y

# Install security tools
sudo apt install -y fail2ban ufw unattended-upgrades apt-listchanges

# Create app user (non-root)
sudo useradd -r -m -s /bin/bash choiros
sudo usermod -aG sudo choiros

# Set up SSH key for choiros user
sudo mkdir -p /home/choiros/.ssh
sudo cp /home/ubuntu/.ssh/authorized_keys /home/choiros/.ssh/
sudo chown -R choiros:choiros /home/choiros/.ssh
sudo chmod 700 /home/choiros/.ssh
sudo chmod 600 /home/choiros/.ssh/authorized_keys

# Configure UFW firewall
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow 22/tcp comment 'SSH'
sudo ufw allow 80/tcp comment 'HTTP'
sudo ufw allow 443/tcp comment 'HTTPS'
sudo ufw --force enable

# Configure fail2ban
cat << 'EOF' | sudo tee /etc/fail2ban/jail.local
[DEFAULT]
bantime = 3600
findtime = 600
maxretry = 3

[sshd]
enabled = true
port = ssh
filter = sshd
logpath = /var/log/auth.log
maxretry = 3
EOF

sudo systemctl restart fail2ban
sudo systemctl enable fail2ban

# Configure auto-updates
cat << 'EOF' | sudo tee /etc/apt/apt.conf.d/50unattended-upgrades
Unattended-Upgrade::Allowed-Origins {
    "${distro_id}:${distro_codename}-security";
};
Unattended-Upgrade::AutoFixInterruptedDpkg "true";
Unattended-Upgrade::MinimalSteps "true";
Unattended-Upgrade::InstallOnShutdown "false";
Unattended-Upgrade::Remove-Unused-Dependencies "true";
Unattended-Upgrade::Remove-New-Unused-Dependencies "true";
Unattended-Upgrade::Automatic-Reboot "false";
EOF

sudo systemctl restart unattended-upgrades
sudo systemctl enable unattended-upgrades

# Harden SSH (optional - be careful!)
# sudo sed -i 's/#PermitRootLogin yes/PermitRootLogin no/' /etc/ssh/sshd_config
# sudo sed -i 's/#PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/sshd_config
# sudo systemctl restart sshd

echo "✅ Security hardening complete"
```

### Step 4: Install Dependencies

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env

# Install Node.js (for dev-browser if needed)
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install -y nodejs

# Install build dependencies
sudo apt install -y build-essential pkg-config libssl-dev git

# Install Caddy (reverse proxy with auto HTTPS)
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update
sudo apt install caddy

echo "✅ Dependencies installed"
```

---

## Application Setup

### Step 5: Setup Application Directory

```bash
# As choiros user
sudo su - choiros

# Create directory structure
mkdir -p /opt/choiros
cd /opt/choiros

# Clone repository
git clone https://github.com/YOUR_USERNAME/choiros-rs.git .

# Create data directory
mkdir -p data
mkdir -p logs

# Set permissions
chmod 755 /opt/choiros
chmod 755 data
chmod 755 logs

echo "✅ Application directory ready"
```

### Step 6: Install Dioxus CLI

```bash
# As choiros user
cargo install dioxus-cli

echo "✅ Dioxus CLI installed"
```

### Step 7: Configure Caddy (Reverse Proxy)

```bash
# As root or with sudo
cat << 'EOF' | sudo tee /etc/caddy/Caddyfile
# Global options
{
    auto_https off  # Change to 'on' when you have a domain
    admin off
}

# Your domain or IP
:80 {
    # Health check endpoint
    handle /health* {
        reverse_proxy localhost:8080
    }

    # Backend API
    handle /api/* {
        reverse_proxy localhost:8080
    }

    handle /chat/* {
        reverse_proxy localhost:8080
    }
    
    handle /desktop/* {
        reverse_proxy localhost:8080
    }

    # Static files (if serving directly)
    # handle_path /static/* {
    #     root * /opt/choiros/sandbox-ui/dist/static
    #     file_server
    # }

    # Frontend (dev server or built files)
    reverse_proxy localhost:5173

    # Logging
    log {
        output file /opt/choiros/logs/caddy.log {
            roll_size 10MB
            roll_keep 5
        }
    }
}

# When you have a domain, replace :80 with your domain:
# yourdomain.com {
#     tls your-email@example.com
#     ... rest of config
# }
EOF

# Start Caddy
sudo systemctl restart caddy
sudo systemctl enable caddy

echo "✅ Caddy configured"
```

### Step 8: Create Systemd Services

**Backend Service:**

```bash
sudo tee /etc/systemd/system/choiros-backend.service << 'EOF'
[Unit]
Description=ChoirOS Backend API
After=network.target

[Service]
Type=simple
User=choiros
Group=choiros
WorkingDirectory=/opt/choiros
Environment=RUST_LOG=info
Environment=DATABASE_URL=/opt/choiros/data/events.db
Environment=RUST_BACKTRACE=1
ExecStart=/opt/choiros/target/release/sandbox
Restart=on-failure
RestartSec=5
StandardOutput=append:/opt/choiros/logs/backend.log
StandardError=append:/opt/choiros/logs/backend-error.log

[Install]
WantedBy=multi-user.target
EOF
```

**Frontend Service (Dev Mode):**

```bash
sudo tee /etc/systemd/system/choiros-frontend.service << 'EOF'
[Unit]
Description=ChoirOS Frontend UI
After=network.target choiros-backend.service
Wants=choiros-backend.service

[Service]
Type=simple
User=choiros
Group=choiros
WorkingDirectory=/opt/choiros/sandbox-ui
Environment=PATH=/home/choiros/.cargo/bin:/usr/local/bin:/usr/bin:/bin
ExecStart=/home/choiros/.cargo/bin/dx serve
Restart=on-failure
RestartSec=5
StandardOutput=append:/opt/choiros/logs/frontend.log
StandardError=append:/opt/choiros/logs/frontend-error.log

[Install]
WantedBy=multi-user.target
EOF
```

**Enable and start services:**

```bash
sudo systemctl daemon-reload
sudo systemctl enable choiros-backend
sudo systemctl enable choiros-frontend

echo "✅ Systemd services created"
```

---

## CI/CD Configuration

### Step 9: Update GitHub Actions Workflow

**File:** `.github/workflows/ci.yml` (update existing)

```yaml
name: CI/CD

on:
  push:
    branches: [ main ]
  pull_request:
    branches: [ main ]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  backend-tests:
    name: Backend Tests
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - uses: actions/cache@v3
      with:
        path: |
          ~/.cargo/registry
          ~/.cargo/git
          target
        key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
    - name: Build backend
      run: cargo build -p sandbox --verbose
    - name: Run tests
      run: cargo test -p sandbox --verbose

  frontend-build:
    name: Frontend Build
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - run: cargo install dioxus-cli
    - uses: actions/cache@v3
      with:
        path: |
          ~/.cargo/registry
          ~/.cargo/git
          target
        key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
    - name: Build frontend
      run: |
        cd sandbox-ui
        cargo build --verbose

  deploy:
    name: Deploy to Production
    needs: [backend-tests, frontend-build]
    runs-on: ubuntu-latest
    if: github.ref == 'refs/heads/main'
    steps:
    - name: Deploy to server
      uses: appleboy/ssh-action@master
      with:
        host: ${{ secrets.SSH_HOST }}
        username: ${{ secrets.SSH_USER }}
        key: ${{ secrets.SSH_KEY }}
        script: |
          cd ${{ secrets.DEPLOY_PATH }}
          
          # Pull latest code
          git fetch origin main
          git reset --hard origin/main
          
          # Run deployment
          ./scripts/deploy.sh
```

### Step 10: Create Deploy Script

**File:** `scripts/deploy.sh` (create in repo)

```bash
#!/bin/bash
set -e

echo "🚀 Starting deployment..."

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Build backend
echo "Building backend..."
cargo build -p sandbox --release

# Build frontend
echo "Building frontend..."
cd sandbox-ui
dx build --release
cd ..

# Check if services exist
if ! systemctl is-active --quiet choiros-backend; then
    echo "Backend service not running, starting..."
    sudo systemctl start choiros-backend
else
    echo "Restarting backend..."
    sudo systemctl restart choiros-backend
fi

if ! systemctl is-active --quiet choiros-frontend; then
    echo "Frontend service not running, starting..."
    sudo systemctl start choiros-frontend
else
    echo "Restarting frontend..."
    sudo systemctl restart choiros-frontend
fi

# Wait for services to start
sleep 5

# Health checks
echo "Running health checks..."

if curl -s http://localhost:8080/health > /dev/null; then
    echo -e "${GREEN}✅ Backend is healthy${NC}"
else
    echo -e "${RED}❌ Backend health check failed${NC}"
    exit 1
fi

if curl -s http://localhost:5173 > /dev/null; then
    echo -e "${GREEN}✅ Frontend is responding${NC}"
else
    echo -e "${RED}❌ Frontend health check failed${NC}"
    exit 1
fi

# Check Caddy
if systemctl is-active --quiet caddy; then
    echo -e "${GREEN}✅ Caddy reverse proxy is running${NC}"
else
    echo "Starting Caddy..."
    sudo systemctl start caddy
fi

echo ""
echo -e "${GREEN}🎉 Deployment successful!${NC}"
echo "App available at: http://$(curl -s ifconfig.me)"
echo ""
echo "Service status:"
sudo systemctl status --no-pager choiros-backend | grep "Active:"
sudo systemctl status --no-pager choiros-frontend | grep "Active:"
sudo systemctl status --no-pager caddy | grep "Active:"
```

**Make it executable:**

```bash
chmod +x scripts/deploy.sh
git add scripts/deploy.sh
git commit -m "Add deployment script"
git push
```

---

## First Deployment

### Step 11: Manual First Deploy

```bash
# As choiros user on server
cd /opt/choiros

# Initial build (will take a while)
cargo build -p sandbox --release

cd sandbox-ui
dx build --release
cd ..

# Start services
sudo systemctl start choiros-backend
sudo systemctl start choiros-frontend
sudo systemctl start caddy

# Verify everything is running
echo "Services status:"
sudo systemctl status choiros-backend --no-pager
sudo systemctl status choiros-frontend --no-pager
sudo systemctl status caddy --no-pager

# Test endpoints
echo ""
echo "Testing endpoints..."
curl http://localhost:8080/health
echo ""
curl -I http://localhost:5173
echo ""

# Check public access
echo ""
echo "Public access test:"
curl -I http://$(curl -s ifconfig.me)
```

### Step 12: Verify Deployment

**Checklist:**

- [ ] Backend health endpoint responds: `curl http://YOUR_IP/health`
- [ ] Frontend loads: Open browser to `http://YOUR_IP`
- [ ] Desktop UI appears
- [ ] Can open/close windows
- [ ] Logs are writing to `/opt/choiros/logs/`
- [ ] Services restart automatically if they crash

---

## Monitoring & Maintenance

### View Logs

```bash
# Backend logs
sudo tail -f /opt/choiros/logs/backend.log

# Frontend logs
sudo tail -f /opt/choiros/logs/frontend.log

# Caddy logs
sudo tail -f /opt/choiros/logs/caddy.log

# All logs
sudo tail -f /opt/choiros/logs/*.log
```

### Check Service Status

```bash
# Check all services
sudo systemctl status choiros-backend
sudo systemctl status choiros-frontend
sudo systemctl status caddy

# Quick status check
sudo systemctl is-active choiros-backend choiros-frontend caddy
```

### Restart Services

```bash
# Restart individual services
sudo systemctl restart choiros-backend
sudo systemctl restart choiros-frontend
sudo systemctl restart caddy

# Restart all
sudo systemctl restart choiros-backend choiros-frontend caddy
```

### Update Application

```bash
# Pull latest code and deploy
cd /opt/choiros
git pull origin main
./scripts/deploy.sh
```

### Monitor Resources

```bash
# Check CPU/Memory
htop

# Check disk space
df -h

# Check load
uptime
```

---

## Rollback Procedures

### Quick Rollback

```bash
cd /opt/choiros

# Go to previous commit
git log --oneline -5  # See recent commits
git reset --hard HEAD~1  # Rollback one commit

# Redeploy
./scripts/deploy.sh
```

### Service Rollback

```bash
# If new deployment fails, restart with previous binary
sudo systemctl stop choiros-backend choiros-frontend

# Restore from backup if available
# cp /opt/choiros/backup/sandbox /opt/choiros/target/release/sandbox

# Restart with old version
sudo systemctl start choiros-backend choiros-frontend
```

---

## Troubleshooting

### Backend Won't Start

```bash
# Check logs
sudo journalctl -u choiros-backend -n 50

# Check if port is in use
sudo lsof -i :8080

# Kill if necessary
sudo kill -9 $(sudo lsof -t -i:8080)

# Restart
sudo systemctl restart choiros-backend
```

### Frontend Won't Start

```bash
# Check logs
sudo journalctl -u choiros-frontend -n 50

# Check if dioxus is installed
which dx
dx --version

# Reinstall if needed
cargo install dioxus-cli --force
```

### Caddy Issues

```bash
# Validate config
sudo caddy validate --config /etc/caddy/Caddyfile

# Check Caddy logs
sudo journalctl -u caddy -n 50

# Restart Caddy
sudo systemctl restart caddy
```

### Permission Issues

```bash
# Fix ownership
sudo chown -R choiros:choiros /opt/choiros

# Fix permissions
chmod 755 /opt/choiros
cd /opt/choiros && chmod -R 755 data logs
```

### Database Issues

```bash
# Check database path
ls -la /opt/choiros/data/

# Fix permissions
sudo chown choiros:choiros /opt/choiros/data/events.db

# If corrupted, backup and delete (data will be lost!)
cp /opt/choiros/data/events.db /opt/choiros/data/events.db.backup.$(date +%Y%m%d)
sudo rm /opt/choiros/data/events.db
sudo systemctl restart choiros-backend
```

---

## Post-Deployment Tasks

### After First Deploy

1. **Test manually:**
   - Open browser to server IP
   - Click chat icon, open window
   - Test mobile layout (dev tools)
   - Verify data persists after refresh

2. **Set up domain (optional):**
   - Point DNS A record to server IP
   - Update Caddyfile with domain
   - Enable auto HTTPS

3. **Set up monitoring (optional):**
   - Install Uptime Kuma or similar
   - Monitor health endpoint
   - Set up alerts

4. **Configure backups (optional):**
   - Backup database daily
   - Backup logs rotation
   - Document restore process

### Next Feature Priorities

After deployment is stable:

1. **Real AI integration** (replace echo/mock)
2. **Authentication** (GitHub OAuth or similar)
3. **Chat message persistence** in UI
4. **App registry** for multiple app types

---

## Emergency Contacts & Resources

- **Server IP:** YOUR_SERVER_IP
- **Domain:** YOUR_DOMAIN (or update when configured)
- **Repository:** https://github.com/YOUR_USERNAME/choiros-rs
- **Logs location:** `/opt/choiros/logs/`
- **Data location:** `/opt/choiros/data/`

### Useful Commands Reference

```bash
# Quick health check
curl http://localhost:8080/health && echo "✅ Backend OK"
curl -I http://localhost:5173 2>/dev/null | head -1 && echo "✅ Frontend OK"

# Restart everything
sudo systemctl restart choiros-backend choiros-frontend caddy

# View recent errors
sudo journalctl -u choiros-backend --since "1 hour ago" | grep -i error

# Disk cleanup (if needed)
sudo apt clean
sudo journalctl --vacuum-time=3d
```

---

## Appendix: Complete Setup Script

For automation, combine everything into one script:

**File:** `scripts/provision-server.sh`

```bash
#!/bin/bash
# Complete server setup script
# Run as root on fresh Ubuntu 22.04 server

set -e

# Configuration
APP_USER="choiros"
APP_DIR="/opt/choiros"
REPO_URL="https://github.com/YOUR_USERNAME/choiros-rs.git"

echo "🚀 ChoirOS Server Setup"
echo "======================="

# Update system
echo "Updating system..."
apt update && apt upgrade -y

# Install dependencies
echo "Installing dependencies..."
apt install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    git \
    curl \
    ufw \
    fail2ban \
    unattended-upgrades \
    apt-listchanges \
    debian-keyring \
    debian-archive-keyring \
    apt-transport-https

# Install Caddy
echo "Installing Caddy..."
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list
apt update
apt install -y caddy

# Create app user
echo "Creating app user..."
useradd -r -m -s /bin/bash $APP_USER

# Install Rust for app user
echo "Installing Rust..."
su - $APP_USER -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"

# Setup firewall
echo "Configuring firewall..."
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

# Configure fail2ban
echo "Configuring fail2ban..."
cat << 'EOF' | tee /etc/fail2ban/jail.local
[DEFAULT]
bantime = 3600
findtime = 600
maxretry = 3

[sshd]
enabled = true
port = ssh
filter = sshd
logpath = /var/log/auth.log
maxretry = 3
EOF
systemctl restart fail2ban
systemctl enable fail2ban

# Setup app directory
echo "Setting up app directory..."
mkdir -p $APP_DIR
chown $APP_USER:$APP_USER $APP_DIR

# Clone repo as app user
echo "Cloning repository..."
su - $APP_USER -c "cd $APP_DIR && git clone $REPO_URL ."

# Install Dioxus CLI
echo "Installing Dioxus CLI..."
su - $APP_USER -c "cargo install dioxus-cli"

# Create systemd services
echo "Creating systemd services..."
# ... (copy service files from above)

# Create deploy script
echo "Creating deploy script..."
# ... (copy deploy script)

# Enable services
systemctl daemon-reload
systemctl enable choiros-backend
systemctl enable choiros-frontend
systemctl enable caddy

echo ""
echo "✅ Setup complete!"
echo ""
echo "Next steps:"
echo "1. Update GitHub secrets with server details"
echo "2. Run initial build: cd $APP_DIR && ./scripts/deploy.sh"
echo "3. Verify deployment: curl http://$(curl -s ifconfig.me)/health"
echo ""
echo "App directory: $APP_DIR"
echo "App user: $APP_USER"
```

---

**END OF RUNBOOK**

Last updated: 2026-01-31
Next agent: Follow steps sequentially, verify each step before proceeding.
