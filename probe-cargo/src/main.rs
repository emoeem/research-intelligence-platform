use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

#[derive(Serialize)]
struct Row { name: String }

fn main() -> Result<()> {
    let conn = Connection::open_in_memory()?;
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master LIMIT 3")?;
    let rows: Vec<Row> = stmt.query_map([], |r| Ok(Row { name: r.get(0)? }))?
        .collect::<std::result::Result<_, _>>()?;
    println!("{}", serde_json::to_string(&rows)?);
    Ok(())
}
