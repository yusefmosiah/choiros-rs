//! E2E Integration Tests for ChoirOS Chat
//!
//! This test suite orchestrates:
//! 1. Backend server startup (sandbox)
//! 2. Frontend dev server startup (sandbox-ui)
//! 3. Browser automation tests via dev-browser skill
//! 4. Screenshot capture and verification
//!
//! Run with: cargo test -p sandbox --test integration_chat_e2e -- --nocapture

use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Test result with screenshot paths
#[derive(Debug)]
struct TestResult {
    name: String,
    passed: bool,
    screenshots: Vec<String>,
    duration_ms: u64,
    error: Option<String>,
}

/// Server process guard - kills process on drop
struct ServerGuard {
    process: Child,
    name: String,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        println!(
            "\n🛑 Stopping {} server (PID: {})...",
            self.name,
            self.process.id()
        );
        let _ = self.process.kill();
        let _ = self.process.wait();
        println!("✅ {} server stopped", self.name);
    }
}

/// Wait for a service to be healthy by polling its health endpoint
fn wait_for_healthy(url: &str, timeout_secs: u64, service_name: &str) -> Result<(), String> {
    let client = reqwest::blocking::Client::new();
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);

    println!(
        "⏳ Waiting for {} to be healthy at {}...",
        service_name, url
    );

    while start.elapsed() < timeout {
        match client.get(url).timeout(Duration::from_secs(2)).send() {
            Ok(response) => {
                if response.status().is_success() {
                    println!("✅ {} is healthy!", service_name);
                    return Ok(());
                }
            }
            Err(e) => {
                // Connection not ready yet, retry
                thread::sleep(Duration::from_millis(500));
            }
        }
    }

    Err(format!(
        "{} failed to become healthy within {} seconds",
        service_name, timeout_secs
    ))
}

/// Start the backend server
fn start_backend() -> Result<ServerGuard, String> {
    println!("\n🚀 Starting backend server...");

    // Set database path for testing
    let db_path = std::env::current_dir()
        .map_err(|e| format!("Failed to get current dir: {}", e))?
        .join("tests/data/test_events.db");

    // Ensure test data directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create test data dir: {}", e))?;
    }

    let process = Command::new("cargo")
        .args(["run", "-p", "sandbox"])
        .env("DATABASE_URL", db_path.to_str().unwrap())
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start backend: {}", e))?;

    println!("   Backend PID: {}", process.id());

    // Wait for backend to be healthy
    wait_for_healthy("http://localhost:8080/health", 60, "Backend")?;

    Ok(ServerGuard {
        process,
        name: "Backend".to_string(),
    })
}

/// Start the frontend dev server
fn start_frontend() -> Result<ServerGuard, String> {
    println!("\n🚀 Starting frontend dev server...");

    let process = Command::new("cargo")
        .args(["run", "-p", "sandbox-ui"])
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start frontend: {}", e))?;

    println!("   Frontend PID: {}", process.id());

    // Wait for frontend to be ready (check if it responds)
    wait_for_healthy("http://localhost:3000", 60, "Frontend")?;

    Ok(ServerGuard {
        process,
        name: "Frontend".to_string(),
    })
}

/// Start the dev-browser automation server
fn start_browser_server() -> Result<ServerGuard, String> {
    println!("\n🚀 Starting browser automation server...");

    let process = Command::new("bash")
        .arg("./skills/dev-browser/server.sh")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start browser server: {}", e))?;

    println!("   Browser server PID: {}", process.id());

    // Wait for server to be ready (check port 3001)
    wait_for_healthy("http://localhost:3001/health", 30, "Browser Server")?;

    Ok(ServerGuard {
        process,
        name: "Browser".to_string(),
    })
}

/// Run a TypeScript E2E test script
fn run_e2e_test(test_name: &str) -> Result<TestResult, String> {
    let start = Instant::now();
    let screenshot_dir = "tests/screenshots/phase4";

    println!("\n🧪 Running test: {}", test_name);

    // Run the TypeScript test script
    let output = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "cd /Users/wiz/choiros-rs/skills/dev-browser && npx tsx /Users/wiz/choiros-rs/tests/e2e/{}.ts",
            test_name
        ))
        .env("SCREENSHOT_DIR", screenshot_dir)
        .env("TEST_NAME", test_name)
        .output()
        .map_err(|e| format!("Failed to run test {}: {}", test_name, e))?;

    let duration_ms = start.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse test output to determine pass/fail and collect screenshots
    let passed = output.status.success();
    let error = if !passed {
        Some(format!(
            "Exit code: {:?}\nStdout: {}\nStderr: {}",
            output.status.code(),
            stdout,
            stderr
        ))
    } else {
        None
    };

    // Extract screenshot paths from output
    let screenshots: Vec<String> = stdout
        .lines()
        .filter(|line| line.contains("SCREENSHOT:"))
        .map(|line| line.replace("SCREENSHOT:", "").trim().to_string())
        .collect();

    Ok(TestResult {
        name: test_name.to_string(),
        passed,
        screenshots,
        duration_ms,
        error,
    })
}

/// Run all E2E tests and generate report
fn run_all_e2e_tests() -> Vec<TestResult> {
    let tests = vec![
        "test_e2e_basic_chat_flow",
        "test_e2e_multiturn_conversation",
        "test_e2e_tool_execution",
        "test_e2e_error_handling",
        "test_e2e_connection_recovery",
        "test_e2e_concurrent_users",
    ];

    let mut results = Vec::new();

    for test in &tests {
        match run_e2e_test(test) {
            Ok(result) => {
                println!(
                    "   Result: {}",
                    if result.passed {
                        "✅ PASS"
                    } else {
                        "❌ FAIL"
                    }
                );
                results.push(result);
            }
            Err(e) => {
                println!("   Result: ❌ FAIL (Error: {})", e);
                results.push(TestResult {
                    name: test.to_string(),
                    passed: false,
                    screenshots: vec![],
                    duration_ms: 0,
                    error: Some(e),
                });
            }
        }
    }

    results
}

/// Print test report
fn print_report(results: &[TestResult]) {
    println!("\n");
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║                  E2E TEST REPORT - Phase 4                     ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();

    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total - passed;

    println!("Summary: {}/{} tests passed", passed, total);
    println!();

    for result in results {
        let status = if result.passed {
            "✅ PASS"
        } else {
            "❌ FAIL"
        };
        println!("{} - {} ({}ms)", status, result.name, result.duration_ms);

        if !result.screenshots.is_empty() {
            println!("   Screenshots:");
            for screenshot in &result.screenshots {
                println!("     - {}", screenshot);
            }
        }

        if let Some(ref error) = result.error {
            println!("   Error: {}", error);
        }
        println!();
    }

    println!("Screenshots saved to: tests/screenshots/phase4/");
    println!();
}

#[test]
#[ignore = "E2E tests require servers and browser automation - run manually with: cargo test -p sandbox --test integration_chat_e2e -- --ignored --nocapture"]
fn integration_chat_e2e_full_suite() {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║     ChoirOS E2E Integration Tests - Phase 4: Chat Flow        ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    // Create screenshot directory
    let screenshot_dir = std::path::PathBuf::from("tests/screenshots/phase4");
    std::fs::create_dir_all(&screenshot_dir).expect("Failed to create screenshot directory");

    // Step 1: Start backend server
    let _backend = match start_backend() {
        Ok(guard) => guard,
        Err(e) => {
            panic!("Failed to start backend: {}", e);
        }
    };

    // Give backend a moment to fully initialize
    thread::sleep(Duration::from_secs(2));

    // Step 2: Start frontend server
    let _frontend = match start_frontend() {
        Ok(guard) => guard,
        Err(e) => {
            panic!("Failed to start frontend: {}", e);
        }
    };

    // Step 3: Start browser automation server
    let _browser = match start_browser_server() {
        Ok(guard) => guard,
        Err(e) => {
            panic!("Failed to start browser server: {}", e);
        }
    };

    println!("\n✅ All servers are ready! Starting E2E tests...");

    // Step 4: Run all E2E tests
    let results = run_all_e2e_tests();

    // Step 5: Print report
    print_report(&results);

    // Assert all tests passed
    let failed = results.iter().filter(|r| !r.passed).count();
    if failed > 0 {
        panic!("{} E2E tests failed!", failed);
    }

    println!("\n🎉 All E2E tests passed!");
}

/// Individual test: Basic Chat Flow
/// Can be run independently for debugging
#[test]
#[ignore = "Individual E2E test - run manually with: cargo test -p sandbox --test integration_chat_e2e test_e2e_basic_chat_flow -- --ignored --nocapture"]
fn test_e2e_basic_chat_flow() {
    let _backend = start_backend().expect("Failed to start backend");
    thread::sleep(Duration::from_secs(2));
    let _frontend = start_frontend().expect("Failed to start frontend");
    let _browser = start_browser_server().expect("Failed to start browser");

    let result = run_e2e_test("test_e2e_basic_chat_flow").expect("Test execution failed");

    assert!(result.passed, "Test failed: {:?}", result.error);
}
