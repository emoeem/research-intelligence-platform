//! MA-3 TaskClaim 协作队列 + 邮箱消息集成测试（docs/rip3/MA-3-TASK.md §4
//! 判据 4/5/6；§三 S5 collab_claim 面）：
//!
//! 1. 判据 4 事务化原子领取（R1 反超）：真线程并发 N=8 同 claim_id 同刻
//!    claim——恰一胜（其余 0 行零副作用，胜方 attempt=1）；依赖门：depends_on
//!    未 completed 行 claim 0 行拒（负例）+ 依赖 completed 后可领（正例）。
//!    dsh 弱点 1 对照声明入批次报告：RIP 领取原语 = 数据库单语句 CAS
//!    （SQLite WAL 单写者串行化使原子性与 BEGIN IMMEDIATE 等价），非 dsh
//!    单进程串行状态文件。
//! 2. 判据 5 转派/接管与引擎侧记账（R3/弱点 3 反超）：transfer 仅
//!    claimed_by 本人可转（他人 CAS 0 行拒负例）→ transferred 终态 + 新
//!    claimable 行 attempt+1 → 败方领取成功；complete 仅本人可写（他人写
//!    0 行拒）；failed 终态 error_json 落行；五态词表逐字（非法词 INSERT
//!    拒断言 029 CHECK）。
//! 3. 判据 6 邮箱分记（R2）：send→fetch delivered_at 落、read_at NULL；
//!    未读行重复 fetch 可见（持久重投语义）；read 标记后 fetch 不再返回；
//!    sweep 未投递行清单/计数回显；from/to 非 caller 或已吊销 caller →
//!    校验拒（负例）。
//!
//! 并发竞争用临时文件库 + WAL + 多连接池（真实多连接 SQLite 锁竞争，非
//! 单连接内存库的池内串行假象）。

#![deny(missing_docs)]

use database::{rusqlite, DbPath, Pool, PoolConfig};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use workflow_engine::collab::{
    ClaimCreateInput, ClaimKind, ClaimRow, ClaimState, ClaimStore, CollabError, MessageSendInput,
    MessageStore,
};

const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");

/// 临时文件库 + WAL + 4 连接（真多连接竞争面；测试结束清理）。
fn wal_pool(tag: &str) -> (PathBuf, Pool) {
    let dir = std::env::temp_dir().join(format!("rip-ma3-collab-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("collab.db");
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(db.clone()),
        readonly: false,
        max_connections: 4,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    pool.enable_wal().unwrap();
    database::migrate(&pool, PathBuf::from(MIGRATIONS_DIR).as_path()).unwrap();
    (dir, pool)
}

fn add_caller(pool: &Pool, caller_id: &str) {
    pool.with_conn(|c| {
        c.execute(
            "INSERT INTO caller (caller_id, display_name, permission_level, created_at) \
             VALUES (?1, ?2, 'T1', '2026-11-22T00:00:00.000Z')",
            rusqlite::params![caller_id, format!("name-{caller_id}")],
        )
    })
    .unwrap();
}

fn sample_input(created_by: &str) -> ClaimCreateInput {
    ClaimCreateInput {
        kind: ClaimKind::AgentSubtask,
        title: "probe target under claim".to_string(),
        payload_json: r#"{"target":"example.com"}"#.to_string(),
        run_id: Some("wfr-collab-1".to_string()),
        node_id: Some("n1".to_string()),
        agent_id: None,
        depends_on_claim_id: None,
        created_by: created_by.to_string(),
    }
}

/// N 线程同刻 CAS 竞争（Barrier 对齐同刻；真线程非模拟）。
#[test]
fn concurrent_claim_exactly_one_winner() {
    const N: usize = 8;
    let (_dir, pool) = wal_pool("race");
    add_caller(&pool, "caller-a");
    for i in 0..N {
        add_caller(&pool, &format!("caller-w{i}"));
    }
    let store = ClaimStore::new(pool.clone());
    let row = store.create(&sample_input("caller-a")).unwrap();
    let claim_id = row.claim_id.clone();

    let wins = Arc::new(AtomicUsize::new(0));
    let losers = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    let barrier = Arc::new(std::sync::Barrier::new(N));
    for i in 0..N {
        let store = store.clone();
        let claim_id = claim_id.clone();
        let barrier = barrier.clone();
        let wins = wins.clone();
        let losers = losers.clone();
        let caller = format!("caller-w{i}");
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            match store.claim(&claim_id, &caller) {
                Ok(claimed) => {
                    assert_eq!(claimed.state, ClaimState::Claimed);
                    assert_eq!(claimed.claimed_by.as_deref(), Some(caller.as_str()));
                    wins.fetch_add(1, Ordering::SeqCst);
                }
                Err(CollabError::NotClaimable { .. }) => {
                    losers.fetch_add(1, Ordering::SeqCst);
                }
                Err(e) => panic!("unexpected error {e:?}"),
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    // 恰一胜 + 败方 0 行零副作用（NotClaimable 词形，行未被败方改写）。
    assert_eq!(wins.load(Ordering::SeqCst), 1, "恰一胜");
    assert_eq!(losers.load(Ordering::SeqCst), N - 1, "其余全败");
    let shown = store.show(&claim_id).unwrap();
    assert_eq!(shown.state, ClaimState::Claimed);
    assert_eq!(shown.attempt, 1, "胜方 attempt=1");
    assert!(
        shown.claimed_by.as_deref().unwrap().starts_with("caller-w"),
        "胜方为并发线程之一: {:?}",
        shown.claimed_by
    );
    // 行面零副作用：claimed_at/updated_at 单次落值，attempt 未被败方推进。
    let claim_events: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM task_claim WHERE claim_id = ?1",
                [claim_id.as_str()],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(claim_events, 1, "竞争全程单行（无 attempt 副产物行）");
}

#[test]
fn dependency_gate_blocks_and_releases() {
    let (_dir, pool) = wal_pool("dep");
    add_caller(&pool, "caller-a");
    add_caller(&pool, "caller-b");
    let store = ClaimStore::new(pool.clone());
    // 负例：depends_on 未 completed（claimable）行 claim 0 行拒。
    let blocking = store.create(&sample_input("caller-a")).unwrap();
    let mut gated = sample_input("caller-a");
    gated.depends_on_claim_id = Some(blocking.claim_id.clone());
    let gated = store.create(&gated).unwrap();
    let err = store
        .claim(&gated.claim_id, "caller-b")
        .expect_err("dependency gate");
    assert!(matches!(err, CollabError::DependencyNotCompleted(_)));
    let shown = store.show(&gated.claim_id).unwrap();
    assert_eq!(shown.state, ClaimState::Claimable, "依赖门拒零副作用");
    // 正例：依赖 completed 后可领。
    store.claim(&blocking.claim_id, "caller-a").unwrap();
    store
        .complete(&blocking.claim_id, "caller-a", r#"{"ok":true}"#)
        .unwrap();
    let claimed = store.claim(&gated.claim_id, "caller-b").unwrap();
    assert_eq!(claimed.state, ClaimState::Claimed);
    assert_eq!(claimed.claimed_by.as_deref(), Some("caller-b"));
}

#[test]
fn transfer_attempt_chain_and_claimer_only_writes() {
    let (_dir, pool) = wal_pool("transfer");
    add_caller(&pool, "caller-a");
    add_caller(&pool, "caller-b");
    add_caller(&pool, "caller-c");
    let store = ClaimStore::new(pool.clone());
    let row = store.create(&sample_input("caller-a")).unwrap();
    // complete 仅本人可写：未领取行（claimable）complete 拒。
    let err = store
        .complete(&row.claim_id, "caller-b", r#"{}"#)
        .expect_err("unclaimed complete");
    assert!(matches!(err, CollabError::NotClaimable { .. }));
    store.claim(&row.claim_id, "caller-b").unwrap();
    // 他人 complete/fail/transfer 全拒（NotClaimer；0 行零副作用）。
    for op_err in [
        store
            .complete(&row.claim_id, "caller-a", r#"{}"#)
            .unwrap_err(),
        store.fail(&row.claim_id, "caller-a", r#"{}"#).unwrap_err(),
        store.transfer(&row.claim_id, "caller-a").unwrap_err(),
    ] {
        assert!(
            matches!(op_err, CollabError::NotClaimer(_)),
            "got {op_err:?}"
        );
    }
    let unchanged = store.show(&row.claim_id).unwrap();
    assert_eq!(unchanged.state, ClaimState::Claimed, "他人写 0 行零副作用");
    // 本人 transfer：transferred 终态 + 新 claimable 行 attempt+1。
    let new_row = store.transfer(&row.claim_id, "caller-b").unwrap();
    assert_eq!(new_row.state, ClaimState::Claimable);
    assert_eq!(new_row.attempt, 2);
    assert_eq!(new_row.claimed_by, None);
    assert_eq!(new_row.run_id, row.run_id);
    assert_eq!(new_row.payload_json, row.payload_json);
    let old = store.show(&row.claim_id).unwrap();
    assert_eq!(old.state, ClaimState::Transferred);
    assert!(old.result_json.is_none());
    // 败方（caller-c）对新行领取成功（R3 败方接新 attempt）。
    let reclaimed = store.claim(&new_row.claim_id, "caller-c").unwrap();
    assert_eq!(reclaimed.claimed_by.as_deref(), Some("caller-c"));
    assert_eq!(reclaimed.attempt, 2);
    // failed 终态 error_json 落行。
    store
        .fail(&new_row.claim_id, "caller-c", r#"{"error":"timeout"}"#)
        .unwrap();
    let failed = store.show(&new_row.claim_id).unwrap();
    assert_eq!(failed.state, ClaimState::Failed);
    assert_eq!(failed.error_json.as_deref(), Some(r#"{"error":"timeout"}"#));
}

/// 五态词表逐字（非法词 INSERT 拒断言 029 CHECK；词表五态逐字在位）。
#[test]
fn state_wordlist_is_enforced_by_029_check() {
    let (_dir, pool) = wal_pool("wordlist");
    add_caller(&pool, "caller-a");
    for state in ["claimable", "claimed", "completed", "failed", "transferred"] {
        let ok: i64 = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM task_claim WHERE state = ?1",
                    [state],
                    |r| r.get(0),
                )
            })
            .unwrap();
        let _ = ok;
        assert!(
            ClaimState::parse(state).is_ok(),
            "词表五态逐字 {state} 必须在 ClaimState::parse 词表"
        );
    }
    // 非法词 INSERT 拒（029 CHECK）。
    let err = pool
        .with_conn(|c| {
            c.execute(
                "INSERT INTO task_claim (claim_id, kind, title, payload_json, state, attempt, \
                 created_by, created_at, updated_at) \
                 VALUES ('tcl-bad', 'agent_subtask', 'x', '{}', 'flying', 1, 'caller-a', \
                 '2026-11-22T00:00:00.000Z', '2026-11-22T00:00:00.000Z')",
                [],
            )
        })
        .unwrap_err();
    assert!(
        err.to_string().contains("CHECK"),
        "029 state CHECK 必须拒非法词: {err}"
    );
    // kind 词表外拒。
    let err = pool
        .with_conn(|c| {
            c.execute(
                "INSERT INTO task_claim (claim_id, kind, title, payload_json, state, attempt, \
                 created_by, created_at, updated_at) \
                 VALUES ('tcl-bad2', 'merge', 'x', '{}', 'claimable', 1, 'caller-a', \
                 '2026-11-22T00:00:00.000Z', '2026-11-22T00:00:00.000Z')",
                [],
            )
        })
        .unwrap_err();
    assert!(
        err.to_string().contains("CHECK"),
        "029 kind CHECK 必须拒词表外值: {err}"
    );
}

/// 邮箱分记全序（判据 6）：send→fetch（delivered 落 / read NULL）→重复
/// fetch 可见（持久重投）→read→不再返回→sweep 计数回显→负例（未知/
/// 吊销 caller）。
#[test]
fn mailbox_delivery_read_sweep_full_sequence() {
    let (_dir, pool) = wal_pool("mailbox");
    add_caller(&pool, "caller-a");
    add_caller(&pool, "caller-b");
    let store = MessageStore::new(pool.clone());
    let m1 = store
        .send(&MessageSendInput {
            from_caller: "caller-a".to_string(),
            to_caller: "caller-b".to_string(),
            payload_json: r#"{"seq":1}"#.to_string(),
            claim_id: None,
            run_id: Some("wfr-collab-1".to_string()),
        })
        .unwrap();
    let m2 = store
        .send(&MessageSendInput {
            from_caller: "caller-a".to_string(),
            to_caller: "caller-b".to_string(),
            payload_json: r#"{"seq":2}"#.to_string(),
            claim_id: None,
            run_id: None,
        })
        .unwrap();
    // sweep：未投递计数 = 2。
    let (rows, total) = store.sweep(100).unwrap();
    assert_eq!(total, 2);
    assert_eq!(rows.len(), 2);
    // fetch：delivered_at 落、read_at NULL（分记时序）。
    let fetched = store.fetch("caller-b", 100).unwrap();
    assert_eq!(fetched.len(), 2);
    assert!(
        fetched
            .iter()
            .all(|m| m.delivered_at.is_some() && m.read_at.is_none()),
        "首拉落 delivered_at 不动 read_at"
    );
    // sweep 归零（已投递）。
    let (_, total) = store.sweep(100).unwrap();
    assert_eq!(total, 0);
    // 未读行重复 fetch 可见（持久重投语义）。
    let again = store.fetch("caller-b", 100).unwrap();
    assert_eq!(again.len(), 2);
    // read 标记（单语句 CAS）后不再返回。
    store.read(&m1.message_id, "caller-b").unwrap();
    let after_read = store.fetch("caller-b", 100).unwrap();
    assert_eq!(after_read.len(), 1);
    assert_eq!(after_read[0].message_id, m2.message_id);
    // 已读消息 direct read 再标 = 拒（read_at IS NULL 守卫）。
    let err = store.read(&m1.message_id, "caller-b").unwrap_err();
    assert!(matches!(err, CollabError::NotClaimable { .. }));
    // 他人邮箱读取拒（to_caller 守卫）。
    add_caller(&pool, "caller-c");
    let err = store.read(&m2.message_id, "caller-c").unwrap_err();
    assert!(matches!(err, CollabError::NotClaimable { .. }));
    // 未知收件人 send 拒 + 吊销收件人 send 拒。
    let err = store
        .send(&MessageSendInput {
            from_caller: "caller-a".to_string(),
            to_caller: "caller-ghost".to_string(),
            payload_json: "{}".to_string(),
            claim_id: None,
            run_id: None,
        })
        .unwrap_err();
    assert!(matches!(err, CollabError::InvalidInput(_)));
    pool.with_conn(|c| {
        c.execute(
            "UPDATE caller SET revoked_at = '2026-11-22T01:00:00.000Z' WHERE caller_id = 'caller-c'",
            [],
        )
    })
    .unwrap();
    let err = store
        .send(&MessageSendInput {
            from_caller: "caller-a".to_string(),
            to_caller: "caller-c".to_string(),
            payload_json: "{}".to_string(),
            claim_id: None,
            run_id: None,
        })
        .unwrap_err();
    assert!(
        matches!(err, CollabError::InvalidInput(_)),
        "吊销 caller 拒收: {err:?}"
    );
}

/// ready_list 与 claim-list CLI 读面同源（依赖门过滤 + created_at 升序）。
#[test]
fn ready_list_orders_and_filters() {
    let (_dir, pool) = wal_pool("ready");
    add_caller(&pool, "caller-a");
    let store = ClaimStore::new(pool.clone());
    let a = store
        .create(&ClaimCreateInput {
            kind: ClaimKind::Review,
            title: "review card kc-1".into(),
            payload_json: r#"{"card":"kc-1"}"#.into(),
            run_id: None,
            node_id: None,
            agent_id: None,
            depends_on_claim_id: None,
            created_by: "caller-a".into(),
        })
        .unwrap();
    let mut gated = sample_input("caller-a");
    gated.depends_on_claim_id = Some(a.claim_id.clone());
    let gated = store.create(&gated).unwrap();
    let ready = store.ready_list(10).unwrap();
    let ids: Vec<&str> = ready
        .iter()
        .map(|r: &ClaimRow| r.claim_id.as_str())
        .collect();
    assert_eq!(ids, vec![a.claim_id.as_str()], "依赖门过滤 + 升序");
    assert!(ready[0].kind == ClaimKind::Review, "kind 词形透传");
    let _ = gated;
}
