# Justfile - Task runner for ChoirOS

# Default recipe
default:
    @just --list

# Development commands
dev-sandbox:
    cd sandbox && cargo run

dev-hypervisor:
    cd hypervisor && cargo run

dev-ui:
    cd sandbox-ui && dx serve --port 3000

# Build commands
build:
    cargo build --release

build-sandbox:
    cd sandbox-ui && dx build --release
    cp -r sandbox-ui/dist/* sandbox/static/
    cd sandbox && cargo build --release

# Testing
test:
    cargo test --workspace

test-unit:
    cargo test --lib --workspace

test-integration:
    cargo test --test '*' --workspace

# Code quality
check:
    cargo fmt --check
    cargo clippy --workspace -- -D warnings

fix:
    cargo fmt
    cargo clippy --fix --allow-staged

# Database
migrate:
    cd sandbox && cargo sqlx migrate run

new-migration NAME:
    cd sandbox && cargo sqlx migrate add {{NAME}}

# Docker
docker-build:
    docker build -t choir-sandbox:latest ./sandbox

docker-run:
    docker run -p 8080:8080 -v ./data:/data choir-sandbox:latest

# Deployment
deploy-ec2:
    rsync -avz --delete ./ ubuntu@3.83.131.245:~/choiros-rs/
    ssh ubuntu@3.83.131.245 'cd ~/choiros-rs && just build-sandbox'
