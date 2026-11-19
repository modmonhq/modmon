use rusqlite::Connection;
use rusqlite::types::ValueRef;

/// The database as SQL that builds it again: its tables, indexes and
/// triggers as SQLite stores them, then every row.
pub fn dump(connection: &Connection) -> rusqlite::Result<String> {
    let mut out = String::from("PRAGMA foreign_keys=OFF;\nBEGIN TRANSACTION;\n");
    let objects = connection
        .prepare(
            "SELECT type, name, sql FROM sqlite_master
             WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
             ORDER BY CASE type WHEN 'table' THEN 0 WHEN 'index' THEN 1 ELSE 2 END, rowid",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (kind, name, sql) in &objects {
        out.push_str(sql);
        out.push_str(";\n");
        if kind == "table" {
            rows(connection, name, &mut out)?;
        }
    }
    let has_sequence: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'sqlite_sequence')",
        [],
        |row| row.get(0),
    )?;
    if has_sequence {
        out.push_str("DELETE FROM sqlite_sequence;\n");
        rows(connection, "sqlite_sequence", &mut out)?;
    }
    out.push_str("COMMIT;\n");
    Ok(out)
}

fn rows(connection: &Connection, table: &str, out: &mut String) -> rusqlite::Result<()> {
    let quoted = format!("\"{}\"", table.replace('"', "\"\""));
    let mut statement = connection.prepare(&format!("SELECT * FROM {quoted} ORDER BY rowid"))?;
    let columns = statement.column_count();
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let values = (0..columns)
            .map(|index| row.get_ref(index).map(literal))
            .collect::<Result<Vec<_>, _>>()?;
        out.push_str(&format!(
            "INSERT INTO {quoted} VALUES({});\n",
            values.join(",")
        ));
    }
    Ok(())
}

fn literal(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "NULL".to_owned(),
        ValueRef::Integer(number) => number.to_string(),
        ValueRef::Real(number) => format!("{number:?}"),
        ValueRef::Text(text) => format!("'{}'", String::from_utf8_lossy(text).replace('\'', "''")),
        ValueRef::Blob(bytes) => format!(
            "X'{}'",
            bytes
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<String>()
        ),
    }
}
