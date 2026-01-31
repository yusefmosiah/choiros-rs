# ChoirOS Testing Strategy

**Version:** 1.0  
**Date:** 2026-01-31  
**Status:** Active - Phase 1 Complete, Phase 2 Ready  

---

## Executive Summary

ChoirOS requires a multi-layered testing strategy that accounts for its unique architecture:
- **Backend:** Rust + Actix Web + SQLite (libsql) with Event Sourcing
- **Frontend:** Dioxus (WASM) + Rust compiled to WebAssembly
- **Architecture:** Actor-based state management (DesktopActor, ChatActor)
- **Communication:** HTTP REST API with CORS
- **Unique Challenge:** Frontend runs in browser WASM sandbox, backend is native Rust

This strategy combines patterns from multiple testing methodologies while staying practical for a small team.

---

## Testing Pyramid for ChoirOS

```
                    /\
                   /E2E\          ← Few tests, full user journeys
                  /─────\            (Playwright + dev-browser skill)
                 /Integr \         ← API contracts, component integration
                /─────────\          (Backend: API tests, Frontend: Component tests)
               /Unit Tests\       ← Many tests, isolated logic
              /─────────────\        (Backend: Actor tests, Frontend: Utility tests)
             /────────────────\
```

**Target Ratios:**
- Unit Tests: 70% (fast, isolated)
- Integration Tests: 20% (APIs, component integration)
- E2E Tests: 10% (critical user journeys)

---

## Tier 1: Unit Tests (Backend & Frontend Logic)

### Backend Unit Tests (Rust)

**Status:** ✅ **IMPLEMENTED** (18 tests passing)

**Current Coverage:**
- EventStoreActor (3 tests)
- ChatActor (8 tests)
- DesktopActor (7 tests)

**Patterns Used:**
- Actix test framework (`#[actix::test]`)
- In-memory SQLite for isolation
- Event projection testing
- Optimistic update testing

**Example Pattern (ChatActor):**
```rust
#[actix::test]
async fn test_send_message_creates_pending() {
    let chat = ChatActor::new(
        "chat-1".to_string(), 
        "user-1".to_string(), 
        EventStoreActor::new_in_memory().await.unwrap().start()
    ).start();
    
    let temp_id = chat.send(SendUserMessage {
        text: "Hello world".to_string(),
    }).await.unwrap().unwrap();
    
    let messages = chat.send(GetMessages).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].pending);
}
```

**Gaps to Fill:**
- [ ] EventStoreActor edge cases (concurrent writes, error handling)
- [ ] DesktopActor window cascading logic
- [ ] ActorManager supervision/restart testing
- [ ] API layer input validation testing

**Recommended Additions:**
```rust
// Test window position cascading
test_window_positions_cascade_correctly()

// Test actor restart preserves identity
test_actor_supervision_restarts_with_same_id()

// Test API validation
test_invalid_desktop_id_returns_400()
```

---

### Frontend Unit Tests (Dioxus/Rust)

**Status:** 🔄 **NOT YET IMPLEMENTED**

**Challenge:** Dioxus testing in WASM environment is limited. We use a hybrid approach.

**Strategy:**
1. **Test pure functions** (formatters, validators, utilities)
2. **Test hooks** (custom Dioxus hooks)
3. **Test API client** (HTTP request/response handling)
4. **Skip component unit tests** → Use integration tests instead

**Patterns (Custom for Dioxus/WASM):**

```rust
// tests/api_client_tests.rs
#[cfg(test)]
mod tests {
    use wasm_bindgen_test::*;
    use sandbox_ui::api::*;
    
    wasm_bindgen_test_configure!(run_in_browser);
    
    #[wasm_bindgen_test]
    async fn test_fetch_messages_parses_response() {
        // Mock response
        let mock_json = r#"{"success":true,"messages":[{"id":"1","text":"Hello","sender":"User","timestamp":"2026-01-31T12:00:00Z","pending":false}]}"#;
        
        // Test parsing logic
        let response: GetMessagesResponse = serde_json::from_str(mock_json).unwrap();
        assert_eq!(response.messages.len(), 1);
        assert_eq!(response.messages[0].text, "Hello");
    }
}
```

**Implementation Priority:**
- [ ] Add `wasm-bindgen-test` dependency
- [ ] Create `sandbox-ui/tests/` directory
- [ ] Write API response parsing tests
- [ ] Write utility function tests
- [ ] Skip: Component rendering tests (use E2E instead)

---

## Tier 2: Integration Tests

### Backend API Integration Tests

**Status:** 🔄 **NOT YET IMPLEMENTED**

**Goal:** Test full HTTP request/response cycles with real database

**Approach:** 
- Use Actix Web's test framework
- Spin up full server with test database
- Test HTTP endpoints end-to-end
- Clean database between tests

**Patterns (from webapp-testing + custom Rust):**

```rust
// tests/api_integration_tests.rs
use actix_web::{test, App};
use sandbox::api;

#[actix::test]
async fn test_desktop_api_end_to_end() {
    // Setup test database
    let db_path = "/tmp/test_choiros.db";
    let event_store = EventStoreActor::new(db_path).await.unwrap().start();
    let app_state = AppState::new(event_store);
    
    // Create test app
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(app_state))
            .configure(api::config)
    ).await;
    
    // Test 1: Get desktop state (empty)
    let req = test::TestRequest::get()
        .uri("/desktop/test-desktop")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["success"].as_bool().unwrap());
    assert!(body["desktop"]["windows"].as_array().unwrap().is_empty());
    
    // Test 2: Open window
    let req = test::TestRequest::post()
        .uri("/desktop/test-desktop/windows")
        .set_json(&json!({"app_id": "chat", "title": "Test Chat"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    
    // Test 3: Verify window created
    let req = test::TestRequest::get()
        .uri("/desktop/test-desktop")
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["desktop"]["windows"].as_array().unwrap().len(), 1);
    
    // Cleanup
    std::fs::remove_file(db_path).ok();
}
```

**Test Database Strategy:**
- Use `tempfile` crate for temporary database paths
- Each test gets isolated database
- Cleanup in `Drop` or `after_each`

**Endpoints to Cover:**
- [ ] GET /health
- [ ] POST /chat/send + GET /chat/{id}/messages
- [ ] GET /desktop/{id}
- [ ] POST /desktop/{id}/windows
- [ ] DELETE /desktop/{id}/windows/{id}
- [ ] POST /desktop/{id}/windows/{id}/focus
- [ ] GET /desktop/{id}/apps
- [ ] POST /desktop/{id}/apps

---

### Frontend Component Integration Tests

**Status:** 🔄 **NOT YET IMPLEMENTED**

**Challenge:** Dioxus doesn't have a Testing Library equivalent (yet).

**Solution:** Use browser-based testing with dev-browser skill

**Hybrid Approach:**
1. **Mount components in test page**
2. **Use dev-browser to interact**
3. **Verify DOM changes via screenshots/selectors**

**Implementation:**

```rust
// Create a test harness app
// sandbox-ui/src/test_harness.rs

#[component]
fn TestHarness() -> Element {
    rsx! {
        div { id: "test-root",
            // Test different component states
            Desktop { desktop_id: "test-desktop".to_string() }
        }
    }
}
```

Then test with dev-browser:
```python
# tests/component_integration.py (using dev-browser skill)
# This runs via the dev-browser skill we installed

"""
Test ChoirOS Desktop Component Integration
"""

async def test_desktop_loads():
    # Navigate to test page
    await page.goto("http://localhost:5173/test")
    await page.wait_for_load_state("networkidle")
    
    # Screenshot for visual verification
    await page.screenshot(path="screenshots/test-desktop-loads.png")
    
    # Verify "No windows open" message
    assert await page.locator("text=No windows open").is_visible()

async def test_open_chat_window():
    await page.goto("http://localhost:5173/test")
    await page.wait_for_load_state("networkidle")
    
    # Click Chat icon
    await page.get_by_text("💬").click()
    
    # Wait for window
    await page.wait_for_selector(".window-chrome")
    
    # Verify window opened
    assert await page.locator("text=Chat").is_visible()
    
    # Screenshot
    await page.screenshot(path="screenshots/test-chat-opened.png")
```

**Alternative:** Wait for Dioxus Testing Library (in development)

---

## Tier 3: End-to-End (E2E) Tests

### Full User Journey Testing

**Status:** 🔄 **NOT YET IMPLEMENTED**

**Tool:** dev-browser skill (Playwright-based)

**Critical User Journeys (CUJs):**

#### CUJ 1: First-Time User Opens Chat
```gherkin
Given a fresh browser session
When the user navigates to ChoirOS
Then the Desktop loads with "No windows open" message
And the Chat app icon is visible in taskbar

When the user clicks the Chat icon
Then a Chat window opens
And the window shows the Chat interface
And the taskbar shows the active Chat window

When the user types "Hello ChoirOS!" and presses Enter
Then the message appears in the chat
And the message shows as "Sending..."
And after confirmation, "Sending..." disappears
```

**Test Implementation:**
```python
# tests/e2e/test_first_time_user.py

import pytest
from dev_browser import browser

@pytest.fixture
async def page():
    """Fresh browser page for each test"""
    p = await browser.new_page()
    yield p
    await p.close()

@pytest.mark.cuj
async def test_first_time_user_opens_chat(page):
    # 1. Load desktop
    await page.goto("http://localhost:5173")
    await page.wait_for_load_state("networkidle")
    
    # Verify initial state
    assert await page.locator("text=No windows open").is_visible()
    assert await page.locator("text=💬").is_visible()
    
    # 2. Open Chat
    await page.get_by_text("💬").click()
    await page.wait_for_selector(".window-chrome")
    
    # Verify window opened
    assert await page.locator("text=Chat").is_visible()
    assert await page.locator(".chat-container").is_visible()
    
    # 3. Send message
    input_box = page.locator(".message-input")
    await input_box.fill("Hello ChoirOS!")
    await input_box.press("Enter")
    
    # Verify optimistic update
    assert await page.locator("text=Hello ChoirOS!").is_visible()
    
    # Wait for confirmation
    await page.wait_for_timeout(1000)  # Wait for API response
    
    # Verify message confirmed (no "Sending...")
    message = page.locator(".user-bubble").filter(has_text="Hello ChoirOS!")
    assert await message.is_visible()
    
    # Screenshot for report
    await page.screenshot(path="screenshots/e2e-cuj1-complete.png")
```

#### CUJ 2: Window Management
```gherkin
Given a user with Chat window open
When the user opens a second window (if supported)
Then both windows appear in the taskbar
And the active window is highlighted

When the user clicks the other window in taskbar
Then that window becomes active
And the previous window is hidden (mobile) or lower z-index (desktop)

When the user clicks the close button
Then the window closes
And the "No windows open" message appears (if last window)
```

#### CUJ 3: Error Recovery
```gherkin
Given the backend server is stopped
When the user tries to open a window
Then an error message appears
And a "Retry" button is shown

When the user starts the backend and clicks Retry
Then the Desktop loads successfully
```

---

### Visual Regression Testing

**Status:** 🔄 **NOT YET IMPLEMENTED**

**Tool:** Playwright screenshot comparison (via dev-browser)

**Implementation:**
```python
# tests/visual/test_screenshots.py

async def test_desktop_layout_regression(page):
    """Compare current screenshot to baseline"""
    await page.goto("http://localhost:5173")
    await page.wait_for_load_state("networkidle")
    
    # Set viewport for consistent screenshots
    await page.set_viewport_size({"width": 390, "height": 844})  # iPhone 12
    
    # Screenshot and compare
    await expect(page).to_have_screenshot("desktop-mobile.png")

async def test_chat_window_regression(page):
    """Test window appearance"""
    await page.goto("http://localhost:5173")
    await page.wait_for_load_state("networkidle")
    
    # Open window
    await page.get_by_text("💬").click()
    await page.wait_for_selector(".window-chrome")
    
    # Screenshot window
    window = page.locator(".window-chrome")
    await expect(window).to_have_screenshot("chat-window.png")
```

**Baseline Management:**
- Store baselines in `tests/visual/baselines/`
- Update baselines intentionally (not on every change)
- Use in CI to catch unintended UI changes

---

### Cross-Browser Testing

**Browsers to Support:**
- Chrome (primary)
- Firefox
- Safari (macOS/iOS)
- Edge (Chromium-based)

**Implementation:**
```python
# Use Playwright's cross-browser support
# dev-browser skill uses Playwright under the hood

@pytest.fixture(params=["chromium", "firefox", "webkit"])
async def browser_page(request):
    browser_type = request.param
    browser = await playwright[browser_type].launch()
    page = await browser.new_page()
    yield page
    await browser.close()
```

---

## Testing Infrastructure

### Project Structure

```
choiros-rs/
├── sandbox/
│   ├── src/
│   │   ├── actors/           # Unit tested ✅
│   │   ├── api/              # Integration tested
│   │   └── ...
│   └── tests/
│       ├── integration/      # API integration tests
│       └── fixtures/         # Test data factories
│
├── sandbox-ui/
│   ├── src/
│   │   ├── desktop.rs        # E2E tested
│   │   ├── components.rs     # E2E tested
│   │   └── api.rs            # Unit tested (parsing)
│   └── tests/
│       ├── unit/             # WASM unit tests
│       └── e2e/              # dev-browser tests
│
└── tests/
    ├── e2e/                  # Full system tests
    │   ├── cuj/              # Critical user journeys
    │   └── visual/           # Screenshot tests
    ├── fixtures/             # Shared test data
    └── utils/                # Test helpers
```

### CI/CD Integration

**GitHub Actions Workflow:**
```yaml
# .github/workflows/test.yml
name: Test ChoirOS

on: [push, pull_request]

jobs:
  backend-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust@stable
      
      - name: Run unit tests
        run: cargo test -p sandbox
      
      - name: Run integration tests
        run: cargo test -p sandbox --test integration

  frontend-build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust@stable
      - uses: actions/setup-node@v3
      
      - name: Install Dioxus CLI
        run: cargo install dioxus-cli
      
      - name: Build frontend
        run: cargo build -p sandbox-ui

  e2e-tests:
    runs-on: ubuntu-latest
    needs: [backend-tests, frontend-build]
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
      
      - name: Install dev-browser dependencies
        run: cd ~/.agents/skills/dev-browser && npm install
      
      - name: Start backend
        run: cargo run -p sandbox &
      
      - name: Start frontend
        run: cd sandbox-ui && dx serve &
      
      - name: Wait for servers
        run: sleep 10
      
      - name: Run E2E tests
        run: npx dev-browser run tests/e2e/
      
      - name: Upload screenshots
        uses: actions/upload-artifact@v3
        with:
          name: e2e-screenshots
          path: screenshots/
```

### Local Development Testing

**Quick Test Command:**
```bash
#!/bin/bash
# test-local.sh

echo "Running ChoirOS Test Suite..."

# 1. Backend unit tests
echo "🧪 Backend Unit Tests..."
cargo test -p sandbox --quiet

# 2. Frontend build
echo "🎨 Frontend Build..."
cargo build -p sandbox-ui --quiet

# 3. API health check
echo "🔌 API Health Check..."
curl -s http://localhost:8080/health | grep -q "healthy" && echo "✅ Backend healthy" || echo "❌ Backend not running"

# 4. Manual testing reminder
echo ""
echo "Manual Testing:"
echo "  1. Start backend: cargo run -p sandbox"
echo "  2. Start frontend: cd sandbox-ui && dx serve"
echo "  3. Open: http://localhost:5173"
echo ""
echo "E2E Testing:"
echo "  npx dev-browser open http://localhost:5173"
```

---

## Test Data Management

### Fixtures Strategy

**Rust Backend (using fake crate):**
```rust
// tests/fixtures/mod.rs
use fake::{Fake, Faker};
use shared_types::*;

pub fn window_fixture() -> WindowState {
    WindowState {
        id: Faker.fake(),
        app_id: "chat".to_string(),
        title: Faker.fake::<String>(),
        x: (100..800).fake(),
        y: (100..600).fake(),
        width: 800,
        height: 600,
        z_index: 100,
        minimized: false,
        maximized: false,
        props: serde_json::json!({}),
    }
}

pub fn app_fixture() -> AppDefinition {
    AppDefinition {
        id: Faker.fake(),
        name: Faker.fake(),
        icon: "💬".to_string(),
        component_code: "ChatApp".to_string(),
        default_width: 800,
        default_height: 600,
    }
}
```

**Frontend API Mocking:**
```rust
// Mock service worker pattern for WASM tests
#[cfg(test)]
mod mocks {
    // Mock fetch responses for consistent testing
}
```

---

## Performance Testing

### Benchmarks

**Backend:**
```rust
// benches/actor_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_window_open(c: &mut Criterion) {
    c.bench_function("open 100 windows", |b| {
        b.iter(|| {
            // Open 100 windows
            for i in 0..100 {
                black_box(open_window(&format!("win-{}", i)));
            }
        })
    });
}
```

**Frontend:**
```javascript
// Lighthouse CI for performance budgets
// .lighthouserc.js
module.exports = {
  ci: {
    collect: {
      url: ['http://localhost:5173'],
      numberOfRuns: 3
    },
    assert: {
      assertions: {
        'categories:performance': ['warn', {minScore: 0.8}],
        'categories:accessibility': ['error', {minScore: 0.9}],
        'first-contentful-paint': ['warn', {maxNumericValue: 2000}]
      }
    }
  }
}
```

---

## Accessibility (A11y) Testing

**Implementation:**
```python
# tests/a11y/test_accessibility.py

async def test_desktop_accessibility(page):
    """Run axe-core accessibility audit"""
    await page.goto("http://localhost:5173")
    await page.wait_for_load_state("networkidle")
    
    # Run accessibility scan
    violations = await page.evaluate("""
        async () => {
            const axe = await import('axe-core');
            const results = await axe.run();
            return results.violations;
        }
    """)
    
    assert len(violations) == 0, f"Accessibility violations: {violations}"
```

**Checks:**
- [ ] Color contrast (WCAG AA)
- [ ] Keyboard navigation
- [ ] Screen reader compatibility
- [ ] Focus management

---

## Security Testing

**Areas to Cover:**
- [ ] API authentication (when added)
- [ ] CORS configuration
- [ ] SQL injection (via libsql parameterized queries ✅)
- [ ] XSS prevention
- [ ] WASM sandbox boundaries

---

## Testing Checklist

### Before Each Release

- [ ] All 18+ unit tests passing
- [ ] Integration tests passing (APIs)
- [ ] E2E tests passing (CUJs)
- [ ] No visual regressions
- [ ] Performance budgets met
- [ ] Accessibility audit clean
- [ ] Cross-browser tested (Chrome, Firefox, Safari)
- [ ] Mobile responsive tested

### Continuous

- [ ] Tests run on every PR
- [ ] Code coverage >80%
- [ ] No flaky tests
- [ ] Test documentation updated

---

## Implementation Roadmap

### Phase 1: Foundation ✅ (COMPLETE)
- [x] Backend unit tests (18 tests)
- [x] Basic API endpoint structure
- [x] Manual testing guide
- [x] Test report template

### Phase 2: Integration (NEXT)
- [ ] Backend API integration tests
- [ ] Frontend API parsing tests
- [ ] Test database utilities
- [ ] CI/CD pipeline setup

### Phase 3: E2E Automation
- [ ] dev-browser skill integration
- [ ] CUJ test implementation
- [ ] Visual regression baselines
- [ ] Cross-browser testing

### Phase 4: Advanced Testing
- [ ] Performance benchmarks
- [ ] Accessibility automation
- [ ] Load testing
- [ ] Chaos testing (actor failures)

---

## Resources

### Tools
- **Backend:** Built-in Rust testing + Actix test utils
- **Frontend:** dev-browser skill (Playwright-based)
- **API Testing:** curl, HTTPie, or Playwright
- **Coverage:** cargo-tarpaulin (Rust), upcoming Dioxus coverage

### Documentation
- `test-report.md` - Manual testing guide
- `test.sh` - Automated test script
- `screenshots/README.md` - Screenshot capture guide
- This document - Testing strategy

### References
- Patterns from: webapp-testing, e2e-testing-patterns, javascript-testing-patterns, frontend-testing skills
- Rust testing: https://doc.rust-lang.org/book/ch11-00-testing.html
- Actix testing: https://actix.rs/docs/testing/
- dev-browser: https://github.com/SawyerHood/dev-browser

---

## Summary

**ChoirOS Testing Philosophy:**
1. **Test at the right level** - Unit for logic, E2E for user journeys
2. **Stay practical** - Use what works (dev-browser), don't wait for perfect tools
3. **Automate ruthlessly** - CI/CD for everything
4. **Visual testing matters** - Screenshots catch UI regressions
5. **Test the hard parts** - Actor supervision, WASM boundaries, mobile layouts

**Current State:**
- ✅ Backend unit tests: COMPLETE (18 tests)
- 🔄 Integration tests: READY TO IMPLEMENT
- 🔄 E2E tests: READY WITH dev-browser SKILL
- 🔄 Visual regression: READY TO IMPLEMENT

**Next Action:**
1. Implement backend API integration tests
2. Set up dev-browser for E2E automation
3. Add CI/CD pipeline
4. Capture baseline screenshots

---

**Document Owner:** YM Nathanson  
**Last Updated:** 2026-01-31  
**Review Cycle:** Monthly or after major architecture changes
