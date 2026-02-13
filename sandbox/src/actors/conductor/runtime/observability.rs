use crate::actors::conductor::actor::ConductorActor;
use crate::actors::conductor::protocol::ConductorError;
use crate::baml_client::types::ConductorDecision;

impl ConductorActor {
    pub(crate) async fn emit_policy_event(
        &self,
        run_id: &str,
        function_name: &str,
        decision: &ConductorDecision,
    ) {
        tracing::info!(
            run_id = %run_id,
            function = %function_name,
            action = %decision.action,
            "Policy decision emitted"
        );
    }

    pub(crate) async fn emit_decision_failure(&self, run_id: &str, error: &str) {
        tracing::error!(
            run_id = %run_id,
            error = %error,
            "Policy decision failed - no deterministic fallback"
        );
    }

    pub(crate) async fn emit_run_complete(
        &self,
        run_id: &str,
        reason: Option<String>,
    ) -> Result<(), ConductorError> {
        tracing::info!(
            run_id = %run_id,
            reason = ?reason,
            "Run completed"
        );
        Ok(())
    }

    pub(crate) async fn emit_run_blocked(
        &self,
        run_id: &str,
        reason: Option<String>,
    ) -> Result<(), ConductorError> {
        tracing::warn!(
            run_id = %run_id,
            reason = ?reason,
            "Run blocked"
        );
        Ok(())
    }
}
