//! The SQLite compiled into the binary is current. It is bundled, so users
//! cannot get fixes from their operating system.

use rusqlite::Connection;

/// The oldest SQLite Tessera may ship: the release that fixed CVE-2025-6965.
const MINIMUM: (u32, u32, u32) = (3, 50, 2);

#[test]
fn the_bundled_sqlite_is_at_least_the_minimum() {
    let conn = Connection::open_in_memory().unwrap();
    let version: String = conn
        .query_row("SELECT sqlite_version()", [], |row| row.get(0))
        .unwrap();
    let parts: Vec<u32> = version.split('.').map(|p| p.parse().unwrap()).collect();
    let found = (parts[0], parts[1], parts[2]);
    assert!(
        found >= MINIMUM,
        "SQLite {version} is older than {}.{}.{}",
        MINIMUM.0,
        MINIMUM.1,
        MINIMUM.2
    );
}
