# Linux inventory collector contract

Status: C3-01 `unit-verified` on the local `dev` checkout. This document describes the bounded, side-effect-free parser seam; it does not claim that the agent process invokes `lsblk` or uploads snapshots.

The collector accepts UTF-8 JSON equivalent to `lsblk --json --bytes --output NAME,KNAME,TYPE,SIZE,MODEL,SERIAL,WWN,ROTA,TRAN,RM,RO,PATH,MOUNTPOINTS` and produces `InventorySnapshotV1`. It has no database, canonical resource UUID, or controller dependency. `snapshot_id`, collection time, and host observation key are supplied by the caller.

Bounds are explicit: 256 KiB input, 128 physical-disk entities, 512 bytes per selected string, and JSON depth 16. Empty, malformed, missing-device, oversized, and too-deep inputs fail closed.

Only `type=disk` entries become `physical_disk` observations. loop, RAM, partition, and `dm-*` entries are excluded. Serial and WWN are identity evidence in precedence order; device paths, names, mountpoints, and other runtime values remain observations and never become server resource IDs. The parser emits no network or filesystem side effect.

The command string is exported as `collector::LSBLK_COMMAND` for the future bounded process runner. Upload, replay, reconciliation, outage recovery, and service scheduling are C3-02/C3-03 work and remain out of scope here.
