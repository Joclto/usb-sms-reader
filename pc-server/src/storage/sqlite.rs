use rusqlite::{Connection, Result as SqliteResult};
use chrono::{DateTime, Utc};
use super::models::SmsRecord;
use crate::error::{AppError, Result};

pub struct SmsStorage {
    conn: Connection,
}

impl SmsStorage {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path).map_err(AppError::DatabaseError)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS sms (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                sender TEXT NOT NULL,
                body TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                category TEXT NOT NULL,
                forwarded INTEGER DEFAULT 0,
                created_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(AppError::DatabaseError)?;

        Ok(SmsStorage { conn })
    }

    pub fn insert(&self, record: &SmsRecord) -> Result<i64> {
        let mut stmt = self.conn
            .prepare(
                "INSERT INTO sms (sender, body, timestamp, category, forwarded, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(AppError::DatabaseError)?;

        stmt.insert((
            &record.sender,
            &record.body,
            record.timestamp.to_rfc3339(),
            &record.category,
            if record.forwarded { 1 } else { 0 },
            record.created_at.to_rfc3339(),
        ))
        .map_err(AppError::DatabaseError)
    }

    pub fn get_latest(&self, limit: usize) -> Result<Vec<SmsRecord>> {
        let mut stmt = self.conn
            .prepare(
                "SELECT id, sender, body, timestamp, category, forwarded, created_at
                 FROM sms ORDER BY timestamp DESC LIMIT ?1",
            )
            .map_err(AppError::DatabaseError)?;

        let records = stmt
            .query_map([limit], |row| {
                Ok(SmsRecord {
                    id: row.get(0)?,
                    sender: row.get(1)?,
                    body: row.get(2)?,
                    timestamp: parse_datetime(&row.get::<_, String>(3)?),
                    category: row.get(4)?,
                    forwarded: row.get::<_, i32>(5)? != 0,
                    created_at: parse_datetime(&row.get::<_, String>(6)?),
                })
            })
            .map_err(AppError::DatabaseError)?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(AppError::DatabaseError)?;

        Ok(records)
    }

    pub fn mark_forwarded(&self, id: i64) -> Result<()> {
        self.conn
            .execute(
                "UPDATE sms SET forwarded = 1 WHERE id = ?1",
                [id],
            )
            .map_err(AppError::DatabaseError)?;
        Ok(())
    }
}

fn parse_datetime(s: &str) -> DateTime<Utc> {
    s.parse().unwrap_or_else(|_| Utc::now())
}