use hief::db::Database;
use hief::router;

async fn open_test_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let db_path = dir.path().join("hief.db");
    let db = Database::open(&db_path).await.expect("open db");
    (dir, db)
}

async fn seed_groundedness(db: &Database, values: &[f64]) {
    for value in values {
        db.record_tool_event_scoped(
            "learn-session",
            "semantic_search",
            "query",
            Some("strategy=semantic"),
            Some(1),
            Some(1),
            Some(*value),
            Some("project-root"),
        )
        .await
        .expect("record groundedness");
    }
}

#[tokio::test]
async fn test_no_history_yields_noop_learning_state() {
    let (_dir, db) = open_test_db().await;

    let learning = router::learn_retrieval_weights(&db, true)
        .await
        .expect("learn weights");

    assert_eq!(learning.learning_state, "neutral");
    assert_eq!(learning.last_learning_outcome, "no_history");
    assert_eq!(learning.candidate_delta, 0.0);

    let weights = router::active_retrieval_weights(&db)
        .await
        .expect("active weights");
    assert!((weights.lexical - 0.4).abs() < 1e-9);
    assert!((weights.semantic - 0.4).abs() < 1e-9);
}

#[tokio::test]
async fn test_candidate_update_is_bounded() {
    let (_dir, db) = open_test_db().await;
    seed_groundedness(&db, &[0.95; 20]).await;

    let learning = router::learn_retrieval_weights(&db, true)
        .await
        .expect("learn weights");

    assert!(learning.candidate_delta <= 0.05);

    let latest = db
        .latest_retrieval_weight_snapshot()
        .await
        .expect("latest snapshot")
        .expect("snapshot row");
    let current: router::RetrievalWeights =
        serde_json::from_str(&latest.current_json).expect("parse current");
    let sum = current.lexical + current.semantic + current.co_access + current.recency;
    assert!((sum - 1.0).abs() < 1e-9, "weights should normalize to 1.0");
}

#[tokio::test]
async fn test_candidate_generation_is_deterministic_for_same_history() {
    let (_dir_a, db_a) = open_test_db().await;
    let (_dir_b, db_b) = open_test_db().await;

    let history = vec![0.81, 0.79, 0.83, 0.8, 0.82, 0.78, 0.84, 0.8, 0.81, 0.79];
    seed_groundedness(&db_a, &history).await;
    seed_groundedness(&db_b, &history).await;

    router::learn_retrieval_weights(&db_a, true)
        .await
        .expect("learn db A");
    router::learn_retrieval_weights(&db_b, true)
        .await
        .expect("learn db B");

    let snap_a = db_a
        .latest_retrieval_weight_snapshot()
        .await
        .expect("snapshot A")
        .expect("row A");
    let snap_b = db_b
        .latest_retrieval_weight_snapshot()
        .await
        .expect("snapshot B")
        .expect("row B");

    assert_eq!(snap_a.current_json, snap_b.current_json);
    assert_eq!(snap_a.candidate_json, snap_b.candidate_json);
    assert_eq!(snap_a.learning_state, snap_b.learning_state);
}
