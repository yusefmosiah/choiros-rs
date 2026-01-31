# Screenshots Directory

This directory should contain test screenshots of the ChoirOS Desktop UI.

## How to Capture Screenshots

### Prerequisites
1. Backend running: `cargo run -p sandbox`
2. Frontend running: `cd sandbox-ui && dx serve`
3. Browser open to: http://localhost:5173

### Screenshots to Capture

#### 1. Initial Load (01-initial-load.png)
- **Steps:** Open browser to http://localhost:5173
- **Expected:** "No windows open" message, taskbar with Chat icon
- **Device:** Any viewport

#### 2. Chat Window Opened (02-chat-window.png)
- **Steps:** Click Chat app icon (💬) in taskbar
- **Expected:** Full-screen window with title bar, Chat UI inside
- **Device:** Mobile (375x667) or Desktop

#### 3. Message Sent (03-message-sent.png)
- **Steps:** Type "Hello ChoirOS!" and press Enter
- **Expected:** Message appears in chat bubble (blue, right-aligned)
- **Device:** Any viewport

#### 4. API Test (04-api-test.png)
- **Steps:** Run `curl http://localhost:8080/health` in terminal
- **Expected:** JSON response showing "status":"healthy"
- **Capture:** Terminal window with command and output

#### 5. Mobile View (05-mobile-view.png)
- **Steps:** Open DevTools (F12) → Toggle Device Toolbar → Select iPhone 12
- **Expected:** Mobile layout with single window, bottom taskbar
- **Device:** iPhone 12 (390x844) or similar

#### 6. Desktop View (06-desktop-view.png)
- **Steps:** Use DevTools Desktop mode (1920x1080)
- **Expected:** Larger layout, still single window (floating in Phase 2)
- **Device:** Desktop (1920x1080)

### Using Browser DevTools

**Chrome/Edge:**
1. Press F12 to open DevTools
2. Click device toolbar icon (📱) or Ctrl+Shift+M
3. Select device preset or enter custom dimensions
4. Use Screenshot button (⋮ menu → Capture screenshot)

**Firefox:**
1. Press F12 to open DevTools
2. Click Responsive Design Mode (📱) or Ctrl+Shift+M
3. Select device or enter dimensions
4. Right-click page → Take Screenshot

**Safari:**
1. Enable Develop menu in preferences
2. Develop → Enter Responsive Design Mode
3. Select device
4. Use screenshot tools

### Naming Convention
- Format: `##-descriptive-name.png`
- Use lowercase with hyphens
- Number in order of testing

### Example
```
screenshots/
├── 01-initial-load.png
├── 02-chat-window.png
├── 03-message-sent.png
├── 04-api-test.png
├── 05-mobile-view.png
└── 06-desktop-view.png
```
