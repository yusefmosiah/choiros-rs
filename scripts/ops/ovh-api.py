#!/usr/bin/env python3
"""OVH API helper for ChoirOS server operations.

Requires: pip install ovh
Env vars: OVH_APPLICATION_KEY, OVH_APPLICATION_SECRET, OVH_CONSUMER_KEY
          (loaded from .env in repo root)

Usage:
  python3 scripts/ops/ovh-api.py list
  python3 scripts/ops/ovh-api.py info <node>
  python3 scripts/ops/ovh-api.py rescue <node>     # Set rescue boot + reboot
  python3 scripts/ops/ovh-api.py normal <node>      # Set normal boot + reboot
  python3 scripts/ops/ovh-api.py reboot <node>      # Hard reboot
  python3 scripts/ops/ovh-api.py status <node>      # Boot status + task info

Where <node> is 'a' or 'b' (or full server name).
"""

import os
import sys
import json
from pathlib import Path

# Load .env from repo root
env_file = Path(__file__).resolve().parent.parent.parent / ".env"
if env_file.exists():
    for line in env_file.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            key, _, value = line.partition("=")
            os.environ.setdefault(key.strip(), value.strip())

import ovh

ENDPOINT = "ovh-us"
SERVERS = {
    "a": "ns1004307.ip-51-81-93.us",
    "b": "ns106285.ip-147-135-70.us",
}
# rescue12-customer is the standard Linux rescue
RESCUE_BOOT_ID = 218949


def get_client():
    return ovh.Client(
        endpoint=ENDPOINT,
        application_key=os.environ["OVH_APPLICATION_KEY"],
        application_secret=os.environ["OVH_APPLICATION_SECRET"],
        consumer_key=os.environ["OVH_CONSUMER_KEY"],
    )


def resolve_server(node):
    if node in SERVERS:
        return SERVERS[node]
    if node in SERVERS.values():
        return node
    print(f"Unknown node: {node}. Use 'a', 'b', or full server name.")
    sys.exit(1)


def cmd_list(client):
    servers = client.get("/dedicated/server")
    for s in servers:
        info = client.get(f"/dedicated/server/{s}")
        print(f"  {s}")
        print(f"    IP: {info.get('ip')}")
        print(f"    Boot ID: {info.get('bootId')}")
        print(f"    State: {info.get('state')}")


def cmd_info(client, node):
    server = resolve_server(node)
    info = client.get(f"/dedicated/server/{server}")
    print(json.dumps(info, indent=2))


def cmd_status(client, node):
    server = resolve_server(node)
    info = client.get(f"/dedicated/server/{server}")
    boot_id = info.get("bootId")

    # Get boot info
    try:
        boot = client.get(f"/dedicated/server/{server}/boot/{boot_id}")
        boot_type = boot.get("bootType", "unknown")
        kernel = boot.get("kernel", "unknown")
    except Exception:
        boot_type = "unknown"
        kernel = "unknown"

    print(f"Server: {server}")
    print(f"IP: {info.get('ip')}")
    print(f"State: {info.get('state')}")
    print(f"Boot: {boot_type} (kernel={kernel}, id={boot_id})")

    # Check recent tasks
    tasks = client.get(f"/dedicated/server/{server}/task", function="hardReboot")
    if tasks:
        latest = client.get(f"/dedicated/server/{server}/task/{tasks[-1]}")
        print(f"Last reboot: status={latest.get('status')}, done={latest.get('doneDate')}")


def cmd_rescue(client, node):
    server = resolve_server(node)
    print(f"Setting {server} to rescue boot...")
    client.put(f"/dedicated/server/{server}", bootId=RESCUE_BOOT_ID)
    print(f"Rebooting {server}...")
    task = client.post(f"/dedicated/server/{server}/reboot")
    print(f"Reboot task: {json.dumps(task, indent=2)}")
    print(
        "\nServer will boot into rescue mode. "
        "OVH sends rescue credentials via email."
    )
    print("SSH: ssh root@<ip> (with rescue password from email)")


def cmd_normal(client, node):
    server = resolve_server(node)
    # Boot ID 1 is typically the normal (harddisk) boot
    print(f"Setting {server} to normal boot...")
    client.put(f"/dedicated/server/{server}", bootId=1)
    print(f"Rebooting {server}...")
    task = client.post(f"/dedicated/server/{server}/reboot")
    print(f"Reboot task: {json.dumps(task, indent=2)}")
    print("\nServer will boot normally from disk.")


def cmd_reboot(client, node):
    server = resolve_server(node)
    print(f"Hard rebooting {server}...")
    task = client.post(f"/dedicated/server/{server}/reboot")
    print(f"Reboot task: {json.dumps(task, indent=2)}")


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    cmd = sys.argv[1]
    client = get_client()

    if cmd == "list":
        cmd_list(client)
    elif cmd == "info" and len(sys.argv) >= 3:
        cmd_info(client, sys.argv[2])
    elif cmd == "status" and len(sys.argv) >= 3:
        cmd_status(client, sys.argv[2])
    elif cmd == "rescue" and len(sys.argv) >= 3:
        cmd_rescue(client, sys.argv[2])
    elif cmd == "normal" and len(sys.argv) >= 3:
        cmd_normal(client, sys.argv[2])
    elif cmd == "reboot" and len(sys.argv) >= 3:
        cmd_reboot(client, sys.argv[2])
    else:
        print(__doc__)
        sys.exit(1)


if __name__ == "__main__":
    main()
