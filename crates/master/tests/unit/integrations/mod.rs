use super::*;
use crate::platform::db::testing;

fn db() -> Db {
    let db = testing::db();
    let mut c = db.get().unwrap();
    c.execute("INSERT INTO folders (name, kind, created_at, user_id) VALUES ('source-d', 'user', 0, 1)", &[])
        .unwrap();
    c.execute(
        "INSERT INTO links (direction, name, kind, config, folder_id, user_id, created_at, interval_secs)
             VALUES ('in', 'd', 'rclone', '{}', (SELECT id FROM folders WHERE name = 'source-d'), 1, 0, 3600)",
        &[],
    )
    .unwrap();
    drop(c);
    db
}

/// What the scheduler does in one tick: queues what is due, then takes the runs that are queued.
fn tick_once(db: &Db) -> Option<(uuid::Uuid, Due)> {
    while schedule(db) {}
    claim(db)
}

#[test]
fn a_link_that_never_ran_is_queued_as_a_job_then_waits_for_the_interval_or_a_request() {
    let db = db();
    let sql = |q: &str, p: &[&(dyn postgres::types::ToSql + Sync)]| db.get().unwrap().execute(q, p).unwrap();
    let jobs = |status: &str| -> i64 {
        db.get()
            .unwrap()
            .query_one("SELECT COUNT(*) FROM jobs WHERE kind = 'source' AND status = $1", &[&status])
            .unwrap()
            .get(0)
    };
    assert!(schedule(&db), "never run: due immediately");
    assert!(!schedule(&db), "and queued once");
    assert_eq!(jobs("queued"), 1);
    let (job, first) = claim(&db).expect("the queued run starts");
    assert_eq!(
        (first.scope.as_str(), first.key().as_str()),
        ("admin/source-d", "admin/d"),
        "writes in its folder, named by its owner"
    );
    assert_eq!(jobs("running"), 1);
    assert!(tick_once(&db).is_none(), "now running");
    let mut c = db.get().unwrap();
    finish(
        &mut *c,
        first.id,
        Dir::In,
        "admin/d",
        Ended { status: RunStatus::Ok, message: "fine", job: Some(job) },
    );
    drop(c);
    assert_eq!(jobs("done"), 1, "the job records how it went");
    assert!(tick_once(&db).is_none(), "ran just now, interval not over");
    sql("UPDATE links SET run_requested = 1", &[]);
    assert!(tick_once(&db).is_some(), "run now");
    sql("UPDATE links SET last_status = 'ok', last_run_at = $1", &[&(unix_now() - 7200)]);
    sql("DELETE FROM jobs", &[]);
    assert!(tick_once(&db).is_some(), "interval over");
    sql("UPDATE links SET last_status = 'ok', last_run_at = $1, enabled = 0", &[&(unix_now() - 7200)]);
    sql("DELETE FROM jobs", &[]);
    assert!(tick_once(&db).is_none(), "disabled sources only run on request");
}

#[test]
fn a_restart_fails_the_runs_it_interrupted_and_keeps_the_queued_ones() {
    let db = db();
    let mut c = db.get().unwrap();
    let c = &mut *c;
    assert!(schedule(&db));
    let (job, due) = claim(&db).unwrap();
    links::fail_interrupted(c, unix_now()).unwrap();
    let row = c.query_one("SELECT status, error FROM jobs WHERE id = $1", &[&job]).unwrap();
    let (status, error): (String, Option<String>) = (row.get(0), row.get(1));
    assert_eq!((status.as_str(), error.as_deref()), ("failed", Some("interrupted by a restart")));
    let link: String = c.query_one("SELECT last_status FROM links WHERE id = $1", &[&due.id]).unwrap().get(0);
    assert_eq!(link, "failed");
    c.execute("UPDATE links SET last_run_at = 0", &[]).unwrap();
    assert!(schedule(&db), "and it is queued again by its schedule");
}

#[test]
fn failures_open_an_alert_and_success_closes_it() {
    let db = db();
    let mut c = db.get().unwrap();
    let c = &mut *c;
    let id: i64 = c.query_one("SELECT id FROM links", &[]).unwrap().get(0);
    finish(c, id, Dir::In, "d", Ended { status: RunStatus::Failed, message: "boom", job: None });
    assert_eq!(testing::open_alerts(c), 1);
    finish(c, id, Dir::In, "d", Ended { status: RunStatus::Ok, message: "fine", job: None });
    assert_eq!(testing::open_alerts(c), 0);
    let row = c.query_one("SELECT last_status, last_ok_at FROM links", &[]).unwrap();
    let (status, ok_at): (String, Option<i64>) = (row.get(0), row.get(1));
    assert_eq!(status, "ok");
    assert!(ok_at.is_some());
}
