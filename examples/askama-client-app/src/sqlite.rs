//! SQLx adapter. Workflows own transactions; every row is decoded again.
use gyral_example_notes_app::{NoteStore, NoteTransaction, StoreError};
use gyral_example_notes_domain::{Note, NoteContent, NoteId, NotebookKey};
use sqlx::{
    ConnectOptions as _, Sqlite, SqlitePool, Transaction,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::{path::Path, time::Duration};

#[derive(Debug, thiserror::Error)]
pub(super) enum OpenError {
    #[error("could not open the notes database")]
    Database(#[from] sqlx::Error),
    #[error("could not migrate the notes database")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

#[derive(Debug, Clone)]
pub(super) struct SqliteNotes {
    pool: SqlitePool,
}

impl SqliteNotes {
    pub(super) async fn open(path: &Path) -> Result<Self, OpenError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(3))
            .disable_statement_logging();
        let pool = SqlitePoolOptions::new()
            .max_connections(if path == Path::new(":memory:") { 1 } else { 4 })
            .acquire_timeout(Duration::from_secs(3))
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub(super) async fn close(&self) {
        self.pool.close().await;
    }
}

#[derive(Debug)]
struct NoteRow {
    id: i64,
    title: String,
    body: String,
}
impl TryFrom<NoteRow> for Note {
    type Error = StoreError;
    fn try_from(row: NoteRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: NoteId::try_from(row.id).map_err(|_| StoreError::Corrupt)?,
            content: NoteContent::try_from((row.title, row.body))
                .map_err(|_| StoreError::Corrupt)?,
        })
    }
}

fn unavailable(_error: sqlx::Error) -> StoreError {
    // Driver errors may contain bind values. Log a category, never the error.
    tracing::error!(category = "notes_storage", "notes storage operation failed");
    StoreError::Unavailable
}

#[async_trait::async_trait]
impl NoteStore for SqliteNotes {
    async fn list(&self, owner: &NotebookKey) -> Result<Vec<Note>, StoreError> {
        let owner = owner.expose();
        sqlx::query_as!(
            NoteRow,
            "SELECT id, title, body FROM notes WHERE owner = ? ORDER BY id DESC LIMIT 200",
            owner
        )
        .fetch_all(&self.pool)
        .await
        .map_err(unavailable)?
        .into_iter()
        .map(Note::try_from)
        .collect()
    }

    async fn find(&self, owner: &NotebookKey, id: NoteId) -> Result<Option<Note>, StoreError> {
        let owner = owner.expose();
        let id = id.value();
        sqlx::query_as!(
            NoteRow,
            "SELECT id, title, body FROM notes WHERE owner = ? AND id = ?",
            owner,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(unavailable)?
        .map(Note::try_from)
        .transpose()
    }

    async fn begin(&self) -> Result<Box<dyn NoteTransaction>, StoreError> {
        // Acquire the SQLite write reservation before quota checks or mutation.
        let tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(unavailable)?;
        Ok(Box::new(SqliteNoteTransaction(tx)))
    }
}

struct SqliteNoteTransaction(Transaction<'static, Sqlite>);

#[async_trait::async_trait]
impl NoteTransaction for SqliteNoteTransaction {
    async fn insert(
        &mut self,
        owner: &NotebookKey,
        content: &NoteContent,
    ) -> Result<NoteId, StoreError> {
        let owner = owner.expose();
        let title = content.title.as_str();
        let body = content.body.as_str();
        let row = sqlx::query!("INSERT INTO notes(owner, title, body) SELECT ?, ?, ? WHERE (SELECT count(*) FROM notes WHERE owner = ?) < 200 RETURNING id", owner, title, body, owner)
            .fetch_optional(&mut *self.0).await.map_err(unavailable)?.ok_or(StoreError::Full)?;
        NoteId::try_from(row.id).map_err(|_| StoreError::Corrupt)
    }

    async fn update(
        &mut self,
        owner: &NotebookKey,
        id: NoteId,
        content: &NoteContent,
    ) -> Result<Option<NoteId>, StoreError> {
        let owner = owner.expose();
        let id = id.value();
        let title = content.title.as_str();
        let body = content.body.as_str();
        sqlx::query!(
            "UPDATE notes SET title = ?, body = ? WHERE owner = ? AND id = ? RETURNING id",
            title,
            body,
            owner,
            id
        )
        .fetch_optional(&mut *self.0)
        .await
        .map_err(unavailable)?
        .map(|row| NoteId::try_from(row.id).map_err(|_| StoreError::Corrupt))
        .transpose()
    }

    async fn delete(
        &mut self,
        owner: &NotebookKey,
        id: NoteId,
    ) -> Result<Option<NoteId>, StoreError> {
        let owner = owner.expose();
        let id = id.value();
        sqlx::query!(
            "DELETE FROM notes WHERE owner = ? AND id = ? RETURNING id",
            owner,
            id
        )
        .fetch_optional(&mut *self.0)
        .await
        .map_err(unavailable)?
        .map(|row| NoteId::try_from(row.id).map_err(|_| StoreError::Corrupt))
        .transpose()
    }

    async fn commit(self: Box<Self>) -> Result<(), StoreError> {
        self.0.commit().await.map_err(unavailable)
    }
}
