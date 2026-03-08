# Overlay: patch virtiofsd's vendored vhost crate to fix snapshot/restore
# Bug: cloud-hypervisor/cloud-hypervisor#6931
# Fix: rust-vmm/vhost#290 (merged in vhost 0.14.0, but virtiofsd still pins 0.13.0)
#
# The vhost crate's update_reply_ack_flag() checks self.protocol_features
# which is only populated by GET_PROTOCOL_FEATURES. During snapshot restore,
# cloud-hypervisor skips that call (features remembered from snapshot), so
# REPLY_ACK never gets enabled, and cloud-hypervisor hangs waiting for a
# reply that virtiofsd never sends.
final: prev: {
  virtiofsd = prev.virtiofsd.overrideAttrs (old: {
    postConfigure = (old.postConfigure or "") + ''
      echo "Patching vendored vhost crate for snapshot/restore fix (rust-vmm/vhost#290)"

      # Find the backend_req_handler.rs file in vendored vhost crate
      HANDLER=$(find /build -name "backend_req_handler.rs" -path "*/vhost_user/*" 2>/dev/null | head -1)

      if [ -z "$HANDLER" ]; then
        echo "ERROR: backend_req_handler.rs not found in vendored dependencies"
        echo "Build directory contents:"
        find /build -maxdepth 4 -type d | head -30
        exit 1
      fi

      echo "Found handler at: $HANDLER"
      VHOST_DIR=$(dirname "$(dirname "$(dirname "$HANDLER")")")

      # 1. Remove the protocol_features field declaration
      sed -i '/^    protocol_features: VhostUserProtocolFeatures,$/d' "$HANDLER"

      # 2. Remove the protocol_features initialization
      sed -i '/^            protocol_features: VhostUserProtocolFeatures::empty(),$/d' "$HANDLER"

      # 3. Add REPLY_ACK to get_protocol_features response
      sed -i 's/let features = self\.backend\.get_protocol_features()?;/let features = self.backend.get_protocol_features()? | VhostUserProtocolFeatures::REPLY_ACK;/' "$HANDLER"

      # 4. Remove the self.protocol_features assignment after send_reply_message
      sed -i '/^                self\.protocol_features = features;$/d' "$HANDLER"

      # 5. Fix update_reply_ack_flag: remove the protocol_features check
      sed -i '/&& self\.protocol_features\.contains(pflag)$/d' "$HANDLER"

      # Update the vendored checksum so cargo doesn't reject the patched source
      CHECKSUM_FILE="$VHOST_DIR/.cargo-checksum.json"
      if [ -f "$CHECKSUM_FILE" ]; then
        sed -i 's/"package":"[^"]*"/"package":null/' "$CHECKSUM_FILE"
      fi

      echo "vhost crate patched successfully"
    '';
  });
}
