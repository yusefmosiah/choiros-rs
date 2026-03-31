use std::fs;
use std::path::PathBuf;

fn read_test_source(file_name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(file_name);
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn assert_ignored(source: &str, file_name: &str, fn_name: &str) {
    let fn_marker = format!("async fn {fn_name}");
    let fn_index = source
        .find(&fn_marker)
        .unwrap_or_else(|| panic!("{file_name} is missing `{fn_marker}`"));
    let attrs = source[..fn_index]
        .lines()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        attrs.contains("#[ignore"),
        "{file_name}:{fn_name} must stay opt-in via #[ignore] so `cargo test --workspace` stays offline-safe"
    );
}

fn assert_suite_documents_opt_in(source: &str, file_name: &str) {
    assert!(
        source.contains("--ignored"),
        "{file_name} should document the explicit `--ignored` opt-in run path"
    );
}

#[test]
fn alm_eval_suites_stay_opt_in() {
    let source = read_test_source("alm_eval_test.rs");
    assert_suite_documents_opt_in(&source, "alm_eval_test.rs");
    for fn_name in [
        "tier1_conductor_bootstrap_eval",
        "tier1_decide_eval",
        "tier1_summarize_changeset_eval",
        "tier2_harness_loop_eval",
        "tier3_conductor_e2e_eval",
    ] {
        assert_ignored(&source, "alm_eval_test.rs", fn_name);
    }
}

#[test]
fn alm_harness_eval_stays_opt_in() {
    let source = read_test_source("alm_harness_eval.rs");
    assert_suite_documents_opt_in(&source, "alm_harness_eval.rs");
    assert_ignored(
        &source,
        "alm_harness_eval.rs",
        "alm_harness_basic_scenarios",
    );
}

#[test]
fn model_provider_live_suites_stay_opt_in() {
    let source = read_test_source("model_provider_live_test.rs");
    assert_suite_documents_opt_in(&source, "model_provider_live_test.rs");
    for fn_name in ["live_provider_smoke_matrix", "live_decide_matrix"] {
        assert_ignored(&source, "model_provider_live_test.rs", fn_name);
    }
}

#[test]
fn sibling_live_eval_suites_stay_opt_in() {
    let dag_eval = read_test_source("dag_eval.rs");
    assert_suite_documents_opt_in(&dag_eval, "dag_eval.rs");
    assert_ignored(&dag_eval, "dag_eval.rs", "dag_runtime_eval");

    let harness_live = read_test_source("harness_live_test.rs");
    assert_suite_documents_opt_in(&harness_live, "harness_live_test.rs");
    for fn_name in [
        "test_spawn_harness_emits_execute_event_to_event_store",
        "test_full_subharness_round_trip_via_alm_port",
    ] {
        assert_ignored(&harness_live, "harness_live_test.rs", fn_name);
    }

    let run_lifecycle = read_test_source("run_lifecycle_e2e_test.rs");
    assert_suite_documents_opt_in(&run_lifecycle, "run_lifecycle_e2e_test.rs");
    for fn_name in [
        "test_live_basic_run_flow_emits_required_milestones",
        "test_live_run_id_is_stable_across_events",
        "test_live_stream_produces_events_before_terminal_state",
        "test_live_concurrent_runs_have_isolated_run_ids",
    ] {
        assert_ignored(&run_lifecycle, "run_lifecycle_e2e_test.rs", fn_name);
    }

    let conductor_e2e = read_test_source("e2e_conductor_scenarios.rs");
    assert_suite_documents_opt_in(&conductor_e2e, "e2e_conductor_scenarios.rs");
    for fn_name in [
        "test_conductor_to_terminal_delegation",
        "test_conductor_to_researcher_delegation",
        "test_conductor_multi_agent_dispatch",
        "test_concurrent_run_isolation",
        "test_conductor_emits_lifecycle_events",
    ] {
        assert_ignored(&conductor_e2e, "e2e_conductor_scenarios.rs", fn_name);
    }
}
