{
  description = "ChoirOS single-file AWS standup config";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" ];
    in
    (flake-utils.lib.eachSystem systems (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in {
        apps.standup-grind = {
          type = "app";
          program = toString (pkgs.writeShellApplication {
            name = "standup-grind";
            runtimeInputs = with pkgs; [ awscli2 git openssh coreutils gnugrep gawk ];
            text = ''
              set -euo pipefail

              REGION="''${CHOIROS_AWS_REGION:-us-east-1}"
              NAME="''${CHOIROS_GRIND_NAME:-choiros-nixos-grind-01}"
              AMI_ID="''${CHOIROS_AMI_ID:-ami-0f0ba1d9d59df96a5}"
              INSTANCE_TYPE="''${CHOIROS_INSTANCE_TYPE:-t3a.large}"
              KEY_NAME="''${CHOIROS_KEY_NAME:-choiros-production-key}"
              SUBNET_ID="''${CHOIROS_SUBNET_ID:-subnet-0892cfd663695743f}"
              SECURITY_GROUP_ID="''${CHOIROS_SECURITY_GROUP_ID:-sg-03f8fe76f447270db}"
              SSH_KEY_PATH="''${CHOIROS_SSH_KEY_PATH:-$HOME/.ssh/choiros-production.pem}"
              REPO_URL="''${CHOIROS_REPO_URL:-https://github.com/yusefmosiah/choiros-rs.git}"

              if ! aws sts get-caller-identity --output text >/dev/null 2>&1; then
                echo "AWS credentials are not configured for this shell."
                exit 1
              fi

              echo "Checking for existing grind instance: $NAME"
              INSTANCE_ID="$(aws ec2 describe-instances \
                --region "$REGION" \
                --filters "Name=tag:Name,Values=$NAME" \
                          "Name=instance-state-name,Values=pending,running,stopping,stopped" \
                --query 'Reservations[].Instances[0].InstanceId' \
                --output text)"

              if [ "$INSTANCE_ID" = "None" ] || [ -z "$INSTANCE_ID" ]; then
                echo "No active grind instance found; launching one."
                INSTANCE_ID="$(aws ec2 run-instances \
                  --region "$REGION" \
                  --image-id "$AMI_ID" \
                  --instance-type "$INSTANCE_TYPE" \
                  --key-name "$KEY_NAME" \
                  --subnet-id "$SUBNET_ID" \
                  --security-group-ids "$SECURITY_GROUP_ID" \
                  --metadata-options HttpTokens=required,HttpEndpoint=enabled,HttpPutResponseHopLimit=2,HttpProtocolIpv6=disabled,InstanceMetadataTags=disabled \
                  --block-device-mappings '[{"DeviceName":"/dev/xvda","Ebs":{"DeleteOnTermination":true,"VolumeSize":100,"VolumeType":"gp3","Iops":3000,"Throughput":125,"Encrypted":false}}]' \
                  --tag-specifications "ResourceType=instance,Tags=[{Key=Name,Value=$NAME},{Key=Project,Value=choiros},{Key=Env,Value=dev},{Key=Role,Value=grind}]" \
                  --query 'Instances[0].InstanceId' \
                  --output text)"
              else
                STATE="$(aws ec2 describe-instances --region "$REGION" --instance-ids "$INSTANCE_ID" --query 'Reservations[0].Instances[0].State.Name' --output text)"
                echo "Found existing instance $INSTANCE_ID in state $STATE"
                if [ "$STATE" = "stopped" ]; then
                  aws ec2 start-instances --region "$REGION" --instance-ids "$INSTANCE_ID" >/dev/null
                fi
              fi

              echo "Waiting for instance $INSTANCE_ID to become running"
              aws ec2 wait instance-running --region "$REGION" --instance-ids "$INSTANCE_ID"

              PUBLIC_IP="$(aws ec2 describe-instances \
                --region "$REGION" \
                --instance-ids "$INSTANCE_ID" \
                --query 'Reservations[0].Instances[0].PublicIpAddress' \
                --output text)"

              echo "Grind host public IP: $PUBLIC_IP"
              echo "Syncing repository on host to origin/main"

              if [ ! -f "$SSH_KEY_PATH" ]; then
                echo "SSH key not found at $SSH_KEY_PATH"
                exit 1
              fi

              ssh -i "$SSH_KEY_PATH" -o StrictHostKeyChecking=accept-new "root@$PUBLIC_IP" '
                set -euo pipefail
                if ! command -v git >/dev/null 2>&1; then
                  nix --extra-experimental-features "nix-command flakes" profile install nixpkgs#git
                fi
                mkdir -p /opt/choiros
                if [ ! -d /opt/choiros/workspace/.git ]; then
                  git clone "'"$REPO_URL"'" /opt/choiros/workspace
                fi
                git -C /opt/choiros/workspace fetch origin main
                git -C /opt/choiros/workspace checkout main
                git -C /opt/choiros/workspace pull --ff-only origin main
                printf "REMOTE_HEAD=%s\n" "$(git -C /opt/choiros/workspace rev-parse --short HEAD)"
              '

              echo
              echo "Grind host is ready:"
              echo "  ssh -i \"$SSH_KEY_PATH\" root@$PUBLIC_IP"
              echo "  cd /opt/choiros/workspace"
            '';
          });
        };
      }))
    // {
      choiros.aws = {
        region = "us-east-1";
        prod = {
          name = "choiros-nixos-prod-01";
          instance_id = "i-0cb76dd46cb699be6";
        };
        grind = {
          name = "choiros-nixos-grind-01";
          instance_id = "i-02d54052ca6dd4b39";
        };
        baseline = {
          ami_id = "ami-0f0ba1d9d59df96a5";
          instance_type = "t3a.large";
          subnet_id = "subnet-0892cfd663695743f";
          security_group_id = "sg-03f8fe76f447270db";
          key_name = "choiros-production-key";
        };
      };
    };
}
