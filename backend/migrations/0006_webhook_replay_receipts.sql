-- Signed inbound webhook delivery identities. Receipts are bounded by the
-- handler's retention policy and the source/nonce key makes first observation
-- atomic across duplicate deliveries and process restarts.
CREATE TABLE IF NOT EXISTS webhook_replay_receipts (
    source_id   TEXT NOT NULL,
    nonce       TEXT NOT NULL,
    timestamp   INTEGER NOT NULL,
    signature   TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (source_id, nonce)
);

CREATE INDEX IF NOT EXISTS idx_webhook_replay_receipts_created_at
    ON webhook_replay_receipts (created_at);
