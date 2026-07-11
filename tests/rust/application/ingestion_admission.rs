use pvlog_application::{IngestionAdmission, IngestionAdmissionError};
use std::error::Error;

#[test]
fn admission_rejects_saturation_with_retry_timing_and_metrics() -> Result<(), Box<dyn Error>> {
    let admission = IngestionAdmission::new(1, 10, 3);
    let permit = admission.try_admit(0)?;
    assert!(matches!(
        admission.try_admit(0),
        Err(IngestionAdmissionError::Overloaded {
            retry_after_seconds: 3,
            reason: "concurrency"
        })
    ));
    assert!(matches!(
        admission.try_admit(11),
        Err(IngestionAdmissionError::Overloaded {
            retry_after_seconds: 3,
            reason: "queue_lag"
        })
    ));
    assert_eq!(
        (
            admission.metrics().active,
            admission.metrics().rejected_concurrency,
            admission.metrics().rejected_queue_lag
        ),
        (1, 1, 1)
    );
    drop(permit);
    assert_eq!(admission.metrics().active, 0);
    assert!(admission.try_admit(0).is_ok());
    Ok(())
}
