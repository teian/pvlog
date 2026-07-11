use async_trait::async_trait;
use pvlog_application::{
    BatchIngestionMode, BatchIngestionRepository, BatchIngestionService, BatchItemStatus, PortError,
};
use pvlog_domain::{
    CanonicalObservation, IdempotencyIdentity, MeasurementValues, ObservationId, ObservationSource,
    ObservationSourceKind, QualityFlags, SystemId, UtcTimestamp,
};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn atomic_batches_roll_back_and_partial_batches_keep_stable_outcomes()
-> Result<(), Box<dyn Error>> {
    let repository = Arc::new(FakeRepository::default());
    let service = BatchIngestionService::new(repository.clone(), 3, 1_024);
    let items = vec![
        observation("valid"),
        observation("invalid"),
        observation("duplicate"),
    ];
    let atomic = service
        .ingest(items.clone(), 100, BatchIngestionMode::Atomic)
        .await?;
    assert_eq!(atomic.outcomes[0].index, 1);
    assert_eq!(repository.writes()?, 0);
    let partial = service
        .ingest(items, 100, BatchIngestionMode::Partial)
        .await?;
    assert_eq!(
        partial
            .outcomes
            .iter()
            .map(|outcome| (outcome.index, outcome.status))
            .collect::<Vec<_>>(),
        [
            (0, BatchItemStatus::Inserted),
            (1, BatchItemStatus::Invalid),
            (2, BatchItemStatus::Duplicate)
        ]
    );
    assert_eq!(repository.writes()?, 2);
    assert!(
        service
            .ingest(vec![observation("a"); 4], 100, BatchIngestionMode::Atomic)
            .await
            .is_err()
    );
    Ok(())
}
fn observation(reference: &str) -> CanonicalObservation {
    let timestamp = UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH);
    CanonicalObservation {
        id: ObservationId::new(),
        system_id: SystemId::new(),
        observed_at: timestamp,
        received_at: timestamp,
        values: MeasurementValues::default(),
        source: ObservationSource {
            kind: ObservationSourceKind::ModernApi,
            source_reference: Some(reference.to_owned()),
        },
        idempotency: IdempotencyIdentity {
            namespace: "test".to_owned(),
            key: reference.to_owned(),
            payload_hash: [0; 32],
        },
        quality: QualityFlags::NONE,
    }
}
#[derive(Default)]
struct FakeRepository(Mutex<u32>);
impl FakeRepository {
    fn writes(&self) -> Result<u32, Box<dyn Error>> {
        Ok(*self.0.lock().map_err(|_| "poisoned")?)
    }
}
#[async_trait]
impl BatchIngestionRepository for FakeRepository {
    async fn validate(&self, observation: &CanonicalObservation) -> Result<(), &'static str> {
        if observation.idempotency.key == "invalid" {
            Err("invalid")
        } else {
            Ok(())
        }
    }
    async fn insert_atomic(
        &self,
        observations: &[CanonicalObservation],
    ) -> Result<Vec<BatchItemStatus>, PortError> {
        *self.0.lock().map_err(|_| PortError::Unavailable)? +=
            u32::try_from(observations.len()).map_err(|_| PortError::Unavailable)?;
        Ok(vec![BatchItemStatus::Inserted; observations.len()])
    }
    async fn insert_one(
        &self,
        observation: &CanonicalObservation,
    ) -> Result<BatchItemStatus, PortError> {
        *self.0.lock().map_err(|_| PortError::Unavailable)? += 1;
        Ok(if observation.idempotency.key == "duplicate" {
            BatchItemStatus::Duplicate
        } else {
            BatchItemStatus::Inserted
        })
    }
}
