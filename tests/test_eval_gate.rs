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

async fn insert_eval_run(db: &Database, passed: bool, created_at: i64) {
    let id = uuid::Uuid::new_v4().to_string();
    let score = if passed { 1.0 } else { 0.0 };
    let passed_int: i64 = if passed { 1 } else { 0 };

    db.conn()
        .execute(
            "INSERT INTO eval_runs (id, golden_set, overall_score, passed, details, git_commit, created_at)\
             VALUES (?1, 'workflow', ?2, ?3, '{}', '', ?4)",
            libsql::params![id.as_str(), score, passed_int, created_at],
        )
        .await
        .expect("insert eval run");
}

async fn create_intent_in_review(db: &Database, title: &str) -> String {
    let intent = Intent::new("feature", title, None, None);
    let id = intent.id.clone();

    graph::create_intent(db, &intent)
        .await
        .expect("create intent");
    graph::update_status(db, &id, "approved")
        .await
        .expect("to approved");
    graph::update_status(db, &id, "in_progress")
        .await
        .expect("to in_progress");
    graph::update_status(db, &id, "in_review")
        .await
        .expect("to in_review");

    id
}

#[tokio::test]
async fn test_in_review_to_verified_requires_eval_history() {
    let (_dir, db) = open_test_db().await;
    let intent_id = create_intent_in_review(&db, "missing eval gate").await;

    let err = graph::update_status(&db, &intent_id, "verified")
        .await
        .expect_err("missing eval should block verified promotion");

    match err {
        HiefError::EvalGateRejected { stage, reason } => {
            assert_eq!(stage, "to_verified");
            assert_eq!(reason, "no_eval_history");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn test_failed_eval_blocks_verified_and_merged_promotions() {
    let (_dir, db) = open_test_db().await;

    let first_intent = create_intent_in_review(&db, "failed eval to verified").await;
    insert_eval_run(&db, false, 1).await;

    let err = graph::update_status(&db, &first_intent, "verified")
        .await
        .expect_err("failed eval should block verified promotion");

    match err {
        HiefError::EvalGateRejected { stage, reason } => {
            assert_eq!(stage, "to_verified");
            assert_eq!(reason, "failed_eval");
        }
        other => panic!("unexpected error: {other}"),
    }

    let second_intent = create_intent_in_review(&db, "failed eval to merged").await;
    insert_eval_run(&db, true, 2).await;
    graph::update_status(&db, &second_intent, "verified")
        .await
        .expect("passing eval should allow verified");

    insert_eval_run(&db, false, 3).await;

    let err = graph::update_status(&db, &second_intent, "merged")
        .await
        .expect_err("failed eval should block merged promotion");

    match err {
        HiefError::EvalGateRejected { stage, reason } => {
            assert_eq!(stage, "to_merged");
            assert_eq!(reason, "failed_eval");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn test_passing_eval_allows_verified_and_merged_promotions() {
    let (_dir, db) = open_test_db().await;
    let intent_id = create_intent_in_review(&db, "passing eval path").await;

    insert_eval_run(&db, true, 1).await;

    graph::update_status(&db, &intent_id, "verified")
        .await
        .expect("passing eval should allow verified");
    graph::update_status(&db, &intent_id, "merged")
        .await
        .expect("passing eval should allow merged");

    let updated = graph::get_intent(&db, &intent_id)
        .await
        .expect("fetch intent");
    assert_eq!(updated.status, "merged");
}
