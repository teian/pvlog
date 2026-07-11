use async_trait::async_trait;
use pvlog_storage::{
    ArchivedSegmentBytes, CompactionError, CompactionKey, CompactionPhase, CompactionRepository,
    CompactionService, SegmentPoint,
};
use std::{
    collections::BTreeMap,
    error::Error,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

#[tokio::test]
async fn compaction_resumes_idempotently_after_each_crash_transition() -> Result<(), Box<dyn Error>>
{
    for failure in [
        CompactionPhase::SegmentWritten,
        CompactionPhase::RollupsWritten,
        CompactionPhase::Verified,
        CompactionPhase::HotRowsDeleted,
        CompactionPhase::Completed,
    ] {
        let repository = Arc::new(FakeRepository::new(failure));
        let service = CompactionService::new(repository.clone(), 1_000);
        let key = CompactionKey {
            system_id: Uuid::now_v7(),
            range_start: 0,
            range_end: 86_400_000,
        };
        assert!(service.compact(key, "worker", 0).await.is_err());
        assert_eq!(
            service.compact(key, "worker", 2_000).await?,
            CompactionPhase::Completed
        );
        assert!(repository.deleted_after_verification()?);
    }
    Ok(())
}
struct State {
    phase: CompactionPhase,
    fail_once: Option<CompactionPhase>,
    verified: bool,
    deleted_after_verification: bool,
}
struct FakeRepository(Mutex<State>);
impl FakeRepository {
    fn new(failure: CompactionPhase) -> Self {
        Self(Mutex::new(State {
            phase: CompactionPhase::Pending,
            fail_once: Some(failure),
            verified: false,
            deleted_after_verification: false,
        }))
    }
    fn fail(&self, target: CompactionPhase) -> Result<(), CompactionError> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| CompactionError::Persistence("poisoned"))?;
        if state.fail_once == Some(target) {
            state.fail_once = None;
            return Err(CompactionError::Persistence("crash"));
        }
        Ok(())
    }
    fn deleted_after_verification(&self) -> Result<bool, Box<dyn Error>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| "poisoned")?
            .deleted_after_verification)
    }
}
#[async_trait]
impl CompactionRepository for FakeRepository {
    async fn acquire_lease(
        &self,
        _key: CompactionKey,
        _owner: &str,
        _expires: i64,
    ) -> Result<Option<CompactionPhase>, CompactionError> {
        Ok(Some(
            self.0
                .lock()
                .map_err(|_| CompactionError::Persistence("poisoned"))?
                .phase,
        ))
    }
    async fn stable_points(
        &self,
        _key: CompactionKey,
    ) -> Result<Vec<SegmentPoint>, CompactionError> {
        Ok(vec![SegmentPoint {
            timestamp_epoch_millis: 1,
            generation_power_watts: Some(1),
            extended: BTreeMap::new(),
            source_kind: "test".to_owned(),
            source_reference: String::new(),
            received_at_epoch_millis: 2,
            quality_flags: 0,
        }])
    }
    async fn write_segment(
        &self,
        _key: CompactionKey,
        _segment: &ArchivedSegmentBytes,
    ) -> Result<(), CompactionError> {
        self.fail(CompactionPhase::SegmentWritten)
    }
    async fn write_rollups(&self, _key: CompactionKey) -> Result<(), CompactionError> {
        self.fail(CompactionPhase::RollupsWritten)
    }
    async fn verify_segment_and_rollups(
        &self,
        _key: CompactionKey,
    ) -> Result<bool, CompactionError> {
        self.fail(CompactionPhase::Verified)?;
        self.0
            .lock()
            .map_err(|_| CompactionError::Persistence("poisoned"))?
            .verified = true;
        Ok(true)
    }
    async fn delete_redundant_hot_rows(&self, _key: CompactionKey) -> Result<(), CompactionError> {
        self.fail(CompactionPhase::HotRowsDeleted)?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| CompactionError::Persistence("poisoned"))?;
        state.deleted_after_verification = state.verified;
        Ok(())
    }
    async fn advance(
        &self,
        _key: CompactionKey,
        phase: CompactionPhase,
    ) -> Result<(), CompactionError> {
        self.fail(phase)?;
        self.0
            .lock()
            .map_err(|_| CompactionError::Persistence("poisoned"))?
            .phase = phase;
        Ok(())
    }
    async fn release_lease(
        &self,
        _key: CompactionKey,
        _owner: &str,
    ) -> Result<(), CompactionError> {
        Ok(())
    }
}
