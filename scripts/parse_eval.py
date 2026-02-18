#!/usr/bin/env python3
"""Parse RLM eval test output into a clean summary table.

Usage:
  cargo test -p sandbox --test rlm_eval_test -- --nocapture 2>&1 | python3 scripts/parse_eval.py
  python3 scripts/parse_eval.py < eval_output.log
  python3 scripts/parse_eval.py eval_output.log
"""

import sys
import re
from collections import defaultdict

# ─── Patterns ─────────────────────────────────────────────────────────────────

TIER_HEADER = re.compile(r"^=== (TIER .+?) ===$")
RESULT_LINE = re.compile(
    r"^\s*\[(PASS|FAIL|MARGINAL)\]\s+(\S+)\s+/\s+(\S+)\s+\((\d+)ms\)\s+--\s+(.*)$"
)
SUMMARY_HEADER = re.compile(r"^--- (.+?) Summary ---$")
SUMMARY_TOTALS = re.compile(
    r"^\s*total=(\d+)\s+pass=(\d+)\s+marginal=(\d+)\s+fail=(\d+)\s+avg_latency=(\d+)ms"
)
MODEL_SUMMARY = re.compile(r"^\s*(\S+):\s+(\d+)/(\d+)\s+pass,\s+avg\s+(\d+)ms")
HARNESS_RUN = re.compile(r"^\s*running\s+(\S+)\s+/\s+(\S+)\.\.\.")
SAMPLED = re.compile(r"^\s*sampled models:\s+(.+)$")
SKIPPED = re.compile(r"^\s*skipped:\s+(.+)$")
TEST_RESULT = re.compile(r"^test (\S+) \.\.\. (ok|FAILED)")


BOOTSTRAP_SCENARIOS = {
    "simple_greeting", "research_and_write", "code_task",
    "no_capabilities", "ping",
}
DECIDE_SCENARIOS = {
    "bash_simple", "web_search", "file_read", "message_parent",
}
CHANGESET_SCENARIOS = {
    "minor_typo_fix", "new_section", "first_version",
}
HARNESS_SCENARIOS = {
    "bash_echo", "file_create_read", "multi_step",
}
E2E_SCENARIOS = {
    "greeting", "research_task", "code_analysis",
}


def infer_tier(model, scenario, fallback):
    """Infer tier from scenario name since output interleaves across threads."""
    if scenario in BOOTSTRAP_SCENARIOS:
        return "TIER 1.1: ConductorBootstrapAgenda"
    if scenario in DECIDE_SCENARIOS:
        return "TIER 1.2: Decide (tool use)"
    if scenario in CHANGESET_SCENARIOS:
        return "TIER 1.3: SummarizeChangeset"
    if scenario in HARNESS_SCENARIOS:
        return "TIER 2: Full AgentHarness loop"
    if scenario in E2E_SCENARIOS or model == "server-default":
        return "TIER 3: End-to-end /conductor/execute"
    return fallback or "?"


def main():
    if len(sys.argv) > 1 and sys.argv[1] != "-":
        with open(sys.argv[1]) as f:
            lines = f.readlines()
    else:
        lines = sys.stdin.readlines()

    tiers = []
    current_tier = None
    results = []
    tier_summaries = []
    model_summaries = defaultdict(list)  # tier_name -> [(model, pass, total, avg_ms)]
    test_outcomes = []

    for line in lines:
        line = line.rstrip("\n")

        m = TIER_HEADER.match(line)
        if m:
            current_tier = m.group(1)
            tiers.append(current_tier)
            continue

        m = RESULT_LINE.match(line)
        if m:
            grade, model, scenario, latency, detail = m.groups()
            # Infer tier from scenario name since output interleaves across threads
            tier = infer_tier(model, scenario, current_tier)
            results.append({
                "tier": tier,
                "grade": grade,
                "model": model,
                "scenario": scenario,
                "latency_ms": int(latency),
                "detail": detail,
            })
            continue

        m = SUMMARY_HEADER.match(line)
        if m:
            tier_summaries.append({"name": m.group(1)})
            continue

        m = SUMMARY_TOTALS.match(line)
        if m:
            total, passed, marginal, fail, avg = m.groups()
            if tier_summaries:
                tier_summaries[-1].update({
                    "total": int(total),
                    "pass": int(passed),
                    "marginal": int(marginal),
                    "fail": int(fail),
                    "avg_latency_ms": int(avg),
                })
            continue

        m = MODEL_SUMMARY.match(line)
        if m:
            model, passed, total, avg = m.groups()
            if tier_summaries:
                key = tier_summaries[-1].get("name", "?")
                model_summaries[key].append({
                    "model": model,
                    "pass": int(passed),
                    "total": int(total),
                    "avg_ms": int(avg),
                })
            continue

        m = TEST_RESULT.match(line)
        if m:
            test_outcomes.append((m.group(1), m.group(2)))
            continue

    # ─── Output ───────────────────────────────────────────────────────────────

    # Canonical tier order (output interleaves, so use a fixed ordering)
    TIER_ORDER = [
        "TIER 1.1: ConductorBootstrapAgenda",
        "TIER 1.2: Decide (tool use)",
        "TIER 1.3: SummarizeChangeset",
        "TIER 2: Full AgentHarness loop",
        "TIER 3: End-to-end /conductor/execute",
    ]
    # Merge discovered tiers with canonical order
    ordered_tiers = [t for t in TIER_ORDER if t in set(r["tier"] for r in results)]
    extra = [t for t in tiers if t not in TIER_ORDER and t in set(r["tier"] for r in results)]
    ordered_tiers.extend(extra)

    print()
    print("=" * 80)
    print("  RLM EVALUATION RESULTS")
    print("=" * 80)

    # Group results by tier
    by_tier = defaultdict(list)
    for r in results:
        by_tier[r["tier"]].append(r)

    for tier in ordered_tiers:
        tier_results = by_tier.get(tier, [])
        if not tier_results:
            print(f"\n  {tier}: (no results)")
            continue

        print(f"\n  {tier}")
        print("  " + "-" * 76)

        # Per-model table
        models = sorted(set(r["model"] for r in tier_results))
        scenarios = sorted(set(r["scenario"] for r in tier_results))

        # Header
        model_col = max(len(m) for m in models) if models else 10
        print(f"  {'model':<{model_col}}  ", end="")
        for s in scenarios:
            print(f"{s[:16]:>16}  ", end="")
        print(f"{'avg_ms':>8}")

        # Rows
        for model in models:
            print(f"  {model:<{model_col}}  ", end="")
            model_results = [r for r in tier_results if r["model"] == model]
            latencies = []
            for s in scenarios:
                match = [r for r in model_results if r["scenario"] == s]
                if match:
                    r = match[0]
                    latencies.append(r["latency_ms"])
                    symbol = {
                        "PASS": "  PASS",
                        "MARGINAL": "  MARG",
                        "FAIL": " *FAIL",
                    }.get(r["grade"], "   ???")
                    cell = f"{symbol} {r['latency_ms']:>5}ms"
                    print(f"{cell:>16}  ", end="")
                else:
                    print(f"{'---':>16}  ", end="")
            avg = sum(latencies) // len(latencies) if latencies else 0
            print(f"{avg:>7}ms")

        # Tier totals
        tier_pass = sum(1 for r in tier_results if r["grade"] == "PASS")
        tier_marg = sum(1 for r in tier_results if r["grade"] == "MARGINAL")
        tier_fail = sum(1 for r in tier_results if r["grade"] == "FAIL")
        tier_total = len(tier_results)
        print(f"\n  totals: {tier_pass}/{tier_total} pass, {tier_marg} marginal, {tier_fail} fail")

    # ─── Failure details ──────────────────────────────────────────────────────

    failures = [r for r in results if r["grade"] == "FAIL"]
    marginals = [r for r in results if r["grade"] == "MARGINAL"]

    if failures:
        print(f"\n{'=' * 80}")
        print(f"  FAILURES ({len(failures)})")
        print(f"{'=' * 80}")
        for r in failures:
            print(f"  [{r['model']}] {r['scenario']}: {r['detail'][:120]}")

    if marginals:
        print(f"\n{'=' * 80}")
        print(f"  MARGINALS ({len(marginals)})")
        print(f"{'=' * 80}")
        for r in marginals:
            print(f"  [{r['model']}] {r['scenario']}: {r['detail'][:120]}")

    # ─── Overall summary ──────────────────────────────────────────────────────

    total = len(results)
    total_pass = sum(1 for r in results if r["grade"] == "PASS")
    total_marg = sum(1 for r in results if r["grade"] == "MARGINAL")
    total_fail = sum(1 for r in results if r["grade"] == "FAIL")

    print(f"\n{'=' * 80}")
    print(f"  OVERALL: {total_pass}/{total} pass, {total_marg} marginal, {total_fail} fail")

    # Per-model overall
    all_models = sorted(set(r["model"] for r in results))
    for model in all_models:
        mr = [r for r in results if r["model"] == model]
        mp = sum(1 for r in mr if r["grade"] == "PASS")
        mt = len(mr)
        ml = sum(r["latency_ms"] for r in mr) // mt if mt else 0
        pct = 100 * mp // mt if mt else 0
        print(f"  {model}: {mp}/{mt} ({pct}%) avg {ml}ms")

    # Test runner outcomes
    if test_outcomes:
        print(f"\n  test runner: {len([t for _, t in test_outcomes if t == 'ok'])} ok, "
              f"{len([t for _, t in test_outcomes if t == 'FAILED'])} failed")

    print(f"{'=' * 80}")
    print()


if __name__ == "__main__":
    main()
