# Rari Research Report

## Executive Summary

**Rari** is the Rust-based build system powering [MDN Web Docs](https://developer.mozilla.org), the authoritative documentation resource for web technologies. It is the successor to Yari (a Node.js-based system), representing Mozilla's strategic rewrite of their documentation platform in Rust for improved performance and maintainability.

---

## Purpose

Rari transforms Markdown-based documentation into structured JSON output that powers the MDN website. Its primary purpose is to:

- Build static documentation from Markdown source files
- Generate `index.json` files for all documentation pages
- Support content and translations for MDN Web Docs
- Provide a fast, reliable build pipeline for one of the world's most visited developer documentation sites

---

## Architecture

Rari follows a modular workspace architecture with specialized crates:

| Crate | Purpose |
|-------|---------|
| `rari-doc` | Core documentation processing |
| `rari-md` | Markdown parsing and transformation |
| `rari-types` | Type definitions and schemas |
| `rari-utils` | Utility functions |
| `rari-deps` | Dependency management |
| `rari-data` | Data handling and structures |
| `rari-templ-func` | Template function processing |
| `rari-linter` | Content linting |
| `rari-lsp` | Language Server Protocol support |
| `rari-sitemap` | Sitemap generation |
| `rari-tools` | Additional tooling |
| `css-syntax` / `css-syntax-types` / `css-definition-syntax` | CSS-related parsing |

### Key Dependencies
- **Tracing**: Structured logging
- **Serde**: Serialization/deserialization
- **Regex/Regress**: Pattern matching
- **Tree-sitter**: Parsing (custom MDN grammar)
- **Tokio**: Async runtime
- **Axum**: Web server (for dev mode)
- **Rayon**: Parallel processing
- **Reqwest**: HTTP client

---

## Programming Language

- **Primary**: Rust (Edition 2024)
- **Minimum Rust Version**: 1.90
- **License**: Mozilla Public License 2.0

---

## Key Features

1. **High Performance**: Rust-based implementation for faster builds compared to Node.js predecessor
2. **Markdown-to-JSON Pipeline**: Transforms MDN's Markdown content into structured JSON
3. **Template System**: Custom template function processing (`rari-templ-func`)
4. **LSP Support**: Built-in Language Server Protocol for editor integration
5. **Linting**: Content quality enforcement via `rari-linter`
6. **Self-updating CLI**: Built-in update mechanism
7. **Development Server**: Axum-based local server for preview
8. **Parallel Processing**: Rayon-powered concurrent builds
9. **CSS Syntax Support**: Specialized crates for CSS documentation

---

## Target Use Cases

1. **MDN Web Docs Production Builds**: Primary use case for generating the live site
2. **Local Development**: Developer preview server for content authors
3. **Content Validation**: Linting and quality checks for documentation
4. **Translation Workflows**: Supporting MDN's extensive localization efforts
5. **Third-party MDN-like Sites**: Potential for similar documentation platforms

---

## Community & Ecosystem Status

| Metric | Value |
|--------|-------|
| GitHub Stars | 36 |
| Forks | 28 |
| Open Issues | 53 |
| Created | June 24, 2024 |
| Latest Release | v0.2.12 (February 3, 2026) |
| Primary Maintainer | @fiji-flo (Florian Dieminger) |

### Development Status
- **⚠️ Work in Progress**: The project explicitly states it is "work in progress and lacking most of its documentation"
- **Not Production Ready**: Mozilla does not encourage external usage yet
- **Yari Parity Goal**: Currently aiming for feature parity with the legacy Yari build system
- **No External Contributions**: Not accepting contributions until stable

### Communication Channels
- Discord: https://developer.mozilla.org/discord

---

## Strengths

1. **Performance**: Rust provides significant speed improvements over Node.js
2. **Type Safety**: Rust's type system reduces runtime errors
3. **Memory Safety**: Eliminates entire classes of bugs present in JavaScript
4. **Mozilla Backing**: Official MDN project with dedicated engineering team
5. **Modern Architecture**: Clean separation of concerns via workspace crates
6. **LSP Integration**: First-class editor support
7. **Active Development**: Regular releases (latest v0.2.12 in Feb 2026)

---

## Weaknesses

1. **Documentation Gap**: Severely lacking documentation (acknowledged by maintainers)
2. **Instability**: Codebase changes frequently; API not stable
3. **Closed Contributions**: Not accepting external contributions during rewrite
4. **Small Community**: Only 36 stars, limited adoption outside MDN
5. **Migration Risk**: Still reaching parity with Yari; potential breaking changes
6. **Complexity**: Multi-crate workspace may be overkill for simpler use cases
7. **Rust Barrier**: Requires Rust knowledge for contributions (vs. JavaScript for Yari)

---

## Comparison: Rari vs. Yari

| Aspect | Yari (Legacy) | Rari (Current) |
|--------|---------------|----------------|
| Language | JavaScript/Node.js | Rust |
| Status | Maintenance mode | Active development |
| Performance | Slower | Faster |
| Documentation | Extensive | Minimal |
| External Use | Encouraged | Discouraged (for now) |
| Stability | Stable | Unstable |

---

## Conclusion

Rari represents Mozilla's strategic investment in modernizing MDN's infrastructure. While promising significant performance and reliability improvements, it is currently an internal tool not ready for external adoption. Organizations interested in similar documentation build systems should either wait for the official announcement or consider alternatives like Docusaurus, VitePress, or continuing with Yari until Rari stabilizes.

**Recommendation**: Monitor for the official migration announcement before considering adoption.

---

*Research conducted: 2026-02-13*
*Sources: GitHub API (mdn/rari), Cargo.toml, README.md, GitHub Releases*
