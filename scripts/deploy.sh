#!/bin/bash
# Deployment script for ChoirOS
# Run this on the server to deploy the application

set -e

APP_DIR="/opt/choiros"
BACKEND_PORT=8080
FRONTEND_PORT=5173

echo "🚀 ChoirOS Deployment"
echo "===================="
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ] || [ ! -d "sandbox" ]; then
    echo -e "${RED}❌ Error: Must run from project root directory${NC}"
    echo "Current directory: $(pwd)"
    echo "Expected to find Cargo.toml and sandbox/ directory"
    exit 1
fi

# Check Rust is available
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}❌ Error: Rust/Cargo not found${NC}"
    echo "Please install Rust: https://rustup.rs"
    exit 1
fi

# Check Dioxus CLI
if ! command -v dx &> /dev/null; then
    echo -e "${YELLOW}⚠️  Dioxus CLI not found, installing...${NC}"
    cargo install dioxus-cli
fi

echo "📦 Building backend..."
cargo build -p sandbox --release 2>&1 | tee logs/build-backend.log
if [ ${PIPESTATUS[0]} -ne 0 ]; then
    echo -e "${RED}❌ Backend build failed${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Backend built successfully${NC}"

echo ""
echo "📦 Building frontend..."
cd sandbox-ui
dx build --release 2>&1 | tee ../logs/build-frontend.log
if [ ${PIPESTATUS[0]} -ne 0 ]; then
    echo -e "${RED}❌ Frontend build failed${NC}"
    exit 1
fi
cd ..
echo -e "${GREEN}✅ Frontend built successfully${NC}"

echo ""
echo "🔄 Managing services..."

# Function to check if service exists
service_exists() {
    local service_name=$1
    if systemctl list-unit-files | grep -q "^$service_name"; then
        return 0
    else
        return 1
    fi
}

# Function to restart or start service
restart_service() {
    local service_name=$1
    local friendly_name=$2
    
    if service_exists "$service_name"; then
        echo "Restarting $friendly_name..."
        sudo systemctl restart $service_name
        sleep 2
        
        if systemctl is-active --quiet $service_name; then
            echo -e "${GREEN}✅ $friendly_name is running${NC}"
        else
            echo -e "${RED}❌ $friendly_name failed to start${NC}"
            echo "Check logs: sudo journalctl -u $service_name -n 50"
            return 1
        fi
    else
        echo -e "${YELLOW}⚠️  Service $service_name not found${NC}"
        return 1
    fi
}

# Restart backend
if restart_service "choiros-backend" "Backend"; then
    BACKEND_OK=true
else
    BACKEND_OK=false
fi

# Restart frontend
if restart_service "choiros-frontend" "Frontend"; then
    FRONTEND_OK=true
else
    FRONTEND_OK=false
fi

# Check Caddy
if service_exists "caddy"; then
    echo "Checking Caddy..."
    if ! systemctl is-active --quiet caddy; then
        echo "Starting Caddy..."
        sudo systemctl start caddy
    fi
    
    if systemctl is-active --quiet caddy; then
        echo -e "${GREEN}✅ Caddy is running${NC}"
        CADDY_OK=true
    else
        echo -e "${RED}❌ Caddy is not running${NC}"
        CADDY_OK=false
    fi
else
    echo -e "${YELLOW}⚠️  Caddy service not found${NC}"
    CADDY_OK=false
fi

echo ""
echo "🔍 Running health checks..."
sleep 3

# Check backend health
if [ "$BACKEND_OK" = true ]; then
    if curl -s http://localhost:$BACKEND_PORT/health > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Backend health check passed${NC}"
        BACKEND_HEALTHY=true
    else
        echo -e "${RED}❌ Backend health check failed${NC}"
        echo "Backend may still be starting up..."
        BACKEND_HEALTHY=false
    fi
else
    BACKEND_HEALTHY=false
fi

# Check frontend
if [ "$FRONTEND_OK" = true ]; then
    if curl -s http://localhost:$FRONTEND_PORT > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Frontend is responding${NC}"
        FRONTEND_HEALTHY=true
    else
        echo -e "${RED}❌ Frontend check failed${NC}"
        FRONTEND_HEALTHY=false
    fi
else
    FRONTEND_HEALTHY=false
fi

# Get server IP
SERVER_IP=$(curl -s ifconfig.me 2>/dev/null || echo "YOUR_SERVER_IP")

echo ""
echo "═══════════════════════════════════════════════════"
if [ "$BACKEND_HEALTHY" = true ] && [ "$FRONTEND_HEALTHY" = true ]; then
    echo -e "${GREEN}🎉 Deployment successful!${NC}"
    echo ""
    echo "Application is running at:"
    echo "  http://$SERVER_IP"
    echo ""
    echo "Health check:"
    echo "  curl http://$SERVER_IP/health"
else
    echo -e "${YELLOW}⚠️  Deployment completed with warnings${NC}"
    echo ""
    echo "Some services may still be starting up."
    echo "Check status in a few seconds:"
    echo "  sudo systemctl status choiros-backend"
    echo "  sudo systemctl status choiros-frontend"
fi
echo "═══════════════════════════════════════════════════"

echo ""
echo "Service Status:"
echo "---------------"
systemctl is-active choiros-backend && echo "Backend: $(systemctl is-active choiros-backend)" || echo "Backend: $(systemctl is-active choiros-backend)"
systemctl is-active choiros-frontend && echo "Frontend: $(systemctl is-active choiros-frontend)" || echo "Frontend: $(systemctl is-active choiros-frontend)"
systemctl is-active caddy && echo "Caddy: $(systemctl is-active caddy)" || echo "Caddy: $(systemctl is-active caddy)"

echo ""
echo "Logs:"
echo "-----"
echo "Backend:  tail -f logs/backend.log"
echo "Frontend: tail -f logs/frontend.log"
echo "Caddy:    tail -f logs/caddy.log"
echo ""
