# Rari Research Proposal

## Executive Summary

**Rari** (Runtime Accelerated Rendering Infrastructure) is a high-performance React Server Components (RSC) framework powered by a Rust runtime. It represents a new generation of full-stack web frameworks that combine the ergonomics of React with the performance characteristics of systems programming.

This research covers Rari as a software framework for comparison with Dioxus, a Rust-native UI framework, focusing on architecture, performance characteristics, and ecosystem positioning.

---

## What is Rari?

### Definition
Rari is a **full-stack React framework** with a Rust-powered runtime that implements React Server Components (RSC) with zero-configuration setup. It aims to deliver exceptional performance while maintaining React's developer experience.

### Core Purpose
- Provide a Next.js-compatible alternative with dramatically better performance
- Implement correct React Server Components semantics
- Enable server-side rendering with instant hydration
- Offer file-based routing with modern app router patterns

---

## Language & Tech Stack

| Layer | Technology |
|-------|------------|
| **Runtime Core** | Rust (Tokio async runtime) |
| **Frontend** | React 19+ (Server Components by default) |
| **Build System** | Vite + Rolldown (Rust-based bundler) |
| **Package Manager** | pnpm with workspace catalogs |
| **Type System** | TypeScript (full type safety) |
| **Testing** | Vitest |

### Code Distribution (from crates.io analysis)
- **Rust**: ~40,442 lines (core runtime, server, RSC renderer)
- **JavaScript/TypeScript**: ~5,428 lines (client runtime, build tools)

---

## Architecture

### High-Level Structure
```
┌─────────────────────────────────────────┐
│           Client (Browser)              │
│  - Hydrated React Components            │
│  - Client-side navigation               │
└─────────────────┬───────────────────────┘
                  │ RSC Payload Stream
┌─────────────────▼───────────────────────┐
│           Rari Runtime (Rust)           │
│  ┌─────────────┐  ┌─────────────────┐   │
│  │   Server    │  │  RSC Renderer   │   │
│  │  (Axum/     │  │  (React Server  │   │
│  │   Tokio)    │  │   Components)   │   │
│  └─────────────┘  └─────────────────┘   │
│  ┌─────────────┐  ┌─────────────────┐   │
│  │   Runtime   │  │  Module Loader  │   │
│  │  (V8/JS)    │  │  (npm/CJS/ESM)  │   │
│  └─────────────┘  └─────────────────┘   │
└─────────────────────────────────────────┘
```

### Key Architectural Components

1. **RSC Renderer** (`crates/rari/src/rsc/`)
   - Implements React Server Components protocol
   - Streams serialized component trees to client
   - Handles server-side data fetching

2. **Runtime** (`crates/rari/src/runtime/`)
   - JavaScript/TypeScript execution environment
   - Component loading and caching
   - Client component bundle splitting

3. **Server** (`crates/rari/src/server/`)
   - HTTP server (Axum-based)
   - Request routing and middleware
   - Static asset serving
   - Hot Module Replacement (HMR)

4. **Module Loader**
   - Universal npm package support
   - CommonJS and ESM compatibility
   - Workspace-aware resolution

---

## Key Features

| Feature | Description |
|---------|-------------|
| **App Router** | File-based routing with layouts, loading states, error boundaries |
| **True SSR** | Pre-rendered HTML with progressive hydration |
| **RSC by Default** | Server components default; `'use client'` for interactivity |
| **Rust Runtime** | Persistent Tokio runtime for maximum throughput |
| **Zero Config** | Works out of box with pre-built binaries |
| **HMR** | Instant feedback during development |
| **Streaming SSR** | Progressive rendering with Suspense boundaries |
| **Universal npm** | Use any npm package seamlessly |
| **Cross-Platform** | macOS, Linux, Windows support |

---

## Performance Claims (vs Next.js)

| Metric | Rari | Next.js | Improvement |
|--------|------|---------|-------------|
| Avg Response Time | 0.43ms | 3.92ms | **9.1x faster** |
| Throughput | 74,662 req/s | 1,605 req/s | **46.5x higher** |
| Latency under load | 0.67ms | 31.17ms | **46.6x faster** |
| Bundle Size | 264 KB | 562 KB | **53% smaller** |
| Build Time | 0.93s | 3.47s | **3.7x faster** |

*Benchmarks from rari.build, February 2026*

---

## Strengths

1. **Performance**: Rust runtime delivers exceptional throughput and low latency
2. **React Compatibility**: Familiar React patterns, correct RSC semantics
3. **Developer Experience**: Zero-config setup, TypeScript-first, HMR
4. **Ecosystem Leverage**: Full npm compatibility without lock-in
5. **Modern Architecture**: Streaming, Suspense, progressive enhancement
6. **Build Speed**: Rust-based tooling (Rolldown) for fast builds

---

## Weaknesses & Limitations

1. **Early Stage**: ~812 GitHub stars, relatively new (created July 2025)
2. **Single Maintainer**: Primarily developed by Ryan Skinner (@skiniks)
3. **Ecosystem Immaturity**: Limited plugin ecosystem vs Next.js
4. **Documentation**: Still building out comprehensive docs
5. **Production Validation**: Limited large-scale production usage data
6. **Deployment Options**: Fewer managed hosting options than Vercel/Next.js

---

## Community & Ecosystem

| Metric | Value |
|--------|-------|
| GitHub Stars | 812 |
| Forks | 19 |
| Open Issues | 2 |
| Discord | Active (discord.gg/GSh2Ak3b8Q) |
| npm Package | `rari` (1,312 downloads) |
| Crates.io | `rari` (48 versions, v0.9.0 current) |
| License | MIT |

### Release Cadence
- Rapid iteration: 48 versions since October 2025
- Latest: v0.9.0 (February 12, 2026)
- Active development with daily/weekly releases

---

## Current Status

**Stage**: Early production-ready (v0.9.0)

### Recent Development Focus
- Client component bundle splitting (v0.9.0)
- Security hardening (SSRF vulnerability fixes)
- Migration from esbuild to Rolldown
- Image optimization and OG image generation
- Deployment tooling (@rari/deploy package)

### Comparison Context with Dioxus

| Aspect | Rari | Dioxus |
|--------|------|--------|
| **Paradigm** | React/JSX with Rust runtime | Rust-native with React-like patterns |
| **Language** | TypeScript/React frontend, Rust backend | Pure Rust |
| **Target** | Full-stack web (SSR/SSG) | Multi-platform (Web, Desktop, Mobile) |
| **Rendering** | Server Components + Hydration | VDOM or SSR (web), native (desktop) |
| **Ecosystem** | npm/React ecosystem | Rust crate ecosystem |
| **Learning Curve** | Low (if know React) | Medium (Rust knowledge required) |

---

## Conclusion

Rari represents an emerging category of **hybrid frameworks** that combine JavaScript/TypeScript frontend ergonomics with Rust backend performance. For teams already invested in React, it offers a compelling performance upgrade path without abandoning familiar patterns.

Compared to Dioxus, Rari targets a different use case: **full-stack web applications** requiring SSR/SSG with React compatibility, versus Dioxus's focus on **Rust-native cross-platform UI development**.

### Recommendation for Comparison
Rari should be evaluated as a **Next.js alternative** rather than a direct Dioxus competitor, though both demonstrate Rust's growing importance in modern web tooling.

---

*Research completed: February 13, 2026*
*Sources: GitHub (rari-build/rari), crates.io, npm, rari.build*
