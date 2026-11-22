//! 拦截队列 + 持久化 + 重启对账集成测试（WSW-2-TASK §三 S5/S6；W2-D4/D6）。
//!
//! 三路径全 EXPIRED 终态（TTL / 停机 / 队满）+ 对账（未过期回载 / 过期→
//! expired / 停机全 expired）+ 内存 PendingQueue 与 024 行双面一致性。

use std::path::Path;

use wsw_runtime::listener::handler::{InterceptCoordinator, InterceptResolution};
use wsw_runtime::persist::{self, InterceptDirection, InterceptRow, PersistStore};
use wsw_runtime::{InterceptState, PendingQueue};

fn migrations_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

fn row(event: &str, queued_ms: i64, ttl_ms: i64) -> InterceptRow {
    InterceptRow {
        intercept_id: persist::generate_intercept_id(),
        event_id: event.to_string(),
        listener_id: "wsw-lis-itest-0001".to_string(),
        session_id: "wsw-ses-itest-0001".to_string(),
        exchange_id: Some("wsw-20261119000000-1-ab12".to_string()),
        direction: InterceptDirection::Request,
        state: InterceptState::Queued,
        host: Some("127.0.0.1".to_string()),
        method: Some("GET".to_string()),
        path: Some("/p59c-marker".to_string()),
        queued_at: persist::epoch_ms_to_iso(queued_ms),
        expires_at: persist::epoch_ms_to_iso(queued_ms + ttl_ms),
        resolved_at: None,
        reason: None,
    }
}

fn mem_store() -> PersistStore {
    let s = PersistStore::open_memory().expect("memory db");
    s.run_migrations(&migrations_dir()).expect("migrate to 024");
    s
}

#[test]
fn user_version_is_24_after_migrations() {
    let s = mem_store();
    let v = s
        .with_conn(|conn| conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)))
        .expect("user_version");
    assert_eq!(v, 27, "027 chain head (024 intercept + 025 exchange + 026 caller 入链 + 027 caller_attribution 入链)");
    // 三表齐备（024 纯新增）。
    for table in ["wsw_listener", "wsw_session", "wsw_intercept"] {
        let n = s
            .with_conn(|conn| {
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
                    r.get::<_, i64>(0)
                })
            })
            .unwrap_or_else(|e| panic!("{table}: {e}"));
        assert_eq!(n, 0, "{table} exists and is empty");
    }
}

#[test]
fn queue_persist_dual_face_forward_drop_modified() {
    let s = mem_store();
    let coord = InterceptCoordinator::new(8, 60_000, Some(s.clone()));
    let r = row("ev-fdm", 1_000, 600_000);
    s.enqueue_intercept(&r).expect("persist enqueue");
    let (id, rx) = coord
        .enqueue(
            "127.0.0.1",
            "GET",
            "/p59c-marker",
            "wsw-20261119000000-1-ab12",
            "wsw-lis-itest-0001",
            "wsw-ses-itest-0001",
        )
        .expect("in-memory enqueue");

    // Forward 动作（阻塞读裁决 + 双面终态）。
    let action = InterceptResolution::Forward;
    coord
        .resolve(&id, action.clone(), &persist::now_iso())
        .expect("resolve");
    assert_eq!(rx.blocking_recv().expect("resolution"), action);
    let all = s.list_all_intercepts().expect("rows");
    let forwarded = all.iter().find(|x| x.intercept_id == id).expect("row");
    assert_eq!(forwarded.state, InterceptState::Forwarded);
    assert!(forwarded.resolved_at.is_some());
    assert_eq!(forwarded.reason.as_deref(), Some("forwarded"));

    // ModifiedForward 动作（编辑态 bytes 传递 + modified_forwarded 终态）。
    let (id2, rx2) = coord
        .enqueue(
            "127.0.0.1",
            "POST",
            "/edit",
            "wsw-20261119000000-1-ab13",
            "wsw-lis-itest-0001",
            "wsw-ses-itest-0001",
        )
        .expect("enqueue2");
    let edited = InterceptResolution::ModifiedForward(b"edited-body".to_vec());
    coord
        .resolve(&id2, edited.clone(), &persist::now_iso())
        .expect("resolve2");
    assert!(matches!(
        rx2.blocking_recv().expect("resolution2"),
        InterceptResolution::ModifiedForward(_)
    ));
    let all = s.list_all_intercepts().expect("rows");
    assert!(all
        .iter()
        .any(|x| x.intercept_id == id2 && x.state == InterceptState::ModifiedForwarded));

    // 已终态行不可再推进（SQL WHERE state='queued' + 内存 AlreadyResolved）。
    assert!(!s
        .resolve_intercept(&id, InterceptState::Dropped, &persist::now_iso(), None)
        .expect("second resolve"));
}

#[test]
fn ttl_expiry_terminal_state() {
    let s = mem_store();
    let coord = InterceptCoordinator::new(8, 40, Some(s.clone()));
    let r = row(
        "ev-ttl",
        persist::iso_to_epoch_ms(&persist::now_iso()).unwrap(),
        600_000,
    );
    s.enqueue_intercept(&r).expect("enqueue");
    let (id, mut rx) = coord
        .enqueue(
            "127.0.0.1",
            "GET",
            "/",
            "wsw-20261119000000-1-ab14",
            "wsw-lis-itest-0001",
            "wsw-ses-itest-0001",
        )
        .expect("in-memory enqueue");
    std::thread::sleep(std::time::Duration::from_millis(80));
    coord.on_timeout(&id, &persist::now_iso());
    // waiter 已撤（EXPIRED 终态，请求自动放行）。
    assert!(rx.try_recv().is_err());
    let all = s.list_all_intercepts().expect("rows");
    assert!(all
        .iter()
        .any(|x| x.intercept_id == id && x.state == InterceptState::Expired));
}

#[test]
fn queue_full_expires_and_records() {
    let s = mem_store();
    let coord = InterceptCoordinator::new(1, 600_000, Some(s.clone()));
    let (id1, rx1) = coord
        .enqueue(
            "h",
            "GET",
            "/1",
            "wsw-20261119000000-1-ab15",
            "wsw-lis-itest-0001",
            "wsw-ses-itest-0001",
        )
        .expect("first");
    // 第二条 = 队满（QueueError::Full → handler 记录 + 自动放行）。
    assert!(matches!(
        coord.enqueue(
            "h",
            "GET",
            "/2",
            "wsw-20261119000000-1-ab16",
            "wsw-lis-itest-0001",
            "wsw-ses-itest-0001"
        ),
        Err(wsw_runtime::QueueError::Full)
    ));
    // 内存队列 pending=1；既有的 QueueBudget 容量语义（根原语单测）双保险。
    let mut budget = wsw_runtime::QueueBudget {
        capacity: 1,
        pending: 0,
    };
    assert!(budget.enqueue().is_ok());
    assert!(budget.enqueue().is_err());
    // 裁决第一条后容量释放。
    coord
        .resolve(&id1, InterceptResolution::Drop, &persist::now_iso())
        .expect("drop");
    assert_eq!(
        rx1.blocking_recv().expect("resolution"),
        InterceptResolution::Drop
    );
    assert!(coord
        .enqueue(
            "h",
            "GET",
            "/3",
            "wsw-20261119000000-1-ab17",
            "wsw-lis-itest-0001",
            "wsw-ses-itest-0001"
        )
        .is_ok());
}

#[test]
fn stop_drains_everything_expired() {
    let s = mem_store();
    let coord = InterceptCoordinator::new(8, 600_000, Some(s.clone()));
    for (i, ev) in ["ev-stop-1", "ev-stop-2", "ev-stop-3"].iter().enumerate() {
        s.enqueue_intercept(&row(ev, 1_000 + i as i64, 600_000))
            .expect("enqueue");
        coord
            .enqueue(
                "127.0.0.1",
                "GET",
                "/x",
                &format!("wsw-{ev}"),
                "wsw-lis-itest-0001",
                "wsw-ses-itest-0001",
            )
            .expect("mem enqueue");
    }
    assert_eq!(coord.pending(), 3);
    // 停机：内存 waiter 清 + persist 全 EXPIRED（PLAN §7.5 :553）。
    coord.shutdown(&persist::now_iso());
    s.expire_all_queued(&persist::now_iso(), "listener_stopped")
        .expect("expire all");
    assert_eq!(coord.pending(), 0);
    let queued = s.list_queued().expect("queued");
    assert!(queued.is_empty(), "stop must drain all queued: {queued:?}");
    let all = s.list_all_intercepts().expect("rows");
    let expired = all
        .iter()
        .filter(|x| x.state == InterceptState::Expired)
        .count();
    assert_eq!(
        expired, 6,
        "3 persist rows (direct) + 3 coordinator rows all expired"
    );
}

#[test]
fn reconcile_on_start_three_paths() {
    let s = mem_store();
    let now_ms = persist::iso_to_epoch_ms(&persist::now_iso()).unwrap();
    // 未过期（回载）/ 过期（→expired）/ 即将过期（回载）。
    let fresh = row("ev-re-fresh", now_ms - 1_000, 600_000);
    let stale = row("ev-re-stale", now_ms - 60_000, 10_000); // expires 早于 now
    let fresh2 = row("ev-re-fresh2", now_ms - 2_000, 600_000);
    for r in [&fresh, &stale, &fresh2] {
        s.enqueue_intercept(r).expect("enqueue");
    }
    let outcome = s
        .reconcile_on_start(&persist::now_iso(), "reconcile_expired")
        .expect("reconcile");
    // 未过期回载（2 行）+ 过期标 EXPIRED（1 行）。
    let mut reloaded_ids: Vec<&str> = outcome
        .reloaded
        .iter()
        .map(|r| r.event_id.as_str())
        .collect();
    reloaded_ids.sort_unstable();
    assert_eq!(reloaded_ids, vec!["ev-re-fresh", "ev-re-fresh2"]);
    assert_eq!(outcome.expired_count, 1);
    let all = s.list_all_intercepts().expect("rows");
    let stale_row = all
        .iter()
        .find(|x| x.event_id == "ev-re-stale")
        .expect("stale row");
    assert_eq!(stale_row.state, InterceptState::Expired);
    assert_eq!(stale_row.reason.as_deref(), Some("reconcile_expired"));
    // 回载行回灌内存队列（recover 沿用原 expires_at，非重置满 TTL）。
    let coord = InterceptCoordinator::new(8, 60_000, None);
    coord.recover(outcome.reloaded.clone());
    assert_eq!(coord.pending(), 2);
    // 回载行终态仍可推进（五态机继续有效）。
    let target = &outcome.reloaded[0];
    assert!(s
        .resolve_intercept(
            &target.intercept_id,
            InterceptState::Forwarded,
            &persist::now_iso(),
            None
        )
        .expect("resolve reloaded"));
}

#[test]
fn pending_queue_five_state_semantics_reused() {
    // 既有根原语（PendingQueue 五态）在队列桥语义下的再核对（禁改根原语）。
    let mut q = PendingQueue::new(2, 100).unwrap();
    q.enqueue("a", 0).unwrap();
    q.enqueue("b", 0).unwrap();
    assert_eq!(q.pending(), 2);
    q.resolve("a", InterceptState::ModifiedForwarded, 1)
        .unwrap();
    assert!(
        q.resolve("a", InterceptState::Forwarded, 2).is_err(),
        "terminal cannot reopen"
    );
    q.expire(200);
    assert_eq!(q.pending(), 0, "b expired past ttl");
    // ID 词表（alnum/-/_）与既有单测一致。
    assert!(q.enqueue("wsw-itc-2026-0001", 0).is_ok());
}
