use async_trait::async_trait;
use pvlog_application::{
    CorrectObservation, CorrectionRepository, CorrectionService, ObservationCorrectionError,
    PortError, VersionedObservation,
};
use pvlog_domain::{MeasurementValues, ObservationId, SystemId, UserId, Watts};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn archived_overlays_and_hot_deletions_are_immediately_visible_and_rebuilt()
-> Result<(), Box<dyn Error>> {
    for archived in [false, true] {
        let id = ObservationId::new();
        let system = SystemId::new();
        let repository = Arc::new(FakeRepository::new(id, system, archived));
        let service = CorrectionService::new(repository.clone());
        let visible = service
            .correct(CorrectObservation {
                observation_id: id,
                system_id: system,
                actor: UserId::new(),
                expected_version: 1,
                replacement: None,
                reason: "bad reading".to_owned(),
            })
            .await?;
        assert_eq!(visible.values, None);
        assert_eq!(visible.version, 2);
        assert_eq!(repository.rebuilds()?, 1);
        assert!(matches!(
            service
                .correct(CorrectObservation {
                    observation_id: id,
                    system_id: system,
                    actor: UserId::new(),
                    expected_version: 1,
                    replacement: Some(MeasurementValues {
                        generation_power: Some(Watts::new(1)),
                        ..MeasurementValues::default()
                    }),
                    reason: "stale".to_owned()
                })
                .await,
            Err(ObservationCorrectionError::Conflict)
        ));
    }
    Ok(())
}
struct State {
    observation: VersionedObservation,
    rebuilds: u32,
}
struct FakeRepository(Mutex<State>);
impl FakeRepository {
    fn new(id: ObservationId, system_id: SystemId, archived: bool) -> Self {
        Self(Mutex::new(State {
            observation: VersionedObservation {
                id,
                system_id,
                values: Some(MeasurementValues::default()),
                version: 1,
                archived,
            },
            rebuilds: 0,
        }))
    }
    fn rebuilds(&self) -> Result<u32, Box<dyn Error>> {
        Ok(self.0.lock().map_err(|_| "poisoned")?.rebuilds)
    }
    fn apply(&self, command: &CorrectObservation) -> Result<bool, PortError> {
        let mut state = self.0.lock().map_err(|_| PortError::Unavailable)?;
        if state.observation.version != command.expected_version {
            return Ok(false);
        }
        state.observation.values.clone_from(&command.replacement);
        state.observation.version += 1;
        Ok(true)
    }
}
#[async_trait]
impl CorrectionRepository for FakeRepository {
    async fn observation(
        &self,
        id: ObservationId,
    ) -> Result<Option<VersionedObservation>, PortError> {
        let observation = self
            .0
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .observation
            .clone();
        Ok((observation.id == id).then_some(observation))
    }
    async fn replace_hot(&self, command: &CorrectObservation) -> Result<bool, PortError> {
        self.apply(command)
    }
    async fn append_archived_overlay(
        &self,
        command: &CorrectObservation,
    ) -> Result<bool, PortError> {
        self.apply(command)
    }
    async fn enqueue_rebuild(
        &self,
        _system: SystemId,
        _observation: ObservationId,
    ) -> Result<(), PortError> {
        self.0.lock().map_err(|_| PortError::Unavailable)?.rebuilds += 1;
        Ok(())
    }
    async fn visible_observation(
        &self,
        id: ObservationId,
    ) -> Result<Option<VersionedObservation>, PortError> {
        self.observation(id).await
    }
}
