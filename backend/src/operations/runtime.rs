use super::{
    adapters::AdapterRegistry,
    approvals,
    clock::{Clock, SystemClock},
    contracts::JobState,
    worker::{self, ClaimedJob, ClaimedStep},
};
use crate::config::OperationRuntimeConfig;
use anyhow::Result;
use sqlx::SqlitePool;
use std::{fmt, sync::Arc, time::Duration};
use tokio::{
    sync::watch,
    task::JoinSet,
    time::{Instant, MissedTickBehavior},
};

#[derive(Clone)]
pub struct RuntimeControl {
    cancelled: watch::Sender<bool>,
}

impl RuntimeControl {
    pub(crate) fn new() -> Self {
        let (cancelled, _) = watch::channel(false);
        Self { cancelled }
    }

    pub fn cancel(&self) {
        self.cancelled.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.cancelled.borrow()
    }

    async fn cancelled(&self) {
        let mut receiver = self.cancelled.subscribe();
        if *receiver.borrow() {
            return;
        }
        while receiver.changed().await.is_ok() {
            if *receiver.borrow() {
                return;
            }
        }
    }
}

impl fmt::Debug for RuntimeControl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeControl")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

pub struct PreparedOperationRuntime {
    pool: SqlitePool,
    adapters: Arc<AdapterRegistry>,
    config: OperationRuntimeConfig,
    clock: Arc<dyn Clock>,
    control: RuntimeControl,
    process_id: String,
}

impl PreparedOperationRuntime {
    pub fn control(&self) -> RuntimeControl {
        self.control.clone()
    }

    pub fn start(self) -> OperationRuntime {
        let mut tasks = JoinSet::new();
        for index in 0..self.config.worker_count {
            let pool = self.pool.clone();
            let adapters = self.adapters.clone();
            let config = self.config.clone();
            let clock = self.clock.clone();
            let control = self.control.clone();
            let worker_id = format!("voidtower:{}:worker:{index}", self.process_id);
            tasks.spawn(async move {
                run_execution_loop(pool, adapters, config, clock, control, worker_id).await;
            });
        }

        let pool = self.pool;
        let adapters = self.adapters;
        let config = self.config.clone();
        let clock = self.clock;
        let control = self.control.clone();
        let worker_id = format!("voidtower:{}:reconciler:0", self.process_id);
        tasks.spawn(async move {
            run_reconciliation_loop(pool, adapters, config, clock, control, worker_id).await;
        });

        OperationRuntime {
            control: self.control,
            tasks,
            shutdown_timeout: self.config.shutdown_timeout(),
        }
    }
}

pub struct OperationRuntime {
    control: RuntimeControl,
    tasks: JoinSet<()>,
    shutdown_timeout: Duration,
}

impl OperationRuntime {
    pub fn control(&self) -> RuntimeControl {
        self.control.clone()
    }

    pub async fn shutdown(mut self) -> bool {
        self.control.cancel();
        let deadline = tokio::time::sleep(self.shutdown_timeout);
        tokio::pin!(deadline);
        let mut clean = true;
        loop {
            tokio::select! {
                joined = self.tasks.join_next() => {
                    match joined {
                        Some(Ok(())) => {}
                        Some(Err(_)) => {
                            clean = false;
                            tracing::warn!(error_code = "operation_runtime_task_failed");
                        }
                        None => return clean,
                    }
                }
                () = &mut deadline => {
                    tracing::warn!(error_code = "operation_runtime_shutdown_timeout");
                    self.tasks.abort_all();
                    while self.tasks.join_next().await.is_some() {}
                    return false;
                }
            }
        }
    }
}

pub async fn prepare(
    pool: SqlitePool,
    adapters: Arc<AdapterRegistry>,
    config: OperationRuntimeConfig,
) -> Result<PreparedOperationRuntime> {
    prepare_with_clock(pool, adapters, config, Arc::new(SystemClock)).await
}

pub async fn prepare_with_clock(
    pool: SqlitePool,
    adapters: Arc<AdapterRegistry>,
    config: OperationRuntimeConfig,
    clock: Arc<dyn Clock>,
) -> Result<PreparedOperationRuntime> {
    config.validate()?;
    adapters.validate_complete()?;
    let now = clock.now();
    let expired_approvals = approvals::expire_pending(&pool, now).await?;
    let recovered_jobs = worker::recover_expired(&pool, now).await?;
    tracing::info!(
        event_code = "operation_runtime_prepared",
        expired_approvals,
        recovered_jobs
    );
    Ok(PreparedOperationRuntime {
        pool,
        adapters,
        config,
        clock,
        control: RuntimeControl::new(),
        process_id: uuid::Uuid::new_v4().to_string(),
    })
}

async fn run_execution_loop(
    pool: SqlitePool,
    adapters: Arc<AdapterRegistry>,
    config: OperationRuntimeConfig,
    clock: Arc<dyn Clock>,
    control: RuntimeControl,
    worker_id: String,
) {
    let lease_seconds = i64::try_from(config.lease_seconds).expect("validated lease duration");
    let mut backoff =
        ErrorBackoff::new(config.idle_poll_interval(), config.maximum_error_backoff());
    loop {
        if control.is_cancelled() {
            return;
        }
        match worker::claim_next(&pool, &adapters, &worker_id, clock.now(), lease_seconds).await {
            Ok(Some(job)) => {
                backoff.reset();
                if run_claimed_job(
                    &pool,
                    &adapters,
                    &config,
                    clock.as_ref(),
                    &control,
                    &worker_id,
                    job,
                )
                .await
                .is_err()
                {
                    tracing::warn!(
                        error_code = "operation_job_supervision_failed",
                        worker_id = %worker_id
                    );
                    if wait_or_cancel(&control, backoff.next_delay()).await {
                        return;
                    }
                }
            }
            Ok(None) => {
                backoff.reset();
                if wait_or_cancel(&control, config.idle_poll_interval()).await {
                    return;
                }
            }
            Err(_) => {
                tracing::warn!(
                    error_code = "operation_claim_failed",
                    worker_id = %worker_id
                );
                if wait_or_cancel(&control, backoff.next_delay()).await {
                    return;
                }
            }
        }
    }
}

async fn run_claimed_job(
    pool: &SqlitePool,
    adapters: &AdapterRegistry,
    config: &OperationRuntimeConfig,
    clock: &dyn Clock,
    control: &RuntimeControl,
    worker_id: &str,
    job: ClaimedJob,
) -> Result<()> {
    let lease_seconds = i64::try_from(config.lease_seconds).expect("validated lease duration");
    loop {
        if control.is_cancelled() {
            let _ = worker::release_claimed_job(pool, &job.id, worker_id, clock.now()).await?;
            return Ok(());
        }
        let Some(step) =
            worker::claim_step(pool, &job.id, worker_id, clock.now(), lease_seconds).await?
        else {
            return Ok(());
        };
        if control.is_cancelled() {
            let _ =
                worker::release_step_before_execution(pool, &step, worker_id, clock.now()).await?;
            return Ok(());
        }
        let state =
            supervise_execution(pool, adapters, &job, &step, worker_id, clock, config).await?;
        if state != JobState::Running {
            return Ok(());
        }
    }
}

async fn supervise_execution(
    pool: &SqlitePool,
    adapters: &AdapterRegistry,
    job: &ClaimedJob,
    step: &ClaimedStep,
    worker_id: &str,
    clock: &dyn Clock,
    config: &OperationRuntimeConfig,
) -> Result<JobState> {
    let execution = worker::execute_claimed_step(pool, adapters, job, step, worker_id, clock);
    tokio::pin!(execution);
    let mut renewal = tokio::time::interval_at(
        Instant::now() + config.lease_renew_interval(),
        config.lease_renew_interval(),
    );
    renewal.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let lease_seconds = i64::try_from(config.lease_seconds).expect("validated lease duration");
    loop {
        tokio::select! {
            result = &mut execution => return result,
            _ = renewal.tick() => {
                match worker::renew_lease(pool, &job.id, worker_id, clock.now(), lease_seconds).await {
                    Ok(true) => {}
                    Ok(false) => {
                        tracing::warn!(
                            error_code = "operation_lease_ownership_lost",
                            worker_id = %worker_id,
                            job_id = %job.id,
                            action = %job.action
                        );
                        return execution.await;
                    }
                    Err(_) => tracing::warn!(
                        error_code = "operation_lease_renewal_failed",
                        worker_id = %worker_id,
                        job_id = %job.id,
                        action = %job.action
                    ),
                }
            }
        }
    }
}

async fn run_reconciliation_loop(
    pool: SqlitePool,
    adapters: Arc<AdapterRegistry>,
    config: OperationRuntimeConfig,
    clock: Arc<dyn Clock>,
    control: RuntimeControl,
    worker_id: String,
) {
    let lease_seconds = i64::try_from(config.lease_seconds).expect("validated lease duration");
    let mut backoff =
        ErrorBackoff::new(config.idle_poll_interval(), config.maximum_error_backoff());
    let mut first_cycle = true;
    loop {
        if !first_cycle && wait_or_cancel(&control, config.reconciliation_poll_interval()).await {
            return;
        }
        first_cycle = false;
        if control.is_cancelled() {
            return;
        }
        let cycle = async {
            worker::recover_expired(&pool, clock.now()).await?;
            let claim = worker::claim_reconciliation(
                &pool,
                &adapters,
                &worker_id,
                clock.now(),
                lease_seconds,
            )
            .await?;
            if let Some((job, step)) = claim {
                if control.is_cancelled() {
                    let _ = worker::release_reconciliation_before_verification(
                        &pool,
                        &step,
                        &worker_id,
                        clock.now(),
                    )
                    .await?;
                } else {
                    supervise_reconciliation(
                        &pool,
                        &adapters,
                        &job,
                        &step,
                        &worker_id,
                        clock.as_ref(),
                        &config,
                    )
                    .await?;
                }
            }
            Result::<()>::Ok(())
        }
        .await;
        match cycle {
            Ok(()) => backoff.reset(),
            Err(_) => {
                tracing::warn!(
                    error_code = "operation_reconciliation_cycle_failed",
                    worker_id = %worker_id
                );
                if wait_or_cancel(&control, backoff.next_delay()).await {
                    return;
                }
            }
        }
    }
}

async fn supervise_reconciliation(
    pool: &SqlitePool,
    adapters: &AdapterRegistry,
    job: &ClaimedJob,
    step: &ClaimedStep,
    worker_id: &str,
    clock: &dyn Clock,
    config: &OperationRuntimeConfig,
) -> Result<JobState> {
    let reconciliation =
        worker::reconcile_claimed_step(pool, adapters, job, step, worker_id, clock);
    tokio::pin!(reconciliation);
    let mut renewal = tokio::time::interval_at(
        Instant::now() + config.lease_renew_interval(),
        config.lease_renew_interval(),
    );
    renewal.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let lease_seconds = i64::try_from(config.lease_seconds).expect("validated lease duration");
    loop {
        tokio::select! {
            result = &mut reconciliation => return result,
            _ = renewal.tick() => {
                match worker::renew_lease(pool, &job.id, worker_id, clock.now(), lease_seconds).await {
                    Ok(true) => {}
                    Ok(false) => {
                        tracing::warn!(
                            error_code = "operation_reconciliation_lease_ownership_lost",
                            worker_id = %worker_id,
                            job_id = %job.id,
                            action = %job.action
                        );
                        return reconciliation.await;
                    }
                    Err(_) => tracing::warn!(
                        error_code = "operation_reconciliation_lease_renewal_failed",
                        worker_id = %worker_id,
                        job_id = %job.id,
                        action = %job.action
                    ),
                }
            }
        }
    }
}

async fn wait_or_cancel(control: &RuntimeControl, duration: Duration) -> bool {
    tokio::select! {
        () = control.cancelled() => true,
        () = tokio::time::sleep(duration) => false,
    }
}

struct ErrorBackoff {
    initial: Duration,
    current: Duration,
    maximum: Duration,
}

impl ErrorBackoff {
    fn new(initial: Duration, maximum: Duration) -> Self {
        Self {
            initial,
            current: initial,
            maximum,
        }
    }

    fn reset(&mut self) {
        self.current = self.initial;
    }

    fn next_delay(&mut self) -> Duration {
        let delay = self.current;
        self.current = self.current.saturating_mul(2).min(self.maximum);
        delay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::mcp::action_registry::{ActionExecution, ACTIONS},
        operations::{
            adapters::{OperationAdapter, PlanRequest, ReconcileOutcome, StepOutcome, StepRequest},
            contracts::{
                ActorRef, ActorType, CapabilityAvailability, OperationPlanV1, PlannedStepV1,
                ResourceRef,
            },
            jobs::{self, SubmissionPolicy, SubmitJob},
            registry::ADAPTERS,
            resources::{self, ObserveResource},
        },
    };
    use anyhow::bail;
    use async_trait::async_trait;
    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering},
            Mutex,
        },
    };
    use tokio::sync::Notify;

    #[derive(Default)]
    struct Tracker {
        active: AtomicUsize,
        maximum_active: AtomicUsize,
        overlap: AtomicBool,
        reconciliations: AtomicUsize,
        active_resources: Mutex<HashMap<String, usize>>,
        calls: Mutex<Vec<(String, String)>>,
        gate_closed: AtomicBool,
        gate: Notify,
    }

    impl Tracker {
        fn with_gate() -> Self {
            Self {
                gate_closed: AtomicBool::new(true),
                ..Self::default()
            }
        }

        fn open_gate(&self) {
            self.gate_closed.store(false, Ordering::SeqCst);
            self.gate.notify_waiters();
        }

        fn enter(&self, request: &StepRequest) {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum_active.fetch_max(active, Ordering::SeqCst);
            let mut resources = self.active_resources.lock().unwrap();
            let resource_active = resources.entry(request.resource.id.clone()).or_default();
            *resource_active += 1;
            if *resource_active > 1 {
                self.overlap.store(true, Ordering::SeqCst);
            }
            self.calls
                .lock()
                .unwrap()
                .push((request.job_id.clone(), request.step.name.clone()));
        }

        fn leave(&self, resource_id: &str) {
            let mut resources = self.active_resources.lock().unwrap();
            let active = resources.get_mut(resource_id).unwrap();
            *active -= 1;
            self.active.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct FakeAdapter {
        key: &'static str,
        actions: Vec<&'static str>,
        tracker: Arc<Tracker>,
    }

    #[async_trait]
    impl OperationAdapter for FakeAdapter {
        fn key(&self) -> &'static str {
            self.key
        }

        fn actions(&self) -> &[&'static str] {
            &self.actions
        }

        async fn plan(&self, _request: PlanRequest) -> Result<OperationPlanV1> {
            bail!("not used")
        }

        async fn external_fingerprint(&self, _request: &PlanRequest) -> Result<String> {
            Ok("state-1".into())
        }

        async fn execute_step(&self, request: StepRequest) -> Result<StepOutcome> {
            self.tracker.enter(&request);
            let notified = self.tracker.gate.notified();
            if self.tracker.gate_closed.load(Ordering::SeqCst) {
                notified.await;
            }
            self.tracker.leave(&request.resource.id);
            Ok(StepOutcome::Succeeded {
                result: serde_json::json!({"completed": request.step.name}),
                external_operation_id: None,
            })
        }

        async fn reconcile(&self, _request: StepRequest) -> Result<ReconcileOutcome> {
            self.tracker.reconciliations.fetch_add(1, Ordering::SeqCst);
            let notified = self.tracker.gate.notified();
            if self.tracker.gate_closed.load(Ordering::SeqCst) {
                notified.await;
            }
            Ok(ReconcileOutcome::StillUncertain {
                message: "verification pending".into(),
            })
        }
    }

    struct ManualClock(AtomicI64);

    impl ManualClock {
        fn new(now: i64) -> Self {
            Self(AtomicI64::new(now))
        }

        fn set(&self, now: i64) {
            self.0.store(now, Ordering::SeqCst);
        }
    }

    impl Clock for ManualClock {
        fn now(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    struct CountingClock {
        now: i64,
        calls: AtomicUsize,
    }

    impl CountingClock {
        fn new(now: i64) -> Self {
            Self {
                now,
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl Clock for CountingClock {
        fn now(&self) -> i64 {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.now
        }
    }

    async fn setup_pool() -> SqlitePool {
        let path = std::env::temp_dir().join(format!(
            "voidtower-operation-runtime-{}-{}.db",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        crate::db::init_pool(&path).await.unwrap()
    }

    fn complete_registry(tracker: Arc<Tracker>) -> Arc<AdapterRegistry> {
        let mut registry = AdapterRegistry::new();
        for metadata in ADAPTERS {
            let actions = ACTIONS
                .iter()
                .filter(|action| {
                    action.execution == ActionExecution::DurableJob
                        && action.adapter_key == Some(metadata.key)
                })
                .map(|action| action.name)
                .collect();
            registry
                .register(Arc::new(FakeAdapter {
                    key: metadata.key,
                    actions,
                    tracker: tracker.clone(),
                }))
                .unwrap();
        }
        Arc::new(registry)
    }

    async fn observe_container(pool: &SqlitePool, alias: &str) -> ResourceRef {
        let resource = resources::observe(
            pool,
            ObserveResource {
                kind: "container",
                display_name: alias,
                node_id: None,
                provider: Some("docker"),
                namespace: "test.runtime.container",
                scope_key: "local",
                alias,
            },
            None,
            "runtime_test",
        )
        .await
        .unwrap();
        resources::set_capability(
            pool,
            &resource.id,
            "container.start",
            CapabilityAvailability::Available,
            None,
            None,
            "runtime_test",
        )
        .await
        .unwrap();
        resource
    }

    async fn submit(
        pool: &SqlitePool,
        resource: &ResourceRef,
        idempotency_key: &str,
        steps: &[&str],
    ) -> String {
        jobs::submit(
            pool,
            submission(resource, idempotency_key, steps, SubmissionPolicy::Allow),
        )
        .await
        .unwrap()
        .id
    }

    fn submission(
        resource: &ResourceRef,
        idempotency_key: &str,
        steps: &[&str],
        policy: SubmissionPolicy,
    ) -> SubmitJob {
        SubmitJob {
            action: "container.start".into(),
            resource: resource.clone(),
            actor: ActorRef {
                actor_type: ActorType::Human,
                id: Some("owner".into()),
                source: Some("runtime_test".into()),
            },
            ingress: "http".into(),
            input: serde_json::json!({}),
            plan: OperationPlanV1 {
                schema_version: 1,
                title: "Runtime test".into(),
                risk: "mutate".into(),
                changes: vec![],
                preview: None,
                external_fingerprint: "state-1".into(),
                steps: steps
                    .iter()
                    .map(|name| PlannedStepV1 {
                        kind: "execute".into(),
                        name: (*name).into(),
                        retry_class: "never".into(),
                        recovery_class: "reconcile".into(),
                    })
                    .collect(),
            },
            idempotency_scope: "runtime-test:owner".into(),
            idempotency_key: idempotency_key.into(),
            concurrency_key: resource.id.clone(),
            retry_class: "never".into(),
            recovery_class: "reconcile".into(),
            policy,
        }
    }

    async fn wait_for_state(pool: &SqlitePool, job_id: &str, expected: JobState) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if jobs::get(pool, job_id).await.unwrap().unwrap().state == expected {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    async fn wait_for_active(tracker: &Tracker, expected: usize) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if tracker.active.load(Ordering::SeqCst) == expected {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn preparation_rejects_an_incomplete_registry_before_start() {
        let pool = setup_pool().await;
        let error = prepare_with_clock(
            pool,
            Arc::new(AdapterRegistry::new()),
            OperationRuntimeConfig::default(),
            Arc::new(ManualClock::new(100)),
        )
        .await
        .err()
        .expect("incomplete runtime registry must fail");
        assert!(error.to_string().contains("runtime implementation"));
    }

    #[tokio::test]
    async fn preparation_expires_approvals_and_recovers_leases_before_start() {
        let pool = setup_pool().await;
        let tracker = Arc::new(Tracker::default());
        let registry = complete_registry(tracker.clone());
        let resource = observe_container(&pool, "startup-order").await;
        let approval_job = jobs::submit(
            &pool,
            submission(
                &resource,
                "expired-approval",
                &["approved-step"],
                SubmissionPolicy::RequireApproval {
                    requirement: "always".into(),
                    reason: "runtime test".into(),
                    expires_at: 99,
                },
            ),
        )
        .await
        .unwrap();
        let expired_lease = submit(&pool, &resource, "expired-lease", &["uncertain-step"]).await;
        let claimed = worker::claim_next(&pool, &registry, "old-worker", 90, 5)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.id, expired_lease);
        worker::claim_step(&pool, &expired_lease, "old-worker", 91, 5)
            .await
            .unwrap()
            .unwrap();

        let prepared = prepare_with_clock(
            pool.clone(),
            registry,
            OperationRuntimeConfig {
                reconciliation_poll_seconds: 3_600,
                ..OperationRuntimeConfig::default()
            },
            Arc::new(ManualClock::new(100)),
        )
        .await
        .unwrap();

        assert_eq!(
            jobs::get(&pool, &approval_job.id)
                .await
                .unwrap()
                .unwrap()
                .state,
            JobState::Expired
        );
        assert_eq!(
            jobs::get(&pool, &expired_lease)
                .await
                .unwrap()
                .unwrap()
                .state,
            JobState::NeedsAttention
        );
        assert!(tracker.calls.lock().unwrap().is_empty());
        assert_eq!(tracker.reconciliations.load(Ordering::SeqCst), 0);
        let runtime = prepared.start();
        tokio::time::timeout(Duration::from_millis(200), async {
            while tracker.reconciliations.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the first reconciliation sweep must not wait for the recurring cadence");
        assert!(runtime.shutdown().await);
    }

    #[tokio::test]
    async fn idle_runtime_shuts_down_without_waiting_for_poll_interval() {
        let pool = setup_pool().await;
        let tracker = Arc::new(Tracker::default());
        let prepared = prepare_with_clock(
            pool,
            complete_registry(tracker),
            OperationRuntimeConfig::default(),
            Arc::new(ManualClock::new(100)),
        )
        .await
        .unwrap();
        let runtime = prepared.start();
        assert!(
            tokio::time::timeout(Duration::from_millis(200), runtime.shutdown())
                .await
                .unwrap()
        );
    }

    #[test]
    fn error_backoff_is_bounded_and_resets_after_progress() {
        let mut backoff = ErrorBackoff::new(Duration::from_millis(100), Duration::from_millis(350));
        assert_eq!(backoff.next_delay(), Duration::from_millis(100));
        assert_eq!(backoff.next_delay(), Duration::from_millis(200));
        assert_eq!(backoff.next_delay(), Duration::from_millis(350));
        assert_eq!(backoff.next_delay(), Duration::from_millis(350));
        backoff.reset();
        assert_eq!(backoff.next_delay(), Duration::from_millis(100));
    }

    #[tokio::test]
    async fn workers_are_bounded_ordered_and_serialize_each_concurrency_key() {
        let pool = setup_pool().await;
        let tracker = Arc::new(Tracker::with_gate());
        let registry = complete_registry(tracker.clone());
        let first_resource = observe_container(&pool, "first").await;
        let second_resource = observe_container(&pool, "second").await;
        let first_job = submit(&pool, &first_resource, "first-job", &["one", "two"]).await;
        let same_key_job = submit(&pool, &first_resource, "same-key-job", &["one"]).await;
        let second_job = submit(&pool, &second_resource, "second-job", &["one"]).await;
        let config = OperationRuntimeConfig {
            worker_count: 2,
            ..OperationRuntimeConfig::default()
        };
        let runtime = prepare_with_clock(pool.clone(), registry, config, Arc::new(SystemClock))
            .await
            .unwrap()
            .start();

        wait_for_active(&tracker, 2).await;
        assert_eq!(tracker.maximum_active.load(Ordering::SeqCst), 2);
        assert!(!tracker.overlap.load(Ordering::SeqCst));
        tracker.open_gate();

        wait_for_state(&pool, &first_job, JobState::Succeeded).await;
        wait_for_state(&pool, &same_key_job, JobState::Succeeded).await;
        wait_for_state(&pool, &second_job, JobState::Succeeded).await;
        {
            let calls = tracker.calls.lock().unwrap();
            let first_steps: Vec<_> = calls
                .iter()
                .filter(|(job_id, _)| job_id == &first_job)
                .map(|(_, step)| step.as_str())
                .collect();
            assert_eq!(first_steps, vec!["one", "two"]);
            assert!(!tracker.overlap.load(Ordering::SeqCst));
        }
        assert!(runtime.shutdown().await);
    }

    #[tokio::test]
    async fn lease_renewal_prevents_recovery_during_a_provider_call() {
        let pool = setup_pool().await;
        let tracker = Arc::new(Tracker::with_gate());
        let resource = observe_container(&pool, "lease").await;
        let job_id = submit(&pool, &resource, "lease-job", &["long-running"]).await;
        let clock = Arc::new(ManualClock::new(100));
        let config = OperationRuntimeConfig {
            worker_count: 1,
            lease_seconds: 6,
            ..OperationRuntimeConfig::default()
        };
        let mut prepared = prepare_with_clock(
            pool.clone(),
            complete_registry(tracker.clone()),
            config,
            clock.clone(),
        )
        .await
        .unwrap();
        // Shorten only the already-validated test instance so the renewal boundary is exercised
        // without adding six seconds to the unit suite.
        prepared.config.lease_seconds = 3;
        let runtime = prepared.start();
        wait_for_active(&tracker, 1).await;

        clock.set(102);
        tokio::time::sleep(Duration::from_millis(1_100)).await;
        assert_eq!(worker::recover_expired(&pool, 104).await.unwrap(), 0);

        tracker.open_gate();
        wait_for_state(&pool, &job_id, JobState::Succeeded).await;
        assert!(runtime.shutdown().await);
    }

    #[tokio::test]
    async fn lease_supervision_waits_for_the_configured_renewal_interval() {
        let pool = setup_pool().await;
        let tracker = Arc::new(Tracker::with_gate());
        let registry = complete_registry(tracker.clone());
        let resource = observe_container(&pool, "renewal-cadence").await;
        let job_id = submit(&pool, &resource, "renewal-cadence-job", &["running"]).await;
        let job = worker::claim_next(&pool, &registry, "worker-a", 100, 30)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.id, job_id);
        let step = worker::claim_step(&pool, &job.id, "worker-a", 100, 30)
            .await
            .unwrap()
            .unwrap();
        let clock = CountingClock::new(101);
        let config = OperationRuntimeConfig::default();
        let supervision =
            supervise_execution(&pool, &registry, &job, &step, "worker-a", &clock, &config);
        tokio::pin!(supervision);

        tokio::select! {
            result = &mut supervision => panic!("provider unexpectedly completed: {result:?}"),
            () = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
        assert_eq!(clock.calls.load(Ordering::SeqCst), 1);

        tracker.open_gate();
        assert_eq!(supervision.await.unwrap(), JobState::Succeeded);
        assert_eq!(clock.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn reconciliation_supervision_waits_for_the_configured_renewal_interval() {
        let pool = setup_pool().await;
        let tracker = Arc::new(Tracker::with_gate());
        let registry = complete_registry(tracker.clone());
        let resource = observe_container(&pool, "reconciliation-renewal-cadence").await;
        let job_id = submit(
            &pool,
            &resource,
            "reconciliation-renewal-cadence-job",
            &["uncertain"],
        )
        .await;
        let job = worker::claim_next(&pool, &registry, "worker-a", 100, 30)
            .await
            .unwrap()
            .unwrap();
        let step = worker::claim_step(&pool, &job.id, "worker-a", 100, 30)
            .await
            .unwrap()
            .unwrap();
        worker::complete_step(
            &pool,
            &step,
            "worker-a",
            101,
            StepOutcome::Uncertain {
                code: "provider_timeout".into(),
                message: "Provider outcome could not be verified".into(),
                external_operation_id: Some("task-renewal".into()),
                diagnostic: None,
            },
        )
        .await
        .unwrap();
        let (job, step) = worker::claim_reconciliation(&pool, &registry, "reconciler-a", 102, 30)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.id, job_id);
        let clock = CountingClock::new(103);
        let config = OperationRuntimeConfig::default();
        let supervision = supervise_reconciliation(
            &pool,
            &registry,
            &job,
            &step,
            "reconciler-a",
            &clock,
            &config,
        );
        tokio::pin!(supervision);

        tokio::select! {
            result = &mut supervision => panic!("reconciliation unexpectedly completed: {result:?}"),
            () = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
        assert_eq!(tracker.reconciliations.load(Ordering::SeqCst), 1);
        assert_eq!(clock.calls.load(Ordering::SeqCst), 0);

        tracker.open_gate();
        assert_eq!(supervision.await.unwrap(), JobState::NeedsAttention);
        assert_eq!(clock.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn job_cancellation_prevents_the_next_provider_step() {
        let pool = setup_pool().await;
        let tracker = Arc::new(Tracker::with_gate());
        let resource = observe_container(&pool, "cancel").await;
        let job_id = submit(&pool, &resource, "cancel-job", &["first", "second"]).await;
        let runtime = prepare_with_clock(
            pool.clone(),
            complete_registry(tracker.clone()),
            OperationRuntimeConfig {
                worker_count: 1,
                ..OperationRuntimeConfig::default()
            },
            Arc::new(SystemClock),
        )
        .await
        .unwrap()
        .start();
        wait_for_active(&tracker, 1).await;
        worker::request_cancellation(
            &pool,
            &job_id,
            ActorRef {
                actor_type: ActorType::Human,
                id: Some("owner".into()),
                source: Some("runtime_test".into()),
            },
            SystemClock.now(),
        )
        .await
        .unwrap();
        tracker.open_gate();

        wait_for_state(&pool, &job_id, JobState::Cancelled).await;
        {
            let calls = tracker.calls.lock().unwrap();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].1, "first");
        }
        assert!(runtime.shutdown().await);
    }

    #[tokio::test]
    async fn graceful_shutdown_finishes_in_flight_work_and_stops_new_claims() {
        let pool = setup_pool().await;
        let tracker = Arc::new(Tracker::with_gate());
        let first_resource = observe_container(&pool, "shutdown-first").await;
        let second_resource = observe_container(&pool, "shutdown-second").await;
        let first_job = submit(&pool, &first_resource, "shutdown-running", &["running"]).await;
        let second_job = submit(&pool, &second_resource, "shutdown-queued", &["queued"]).await;
        sqlx::query(
            "UPDATE jobs SET queued_at = CASE WHEN id = ? THEN 1 ELSE 2 END WHERE id IN (?, ?)",
        )
        .bind(&first_job)
        .bind(&first_job)
        .bind(&second_job)
        .execute(&pool)
        .await
        .unwrap();
        let runtime = prepare_with_clock(
            pool.clone(),
            complete_registry(tracker.clone()),
            OperationRuntimeConfig {
                worker_count: 1,
                ..OperationRuntimeConfig::default()
            },
            Arc::new(SystemClock),
        )
        .await
        .unwrap()
        .start();
        wait_for_active(&tracker, 1).await;
        let control = runtime.control();
        control.cancel();
        let shutdown = tokio::spawn(runtime.shutdown());
        tokio::task::yield_now().await;
        assert_eq!(
            jobs::get(&pool, &second_job).await.unwrap().unwrap().state,
            JobState::Queued
        );
        tracker.open_gate();
        assert!(shutdown.await.unwrap());
        assert_eq!(
            jobs::get(&pool, &first_job).await.unwrap().unwrap().state,
            JobState::Succeeded
        );
        assert_eq!(
            jobs::get(&pool, &second_job).await.unwrap().unwrap().state,
            JobState::Queued
        );
        assert_eq!(tracker.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn shutdown_timeout_leaves_the_in_flight_attempt_for_recovery() {
        let pool = setup_pool().await;
        let tracker = Arc::new(Tracker::with_gate());
        let resource = observe_container(&pool, "shutdown-timeout").await;
        let job_id = submit(&pool, &resource, "shutdown-timeout-job", &["running"]).await;
        let runtime = prepare_with_clock(
            pool.clone(),
            complete_registry(tracker.clone()),
            OperationRuntimeConfig {
                worker_count: 1,
                shutdown_timeout_seconds: 1,
                ..OperationRuntimeConfig::default()
            },
            Arc::new(SystemClock),
        )
        .await
        .unwrap()
        .start();
        wait_for_active(&tracker, 1).await;

        assert!(!runtime.shutdown().await);
        let summary = jobs::get(&pool, &job_id).await.unwrap().unwrap();
        assert_eq!(summary.state, JobState::Running);
        let attempt: (Option<i64>, Option<String>) =
            sqlx::query_as("SELECT finished_at, outcome FROM job_attempts WHERE job_id = ?")
                .bind(&job_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(attempt, (None, None));
    }

    #[tokio::test]
    async fn fast_restart_recovers_after_expiry_and_reconciles_without_replay() {
        let pool = setup_pool().await;
        let tracker = Arc::new(Tracker::default());
        let registry = complete_registry(tracker.clone());
        let resource = observe_container(&pool, "restart").await;
        let job_id = submit(&pool, &resource, "restart-job", &["uncertain"]).await;
        worker::claim_next(&pool, &registry, "old-worker", 100, 5)
            .await
            .unwrap()
            .unwrap();
        worker::claim_step(&pool, &job_id, "old-worker", 100, 5)
            .await
            .unwrap()
            .unwrap();
        let clock = Arc::new(ManualClock::new(100));
        let runtime = prepare_with_clock(
            pool.clone(),
            registry,
            OperationRuntimeConfig {
                worker_count: 1,
                reconciliation_poll_seconds: 1,
                ..OperationRuntimeConfig::default()
            },
            clock.clone(),
        )
        .await
        .unwrap()
        .start();
        assert_eq!(
            jobs::get(&pool, &job_id).await.unwrap().unwrap().state,
            JobState::Running
        );

        clock.set(106);
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if tracker.reconciliations.load(Ordering::SeqCst) == 1 {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            jobs::get(&pool, &job_id).await.unwrap().unwrap().state,
            JobState::NeedsAttention
        );
        assert!(tracker.calls.lock().unwrap().is_empty());
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(tracker.reconciliations.load(Ordering::SeqCst), 1);
        let lease_owner: Option<String> =
            sqlx::query_scalar("SELECT lease_owner FROM jobs WHERE id = ?")
                .bind(&job_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(lease_owner.is_none());
        assert!(runtime.shutdown().await);
    }
}
