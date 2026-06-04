use rusqlite::Connection;

pub fn migrate(_conn: &Connection) -> Result<(), crate::db::DbError> {
    unimplemented!()
}
