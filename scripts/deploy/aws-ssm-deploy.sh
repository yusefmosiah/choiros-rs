#!/usr/bin/env bash
set -euo pipefail

# Runs from local/CI.
# Sends scripts/deploy/host-switch.sh to AWS SSM target instance.

AWS_REGION="${AWS_REGION:-${CHOIROS_AWS_REGION:-us-east-1}}"
DEPLOY_INSTANCE_ID="${DEPLOY_INSTANCE_ID:-${EC2_INSTANCE_ID:-}}"
DEPLOY_SHA="${DEPLOY_SHA:-$(git rev-parse HEAD)}"
WORKDIR="${WORKDIR:-/opt/choiros/deploy-repo}"

SANDBOX_STORE_PATH="${SANDBOX_STORE_PATH:-}"
HYPERVISOR_STORE_PATH="${HYPERVISOR_STORE_PATH:-}"
DESKTOP_STORE_PATH="${DESKTOP_STORE_PATH:-}"

if ! command -v aws >/dev/null 2>&1; then
  echo "error: aws CLI is required"
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required"
  exit 2
fi

if [[ -z "${DEPLOY_INSTANCE_ID}" ]]; then
  echo "error: DEPLOY_INSTANCE_ID (or EC2_INSTANCE_ID) is required"
  exit 2
fi

SCRIPT_PATH="$(dirname "$0")/host-switch.sh"
if [[ ! -f "${SCRIPT_PATH}" ]]; then
  echo "error: missing ${SCRIPT_PATH}"
  exit 2
fi

aws ssm describe-instance-information \
  --region "${AWS_REGION}" \
  --filters "Key=InstanceIds,Values=${DEPLOY_INSTANCE_ID}" \
  --query 'InstanceInformationList[0].PingStatus' \
  --output text | grep -q '^Online$'

SCRIPT_B64="$(base64 < "${SCRIPT_PATH}" | tr -d '\n')"
REMOTE_CMD="echo '${SCRIPT_B64}' | base64 -d > /tmp/choiros-host-switch.sh && chmod +x /tmp/choiros-host-switch.sh && RELEASE_SHA='${DEPLOY_SHA}' WORKDIR='${WORKDIR}' SANDBOX_STORE_PATH='${SANDBOX_STORE_PATH}' HYPERVISOR_STORE_PATH='${HYPERVISOR_STORE_PATH}' DESKTOP_STORE_PATH='${DESKTOP_STORE_PATH}' bash /tmp/choiros-host-switch.sh"

COMMAND_ID="$(aws ssm send-command \
  --region "${AWS_REGION}" \
  --instance-ids "${DEPLOY_INSTANCE_ID}" \
  --document-name 'AWS-RunShellScript' \
  --comment "choiros deploy ${DEPLOY_SHA}" \
  --parameters "$(jq -cn --arg cmd "${REMOTE_CMD}" '{commands: [$cmd]}')" \
  --query 'Command.CommandId' \
  --output text)"

STATUS=""
for _ in $(seq 1 120); do
  STATUS="$(aws ssm get-command-invocation \
    --region "${AWS_REGION}" \
    --command-id "${COMMAND_ID}" \
    --instance-id "${DEPLOY_INSTANCE_ID}" \
    --query 'Status' \
    --output text 2>/dev/null || true)"

  case "${STATUS}" in
    Success)
      break
      ;;
    Failed|Cancelled|TimedOut)
      echo "SSM command ended with status: ${STATUS}"
      aws ssm get-command-invocation \
        --region "${AWS_REGION}" \
        --command-id "${COMMAND_ID}" \
        --instance-id "${DEPLOY_INSTANCE_ID}" \
        --output json
      exit 1
      ;;
    *)
      sleep 10
      ;;
  esac
done

if [[ "${STATUS}" != "Success" ]]; then
  echo "SSM command did not reach Success within timeout window (last status: ${STATUS:-unknown})"
  aws ssm get-command-invocation \
    --region "${AWS_REGION}" \
    --command-id "${COMMAND_ID}" \
    --instance-id "${DEPLOY_INSTANCE_ID}" \
    --output json || true
  exit 1
fi

aws ssm get-command-invocation \
  --region "${AWS_REGION}" \
  --command-id "${COMMAND_ID}" \
  --instance-id "${DEPLOY_INSTANCE_ID}" \
  --query '{Status:Status,StdOut:StandardOutputContent,StdErr:StandardErrorContent}' \
  --output json
