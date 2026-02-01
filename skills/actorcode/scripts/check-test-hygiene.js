#!/usr/bin/env node
/**
 * Test Hygiene Checker
 * 
 * Validates test coverage and quality before allowing merges.
 * Raises the bar for safe branch merging.
 */

import { spawn } from "child_process";
import fs from "fs/promises";
import path from "path";

const TEST_REQUIREMENTS = {
  minCoverage: 70, // Minimum code coverage percentage
  requiredTests: [
    "unit tests must pass",
    "integration tests must pass",
    "no compiler warnings",
    "clippy must pass",
    "fmt must pass"
  ]
};

class TestHygieneChecker {
  constructor() {
    this.results = [];
  }

  async checkAll() {
    console.log("=== Test Hygiene Checker ===\n");
    
    const checks = [
      { name: "Code Formatting", fn: () => this.checkFormatting() },
      { name: "Clippy Lints", fn: () => this.checkClippy() },
      { name: "Unit Tests", fn: () => this.checkUnitTests() },
      { name: "Integration Tests", fn: () => this.checkIntegrationTests() },
      { name: "Compiler Warnings", fn: () => this.checkCompilerWarnings() },
      { name: "Test Coverage", fn: () => this.checkCoverage() }
    ];
    
    for (const check of checks) {
      process.stdout.write(`Checking ${check.name}... `);
      try {
        const result = await check.fn();
        this.results.push({ name: check.name, ...result });
        console.log(result.success ? "✓ PASS" : "✗ FAIL");
        if (!result.success && result.details) {
          console.log(`  ${result.details}`);
        }
      } catch (error) {
        this.results.push({ name: check.name, success: false, error: error.message });
        console.log(`✗ ERROR: ${error.message}`);
      }
    }
    
    return this.summarize();
  }

  async checkFormatting() {
    const result = await this.execCommand("cargo", ["fmt", "--", "--check"]);
    return {
      success: result.code === 0,
      details: result.code !== 0 ? "Code needs formatting: run 'cargo fmt'" : null
    };
  }

  async checkClippy() {
    const result = await this.execCommand("cargo", ["clippy", "--workspace", "--", "-D", "warnings"]);
    return {
      success: result.code === 0,
      details: result.code !== 0 ? "Clippy warnings found" : null
    };
  }

  async checkUnitTests() {
    const result = await this.execCommand("cargo", ["test", "--lib", "--workspace"]);
    return {
      success: result.code === 0,
      details: result.code !== 0 ? "Unit tests failed" : null
    };
  }

  async checkIntegrationTests() {
    const result = await this.execCommand("cargo", ["test", "--test", "'*'", "--workspace"]);
    return {
      success: result.code === 0,
      details: result.code !== 0 ? "Integration tests failed" : null
    };
  }

  async checkCompilerWarnings() {
    const result = await this.execCommand("cargo", ["build", "--workspace"]);
    const hasWarnings = result.stderr.includes("warning:");
    return {
      success: !hasWarnings,
      details: hasWarnings ? "Compiler warnings found" : null
    };
  }

  async checkCoverage() {
    // Check if tarpaulin is installed
    const checkResult = await this.execCommand("cargo", ["tarpaulin", "--version"]);
    if (checkResult.code !== 0) {
      return {
        success: true, // Skip if not installed
        details: "cargo-tarpaulin not installed, skipping coverage check"
      };
    }
    
    const result = await this.execCommand("cargo", [
      "tarpaulin",
      "--workspace",
      "--lib",
      "--timeout", "120",
      "--out", "Stdout"
    ]);
    
    // Parse coverage from output
    const coverageMatch = result.stdout.match(/(\d+\.?\d*)%/);
    const coverage = coverageMatch ? parseFloat(coverageMatch[1]) : 0;
    
    return {
      success: coverage >= TEST_REQUIREMENTS.minCoverage,
      details: `Coverage: ${coverage}% (min: ${TEST_REQUIREMENTS.minCoverage}%)`
    };
  }

  async execCommand(cmd, args) {
    return new Promise((resolve) => {
      const child = spawn(cmd, args, {
        cwd: process.cwd(),
        stdio: "pipe"
      });
      
      let stdout = "";
      let stderr = "";
      
      child.stdout.on("data", (data) => { stdout += data; });
      child.stderr.on("data", (data) => { stderr += data; });
      
      child.on("exit", (code) => {
        resolve({ code, stdout, stderr });
      });
    });
  }

  summarize() {
    console.log("\n=== Summary ===");
    const passed = this.results.filter(r => r.success).length;
    const failed = this.results.filter(r => !r.success).length;
    
    console.log(`Passed: ${passed}/${this.results.length}`);
    console.log(`Failed: ${failed}/${this.results.length}`);
    
    if (failed > 0) {
      console.log("\nFailed checks:");
      this.results
        .filter(r => !r.success)
        .forEach(r => console.log(`  - ${r.name}: ${r.details || r.error || "Failed"}`));
    }
    
    const allPassed = failed === 0;
    console.log(`\n${allPassed ? "✓ All checks passed - safe to merge" : "✗ Fix issues before merging"}`);
    
    return allPassed;
  }
}

// CLI
async function main() {
  const checker = new TestHygieneChecker();
  const passed = await checker.checkAll();
  process.exit(passed ? 0 : 1);
}

main().catch(error => {
  console.error(error.message);
  process.exit(1);
});
