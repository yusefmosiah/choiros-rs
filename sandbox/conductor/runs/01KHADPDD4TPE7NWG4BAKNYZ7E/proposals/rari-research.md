# Rari Framework Research Report

## Overview

**Rari** (Runtime Accelerated Rendering Infrastructure) is a high-performance React Server Components framework powered by a Rust runtime. It aims to deliver exceptional performance while maintaining a zero-configuration developer experience.

---

## What Is Rari?

Rari is a full-stack React framework that emphasizes:
- **Performance-first architecture** with Rust-powered runtime
- **React Server Components (RSC)** by default
- **Zero-config setup** with sensible defaults
- **File-based routing** similar to Next.js App Router

---

## Language & Technology Stack

| Component | Technology |
|-----------|------------|
| **Runtime** | Rust |
| **Frontend** | React (Server Components by default) |
| **Bundling** | Rolldown-Vite (Rust-based bundler) |
| **Type Checking** | tsgo (10x faster TypeScript checking) |
| **Package Manager** | npm, pnpm, yarn, bun, deno supported |

**Language Distribution** (from GitHub):
- Rust: ~62% (1.7M lines)
- TypeScript: ~28% (795K lines)
- JavaScript: ~7% (206K lines)
- MDX, CSS, HTML: ~3%

---

## Architecture

### Core Components

1. **Rust Runtime Engine**
   - Persistent runtime for maximum performance
   - Handles SSR, RSC streaming, and bundling
   - Built on top of modern Rust async ecosystem

2. **App Router**
   - File-based routing system
   - Supports layouts, loading states, error boundaries
   - Nested route definitions

3. **React Server Components**
   - Server components by default
   - Client components with `'use client'` directive
   - Streaming SSR with Suspense boundaries

4. **Build System**
   - Powered by Rolldown (Rust-based Vite alternative)
   - Native-speed bundling
   - Hot Module Replacement (HMR)

---

## Key Features

| Feature | Description |
|---------|-------------|
| **Zero Config** | Works out of the box with pre-built binaries |
| **True SSR** | Pre-rendered HTML with instant hydration |
| **Streaming** | Progressive rendering with Suspense |
| **TypeScript First** | Full type safety across server/client |
| **Cross-Platform** | macOS, Linux, Windows support |
| **Universal NPM** | Use any npm package seamlessly |
| **Code Splitting** | Automatic client component splitting |

---

## Performance Claims

Rari claims significant performance improvements over Next.js:

| Metric | Rari | Next.js | Improvement |
|--------|------|---------|-------------|
| Response Time | 0.43ms | 3.92ms | **9.1x faster** |
| Throughput | 74,662 req/s | 1,605 req/s | **46.5x higher** |
| Latency (P95) | 1.15ms | 38.93ms | **33.9x faster** |
| Bundle Size | 264 KB | 562 KB | **53% smaller** |
| Build Time | 0.93s | 3.47s | **3.7x faster** |

*Benchmarks from February 2026, methodology available in benchmarks/ directory*

---

## Use Cases

### Ideal For:
- **High-performance web applications** requiring low latency
- **Content-heavy sites** benefiting from RSC
- **React developers** seeking better performance without complexity
- **Teams wanting zero-config setup** with Rust performance

### Comparison with Alternatives:
- vs **Next.js**: Better raw performance, smaller bundles
- vs **Remix**: Different architecture (RSC-first vs progressive enhancement)
- vs **Astro**: Full React integration vs islands architecture

---

## Pros & Cons

### Pros ✅
- Exceptional performance metrics
- Zero-configuration setup
- Modern React patterns (RSC, Streaming)
- Rust-powered reliability
- Active development
- MIT Licensed

### Cons ⚠️
- **Very new** (created July 2025)
- Small ecosystem compared to Next.js
- Limited third-party integrations
- Migration path from existing frameworks unclear
- Production battle-testing still ongoing

---

## Community & Maturity

| Metric | Value |
|--------|-------|
| **GitHub Stars** | 815 |
| **Forks** | 19 |
| **Contributors** | 3 (primarily skiniks) |
| **Open Issues** | 2 |
| **Created** | July 26, 2025 |
| **License** | MIT |

### Community Channels:
- **Discord**: Active community server
- **GitHub Discussions**: Enabled
- **Bluesky**: @rari.build

### Maturity Assessment: **Early Stage**
- ⚠️ Less than 1 year old
- ⚠️ Single main contributor (skiniks/Ryan Skinner)
- ✅ Regular commits and releases
- ✅ Responsive to issues
- ⚠️ Limited real-world production usage documented

---

## Unique Selling Points

1. **"Write JavaScript, Get Rust Performance"**
   - Developer experience of JS with runtime performance of Rust

2. **Correct RSC Semantics**
   - Server components by default, opt-in client components

3. **Persistent Rust Runtime**
   - Unlike Node.js-based frameworks, maintains state for better performance

4. **Integrated Toolchain**
   - Rolldown + tsgo + custom runtime all work together

5. **Performance Transparency**
   - Published benchmarks with reproducible methodology

---

## Recent Development Activity

Recent commits (as of Feb 13, 2026):
- Rate limiting improvements with proxy IP extraction
- Client component code splitting
- Release tooling improvements
- RSC renderer optimizations

---

## Conclusion

Rari represents an ambitious attempt to bring Rust-level performance to React development. While the performance claims are impressive, the framework is still in early stages with a small community. It shows promise for performance-critical applications but carries the risks associated with early-stage technology.

**Recommendation**: Evaluate for new projects where performance is critical and team is comfortable with bleeding-edge tech. Consider waiting for more maturity for production enterprise applications.

---

*Research conducted: February 13, 2026*
*Sources: GitHub (rari-build/rari), rari.build website, npm registry*