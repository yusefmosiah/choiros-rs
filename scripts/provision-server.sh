#!/bin/bash
# Complete server setup script for ChoirOS
# Run as root on fresh Ubuntu 22.04 server

set -e

# Configuration
APP_USER="choiros"
APP_DIR="/opt/choiros"
REPO_URL="https://github.com/anomalyco/choiros-rs.git"  # Update this!

echo "🚀 ChoirOS Server Setup"
echo "======================="
echo ""
echo "This script will:"
echo "- Update system packages"
echo "- Install Rust, Node.js, and build tools"
echo "- Configure firewall (UFW)"
echo "- Install and configure fail2ban"
echo "- Install Caddy reverse proxy"
echo "- Create app user and directory"
echo "- Clone repository"
echo "- Create systemd services"
echo ""
read -p "Continue? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 1
fi

# Update system
echo ""
echo "📦 Updating system..."
apt update && apt upgrade -y

# Install dependencies
echo ""
echo "📦 Installing dependencies..."
apt install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    git \
    curl \
    wget \
    ufw \
    fail2ban \
    unattended-upgrades \
    apt-listchanges \
    debian-keyring \
    debian-archive-keyring \
    apt-transport-https \
    software-properties-common

# Install Caddy
echo ""
echo "🌐 Installing Caddy reverse proxy..."
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list
apt update
apt install -y caddy

# Create app user
echo ""
echo "👤 Creating app user: $APP_USER"
if id "$APP_USER" &>/dev/null; then
    echo "User $APP_USER already exists"
else
    useradd -r -m -s /bin/bash $APP_USER
    usermod -aG sudo $APP_USER
    echo "$APP_USER user created"
fi

# Install Rust for app user
echo ""
echo "🦀 Installing Rust..."
if ! su - $APP_USER -c "which cargo" &>/dev/null; then
    su - $APP_USER -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
    echo "Rust installed"
else
    echo "Rust already installed"
fi

# Setup firewall
echo ""
echo "🔥 Configuring firewall (UFW)..."
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp comment 'SSH'
ufw allow 80/tcp comment 'HTTP'
ufw allow 443/tcp comment 'HTTPS'
ufw --force enable
systemctl enable ufw
echo "Firewall configured"

# Configure fail2ban
echo ""
echo "🛡️  Configuring fail2ban..."
cat << 'EOF' | tee /etc/fail2ban/jail.local
[DEFAULT]
bantime = 3600
findtime = 600
maxretry = 3
backend = systemd

[sshd]
enabled = true
port = ssh
filter = sshd
logpath = /var/log/auth.log
maxretry = 3
EOF

systemctl restart fail2ban
systemctl enable fail2ban
echo "Fail2ban configured"

# Configure auto-updates
echo ""
echo "🔄 Configuring auto-updates..."
cat << 'EOF' | tee /etc/apt/apt.conf.d/50unattended-upgrades
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

systemctl restart unattended-upgrades
systemctl enable unattended-upgrades
echo "Auto-updates configured"

# Setup app directory
echo ""
echo "📁 Setting up app directory..."
mkdir -p $APP_DIR
mkdir -p $APP_DIR/data
mkdir -p $APP_DIR/logs
chown -R $APP_USER:$APP_USER $APP_DIR
chmod 755 $APP_DIR
chmod 755 $APP_DIR/data
chmod 755 $APP_DIR/logs
echo "App directory ready: $APP_DIR"

# Clone repository
echo ""
echo "📥 Cloning repository..."
if [ -d "$APP_DIR/.git" ]; then
    echo "Repository already exists, skipping clone"
else
    su - $APP_USER -c "cd $APP_DIR && git clone $REPO_URL ."
    echo "Repository cloned"
fi

# Install Dioxus CLI
echo ""
echo "⚙️  Installing Dioxus CLI..."
su - $APP_USER -c "export PATH=\"\$HOME/.cargo/bin:\$PATH\" && cargo install dioxus-cli 2>/dev/null || echo 'Dioxus CLI already installed'"
echo "Dioxus CLI ready"

# Create systemd services
echo ""
echo "🔧 Creating systemd services..."

# Backend service
cat << 'EOF' | tee /etc/systemd/system/choiros-backend.service
[Unit]
Description=ChoirOS Backend API
After=network.target

[Service]
Type=simple
User=choiros
Group=choiros
WorkingDirectory=/opt/choiros
Environment="PATH=/home/choiros/.cargo/bin:/usr/local/bin:/usr/bin:/bin"
Environment="RUST_LOG=info"
Environment="RUST_BACKTRACE=1"
ExecStart=/opt/choiros/target/release/sandbox
Restart=on-failure
RestartSec=5
StandardOutput=append:/opt/choiros/logs/backend.log
StandardError=append:/opt/choiros/logs/backend-error.log

[Install]
WantedBy=multi-user.target
EOF

# Frontend service
cat << 'EOF' | tee /etc/systemd/system/choiros-frontend.service
[Unit]
Description=ChoirOS Frontend UI
After=network.target choiros-backend.service
Wants=choiros-backend.service

[Service]
Type=simple
User=choiros
Group=choiros
WorkingDirectory=/opt/choiros/sandbox-ui
Environment="PATH=/home/choiros/.cargo/bin:/usr/local/bin:/usr/bin:/bin"
ExecStart=/home/choiros/.cargo/bin/dx serve
Restart=on-failure
RestartSec=5
StandardOutput=append:/opt/choiros/logs/frontend.log
StandardError=append:/opt/choiros/logs/frontend-error.log

[Install]
WantedBy=multi-user.target
EOF

# Enable services
systemctl daemon-reload
systemctl enable choiros-backend
systemctl enable choiros-frontend

echo "Systemd services created"

# Configure Caddy
echo ""
echo "🌐 Configuring Caddy..."
cat << 'EOF' | tee /etc/caddy/Caddyfile
# Global options
{
    auto_https off
    admin off
}

# Your server IP or domain
:80 {
    # Health check endpoint
    handle /health* {
        reverse_proxy localhost:8080
    }

    # Backend API routes
    handle /api/* {
        reverse_proxy localhost:8080
    }

    handle /chat/* {
        reverse_proxy localhost:8080
    }
    
    handle /desktop/* {
        reverse_proxy localhost:8080
    }

    # Frontend (Dioxus dev server)
    reverse_proxy localhost:5173

    # Logging
    log {
        output file /opt/choiros/logs/caddy.log {
            roll_size 10MB
            roll_keep 5
        }
    }
}

# When you have a domain, replace :80 with:
# yourdomain.com {
#     tls your-email@example.com
#     ... rest of config
# }
EOF

systemctl restart caddy
systemctl enable caddy
echo "Caddy configured"

# Set permissions
echo ""
echo "🔒 Setting permissions..."
chown -R $APP_USER:$APP_USER $APP_DIR
chown -R $APP_USER:$APP_USER /home/$APP_USER/.cargo 2>/dev/null || true

echo ""
echo "✅ Server setup complete!"
echo ""
echo "═══════════════════════════════════════════════════"
echo "Next steps:"
echo "1. Set up GitHub secrets for auto-deployment:"
echo "   - SSH_HOST: $(curl -s ifconfig.me)"
echo "   - SSH_USER: $APP_USER"
echo "   - SSH_KEY: (your private key)"
echo "   - DEPLOY_PATH: $APP_DIR"
echo ""
echo "2. Deploy the application:"
echo "   cd $APP_DIR && ./scripts/deploy.sh"
echo ""
echo "3. Test the deployment:"
echo "   curl http://$(curl -s ifconfig.me)/health"
echo ""
echo "4. Open in browser:"
echo "   http://$(curl -s ifconfig.me)"
echo "═══════════════════════════════════════════════════"
echo ""
echo "Server details:"
echo "- User: $APP_USER"
echo "- App directory: $APP_DIR"
echo "- Logs: $APP_DIR/logs/"
echo "- Data: $APP_DIR/data/"
echo ""
echo "Useful commands:"
echo "  sudo systemctl status choiros-backend  # Check backend"
echo "  sudo systemctl status choiros-frontend # Check frontend"
echo "  sudo tail -f $APP_DIR/logs/backend.log # View logs"
echo ""
