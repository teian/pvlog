use pvlog_application::{CursorPosition, PageCursorCodec, PaginationError};
use std::error::Error;

#[test]
fn cursors_bind_stable_position_query_expiry_and_page_bounds() -> Result<(), Box<dyn Error>> {
    let codec = PageCursorCodec::new([5; 32], 60);
    let id = uuid::Uuid::now_v7();
    let cursor = codec.encode(
        CursorPosition {
            sort_value: "2026-01-01T00:00:00Z".to_owned(),
            id,
        },
        "filter=active&sort=created_at,id",
        1_000,
    )?;
    assert_eq!(
        codec.decode(&cursor, "filter=active&sort=created_at,id", 2_000)?,
        CursorPosition {
            sort_value: "2026-01-01T00:00:00Z".to_owned(),
            id
        }
    );
    assert_eq!(
        codec.decode(&cursor, "filter=archived&sort=created_at,id", 2_000),
        Err(PaginationError::QueryMismatch)
    );
    assert_eq!(
        codec.decode(&cursor, "filter=active&sort=created_at,id", 61_000),
        Err(PaginationError::ExpiredCursor)
    );
    let mut tampered = cursor;
    tampered.push('0');
    assert_eq!(
        codec.decode(&tampered, "filter=active&sort=created_at,id", 2_000),
        Err(PaginationError::InvalidCursor)
    );
    assert_eq!(PageCursorCodec::page_size(100, 100)?, 100);
    assert_eq!(
        PageCursorCodec::page_size(101, 100),
        Err(PaginationError::InvalidPageSize)
    );
    Ok(())
}
