//! Explicit PostgreSQL schema migration runner. Searches never migrate databases.
use intel_db::{open_pool, run_migrations, PgConfig};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("rp-db-migrate --apply\n\nApplies pending PostgreSQL migrations using RP_PG_DSN. Dry-run is the default.");
        return;
    }
    if args.as_slice() != ["--apply"] {
        eprintln!("usage: rp-db-migrate --apply (requires RP_PG_DSN; no DSN is printed)");
        std::process::exit(2);
    }
    let pool = match open_pool(&PgConfig::default()) {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("PostgreSQL connection failed: {error}");
            std::process::exit(1);
        }
    };
    let mut conn = match pool.get() {
        Ok(conn) => conn,
        Err(error) => {
            eprintln!("PostgreSQL connection checkout failed: {error}");
            std::process::exit(1);
        }
    };
    match run_migrations(&mut conn) {
        Ok(applied) => println!("{{\"status\":\"ok\",\"applied_migrations\":{:?}}}", applied),
        Err(error) => {
            eprintln!("PostgreSQL migration failed: {error}");
            std::process::exit(1);
        }
    }
}
