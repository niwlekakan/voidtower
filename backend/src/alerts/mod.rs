// Phase 2: Alert management and health check evaluation
// Evaluates thresholds against metrics snapshots
// This module will expose:
//   - evaluate_alerts(metrics) -> Vec<Alert>
//   - list_alerts(pool) -> Vec<Alert>
//   - acknowledge_alert(pool, id, user_id) -> Result<()>
//   - silence_alert(pool, id) -> Result<()>
