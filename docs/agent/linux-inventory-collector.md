# Linux inventory collector contract

Status: C3-03 source contract `unit-verified` on the local `dev` checkout. This document describes both the bounded parser seam and its current agent integration; supported-host systemd qualification remains blocked until exercised on a named Linux host or VM.

The collector accepts UTF-8 JSON equivalent to `lsblk --json --bytes --output NAME,KNAME,TYPE,SIZE,MODEL,SERIAL,WWN,ROTA,TRAN,RM,RO,PATH,MOUNTPOINTS` and produces `InventorySnapshotV1`. It has no database, canonical resource UUID, or controller dependency. `snapshot_id`, collection time, and host observation key are supplied by the caller.

Bounds are explicit: 256 KiB input, 128 physical-disk entities, 512 bytes per selected string, and JSON depth 16. Empty, malformed, missing-device, oversized, and too-deep inputs fail closed.

Only `type=disk` entries become `physical_disk` observations. loop, RAM, partition, and `dm-*` entries are excluded. Serial and WWN are identity evidence in precedence order; device paths, names, mountpoints, and other runtime values remain observations and never become server resource IDs. The parser emits no network or filesystem side effect.

The command string is exported as `collector::LSBLK_COMMAND`; the agent's bounded process runner executes that fixed command, parses the result, persists a pending snapshot, and uploads it through the enrolled node path. Inventory request serialization is capped at 256 KiB before any network request; the controller independently enforces its 4 MiB authenticated route-body limit. Upload, replay, reconciliation, outage recovery, and service scheduling are documented in the C3-02/C3-03 service and upload contracts; supported-host runtime qualification remains a separate gate.
