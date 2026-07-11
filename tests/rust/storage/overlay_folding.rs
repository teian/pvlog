#![allow(clippy::unwrap_used)]

use async_trait::async_trait;
use pvlog_storage::{
    ArchivedSegmentBytes, OverlayFoldError, OverlayFoldKey, OverlayFoldPhase,
    OverlayFoldRepository, OverlayFoldService, OverlayFoldState, SegmentPoint, decode_segment_v1,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

#[tokio::test]
async fn fold_resumes_at_every_crash_point_with_immediate_consistency() {
    for crash_operation in 1..=5 {
        let repository = Arc::new(FakeRepository::new(Some(crash_operation)));
        let service = OverlayFoldService::new(repository.clone());
        let key = key();
        assert!(service.fold(key).await.is_err());
        assert_eq!(repository.visible_watts(), 20);
        repository.disable_crash();
        assert_eq!(
            service.fold(key).await.unwrap(),
            OverlayFoldPhase::Completed
        );
        assert_eq!(repository.visible_watts(), 20);
        assert!(repository.inner.lock().unwrap().overlays.is_empty());
    }
}

#[tokio::test]
async fn fold_rejects_a_stale_segment_generation() {
    let repository = Arc::new(FakeRepository::new(None));
    repository.inner.lock().unwrap().generation = 2;
    let service = OverlayFoldService::new(repository);
    assert!(matches!(
        service.fold(key()).await,
        Err(OverlayFoldError::GenerationConflict)
    ));
}

struct State {
    phase: OverlayFoldPhase,
    generation: u64,
    segment: Vec<SegmentPoint>,
    overlays: Vec<(u64, SegmentPoint)>,
    crash_operation: Option<usize>,
    operation: usize,
}
struct FakeRepository {
    inner: Mutex<State>,
}
impl FakeRepository {
    fn new(crash_operation: Option<usize>) -> Self {
        Self {
            inner: Mutex::new(State {
                phase: OverlayFoldPhase::Pending,
                generation: 1,
                segment: vec![point(10)],
                overlays: vec![(7, point(20))],
                crash_operation,
                operation: 0,
            }),
        }
    }
    fn fail(state: &mut State) -> Result<(), OverlayFoldError> {
        state.operation += 1;
        if state.crash_operation == Some(state.operation) {
            return Err(OverlayFoldError::Persistence("injected crash"));
        }
        Ok(())
    }
    fn disable_crash(&self) {
        self.inner.lock().unwrap().crash_operation = None;
    }
    fn visible_watts(&self) -> i64 {
        let state = self.inner.lock().unwrap();
        state
            .overlays
            .last()
            .map_or_else(|| &state.segment[0], |(_, point)| point)
            .generation_power_watts
            .unwrap()
    }
}

#[async_trait]
impl OverlayFoldRepository for FakeRepository {
    async fn prepare(&self, _: OverlayFoldKey) -> Result<OverlayFoldState, OverlayFoldError> {
        let mut state = self.inner.lock().unwrap();
        Self::fail(&mut state)?;
        Ok(OverlayFoldState {
            phase: state.phase,
            expected_generation: 1,
            overlay_revision_watermark: 7,
        })
    }
    async fn merged_points(
        &self,
        _: OverlayFoldKey,
        _: u64,
    ) -> Result<Vec<SegmentPoint>, OverlayFoldError> {
        let mut state = self.inner.lock().unwrap();
        Self::fail(&mut state)?;
        Ok(vec![state.overlays.last().unwrap().1.clone()])
    }
    async fn replace_segment(
        &self,
        _: OverlayFoldKey,
        expected_generation: u64,
        replacement: &ArchivedSegmentBytes,
    ) -> Result<bool, OverlayFoldError> {
        let mut state = self.inner.lock().unwrap();
        Self::fail(&mut state)?;
        if state.generation != expected_generation {
            return Ok(state.generation == expected_generation + 1
                && state.segment == decode_segment_v1(replacement)?.1);
        }
        state.segment = decode_segment_v1(replacement)?.1;
        state.generation += 1;
        Ok(true)
    }
    async fn delete_overlays_through(
        &self,
        _: OverlayFoldKey,
        watermark: u64,
    ) -> Result<(), OverlayFoldError> {
        let mut state = self.inner.lock().unwrap();
        Self::fail(&mut state)?;
        state.overlays.retain(|(revision, _)| *revision > watermark);
        Ok(())
    }
    async fn advance(
        &self,
        _: OverlayFoldKey,
        phase: OverlayFoldPhase,
    ) -> Result<(), OverlayFoldError> {
        let mut state = self.inner.lock().unwrap();
        Self::fail(&mut state)?;
        state.phase = phase;
        Ok(())
    }
}

fn key() -> OverlayFoldKey {
    OverlayFoldKey {
        system_id: Uuid::from_u128(1),
        range_start: 0,
        range_end: 86_400_000,
    }
}
fn point(watts: i64) -> SegmentPoint {
    SegmentPoint {
        timestamp_epoch_millis: 1_000,
        generation_power_watts: Some(watts),
        extended: BTreeMap::new(),
        source_kind: "fixture".into(),
        source_reference: "overlay-fold".into(),
        received_at_epoch_millis: 1_001,
        quality_flags: 0,
    }
}
