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
    # Patch the vendored vhost crate source after cargo vendor runs
    postConfigure = (old.postConfigure or "") + ''
      echo "Patching vendored vhost crate for snapshot/restore fix (rust-vmm/vhost#290)"

      # Find the vendored vhost crate directory
      VHOST_DIR=$(find cargo-vendor-dir* -type d -name "vhost-0.13.0" 2>/dev/null | head -1)
      if [ -z "$VHOST_DIR" ]; then
        echo "WARNING: Could not find vendored vhost-0.13.0, trying alternative paths"
        VHOST_DIR=$(find . -path "*/vhost-0.13.0" -type d 2>/dev/null | head -1)
      fi

      if [ -n "$VHOST_DIR" ]; then
        HANDLER="$VHOST_DIR/src/vhost_user/backend_req_handler.rs"
        if [ -f "$HANDLER" ]; then
          echo "Patching $HANDLER"

          # 1. Remove the protocol_features field declaration
          sed -i '/^    protocol_features: VhostUserProtocolFeatures,$/d' "$HANDLER"

          # 2. Remove the protocol_features initialization
          sed -i '/^            protocol_features: VhostUserProtocolFeatures::empty(),$/d' "$HANDLER"

          # 3. Add REPLY_ACK to get_protocol_features response
          sed -i 's/let features = self\.backend\.get_protocol_features()?;/let features = self.backend.get_protocol_features()? | VhostUserProtocolFeatures::REPLY_ACK;/' "$HANDLER"

          # 4. Remove the self.protocol_features assignment after send_reply_message
          sed -i '/^                self\.protocol_features = features;$/d' "$HANDLER"

          # 5. Fix update_reply_ack_flag: remove the protocol_features check
          sed -i 's/\&\& self\.protocol_features\.contains(pflag)$//' "$HANDLER"

          # Update the vendored checksum so cargo doesn't reject the patched source
          CHECKSUM_FILE="$VHOST_DIR/.cargo-checksum.json"
          if [ -f "$CHECKSUM_FILE" ]; then
            # Set package checksum to empty to skip verification
            sed -i 's/"package":"[^"]*"/"package":null/' "$CHECKSUM_FILE"
          fi

          echo "vhost crate patched successfully"
        else
          echo "ERROR: backend_req_handler.rs not found in $VHOST_DIR"
          exit 1
        fi
      else
        echo "ERROR: vendored vhost crate not found"
        exit 1
      fi
    '';
  });
}
