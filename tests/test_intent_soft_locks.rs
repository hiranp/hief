use hief::db::Database;
use hief::errors::HiefError;
use hief::graph;
use hief::graph::Intent;

async fn open_test_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let db_path = dir.path().join("hief.db");
    let db = Database::open(&db_path).await.expect("open db");
    (dir, db)
}

async fn create_approved_intent(db: &Database, title: &str) -> String {
    let intent = Intent::new("feature", title, None, None);
    let id = intent.id.clone();
    graph::create_intent(db, &intent).await.expect("create intent");
    graph::update_status(db, &id, "approved")
        .await
        .expect("approve intent");
    id
}

#[tokio::test]
async fn test_in_progress_transition_acquires_soft_lock() {
    let (_dir, db) = open_test_db().await;
    let intent_id = create_approved_intent(&db, "acquire lock on transition").await;

    graph::update_status_scoped(
        &db,
        &intent_id,
        "in_progress",
        Some("agent-a"),
        Some("wt-a"),
        48,
    )
    .await
    .expect("transition should acquire lock");

    let mut rows = db
        .conn()
        .query(
            "SELECT holder, worktree_id FROM intent_locks WHERE intent_id = ?1",
            [intent_id.as_str()],
        )
        .await
        .expect("query lock");

    let row = rows.next().await.expect("next row").expect("lock row");
    let holder: String = row.get(0).expect("holder");
    let worktree_id: String = row.get(1).expect("worktree");
    assert_eq!(holder, "agent-a");
    assert_eq!(worktree_id, "wt-a");
}

#[tokio::test]
async fn test_soft_lock_conflict_release_and_expiry_reclaim() {
    let (_dir, db) = open_test_db().await;
    let intent_id = create_approved_intent(&db, "lock conflict and reclaim").await;

    graph::intent::acquire_soft_lock(&db, &intent_id, "agent-a", "wt-a", 3600)
        .await
        .expect("first acquire");

    let err = graph::intent::acquire_soft_lock(&db, &intent_id, "agent-b", "wt-b", 3600)
        .await
        .expect_err("competing lock should fail");
    match err {
        HiefError::IntentLockConflict {
            intent_id,
            holder,
            worktree_id,
        } => {
            assert!(!intent_id.is_empty());
            assert_eq!(holder, "agent-a");
            assert_eq!(worktree_id, "wt-a");
        }
        other => panic!("unexpected error: {other}"),
    }

    graph::intent::release_soft_lock(&db, &intent_id, Some("wt-a"))
        .await
        .expect("release lock");

    graph::intent::acquire_soft_lock(&db, &intent_id, "agent-b", "wt-b", 3600)
        .await
        .expect("acquire after release");

    // Lease of 0 should be immediately reclaimable.
    graph::intent::acquire_soft_lock(&db, &intent_id, "agent-b", "wt-b", 0)
        .await
        .expect("create immediately-expired lease");
    graph::intent::acquire_soft_lock(&db, &intent_id, "agent-c", "wt-c", 3600)
        .await
        .expect("expired lock should be reclaimable");
}
