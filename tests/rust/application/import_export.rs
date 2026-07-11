use async_trait::async_trait;
use pvlog_application::{
    Clock, ExportJobResource, ImportExportError, ImportExportRepository, ImportExportService,
    ImportPlan, PortError,
};
use pvlog_domain::{AccountId, ExportFormat, ImportId, SystemId, UserId, UtcTimestamp};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn imports_validate_before_commit_and_exports_are_expiring_and_checksummed()
-> Result<(), Box<dyn Error>> {
    let repository = Arc::new(FakeRepository::default());
    let service = ImportExportService::new(repository, Arc::new(FixedClock), 300);
    let account = AccountId::new();
    let invalid = service
        .dry_run(
            account,
            br#"{"systems":[{"sourceId":"a","name":"Roof"},{"sourceId":"a","name":""}]}"#.to_vec(),
        )
        .await?;
    assert_eq!(invalid.issues.len(), 2);
    assert!(matches!(
        service.commit(&invalid).await,
        Err(ImportExportError::ValidationFailed)
    ));
    let valid = service
        .dry_run(
            account,
            br#"{"systems":[{"sourceId":"a","name":"Roof"}]}"#.to_vec(),
        )
        .await?;
    service.commit(&valid).await?;
    assert!(matches!(
        service.commit(&valid).await,
        Err(ImportExportError::PlanUnavailable)
    ));
    let mut export = service
        .request_export(
            account,
            SystemId::new(),
            UserId::new(),
            ExportFormat::PortableBundle,
        )
        .await?;
    assert_eq!(export.expires_at, 300_000);
    let bytes = b"portable";
    export.content_checksum = Some(*blake3::hash(bytes).as_bytes());
    assert!(ImportExportService::verify_artifact(&export, bytes));
    assert!(!ImportExportService::verify_artifact(&export, b"changed"));
    Ok(())
}
struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH)
    }
}
#[derive(Default)]
struct FakeRepository(Mutex<Vec<ImportId>>);
#[async_trait]
impl ImportExportRepository for FakeRepository {
    async fn save_plan(&self, plan: ImportPlan, _document: Vec<u8>) -> Result<(), PortError> {
        self.0
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .push(plan.id);
        Ok(())
    }
    async fn commit_plan(&self, id: ImportId, _account: AccountId) -> Result<bool, PortError> {
        let mut ids = self.0.lock().map_err(|_| PortError::Unavailable)?;
        if let Some(index) = ids.iter().position(|candidate| *candidate == id) {
            ids.remove(index);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    async fn enqueue_export(
        &self,
        _resource: ExportJobResource,
        _requested_by: UserId,
    ) -> Result<(), PortError> {
        Ok(())
    }
}
