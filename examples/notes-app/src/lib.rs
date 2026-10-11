//! Injected storage ports and transaction ownership for the notes example.
#![forbid(unsafe_code)]
#[cfg(test)]
mod tests;

use gyral_example_notes_domain::{Note, NoteContent, NoteId, NotebookKey};

/// A storage failure translated by infrastructure.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Storage cannot complete the operation; never expose driver details.
    #[error("notes storage unavailable")]
    Unavailable,
    /// A row violates a domain invariant.
    #[error("invalid stored note")]
    Corrupt,
    /// A notebook already has the example's maximum of 200 records.
    #[error("This notebook is full. Delete a note before creating another.")]
    Full,
}

/// An uncommitted unit of work. Dropping it must roll back all pending writes.
#[async_trait::async_trait]
pub trait NoteTransaction: Send {
    /// Insert validated content in the requesting notebook.
    /// # Errors
    /// Returns a translated storage error or the notebook quota failure.
    async fn insert(
        &mut self,
        owner: &NotebookKey,
        content: &NoteContent,
    ) -> Result<NoteId, StoreError>;
    /// Update only a record owned by the requesting notebook.
    /// # Errors
    /// Returns a translated storage error.
    async fn update(
        &mut self,
        owner: &NotebookKey,
        id: NoteId,
        content: &NoteContent,
    ) -> Result<Option<NoteId>, StoreError>;
    /// Delete only a record owned by the requesting notebook.
    /// # Errors
    /// Returns a translated storage error.
    async fn delete(
        &mut self,
        owner: &NotebookKey,
        id: NoteId,
    ) -> Result<Option<NoteId>, StoreError>;
    /// Make this transaction durable, consuming its rollback guard.
    /// # Errors
    /// Returns a translated storage error.
    async fn commit(self: Box<Self>) -> Result<(), StoreError>;
}

/// Storage capabilities required by this example, with no database driver types.
#[async_trait::async_trait]
pub trait NoteStore: Send + Sync {
    /// List at most 200 records in one notebook.
    /// # Errors
    /// Returns a translated storage or row-decoding error.
    async fn list(&self, owner: &NotebookKey) -> Result<Vec<Note>, StoreError>;
    /// Find a record only within the requesting notebook.
    /// # Errors
    /// Returns a translated storage or row-decoding error.
    async fn find(&self, owner: &NotebookKey, id: NoteId) -> Result<Option<Note>, StoreError>;
    /// Start a write transaction; the workflow decides when to commit.
    /// # Errors
    /// Returns a translated storage error.
    async fn begin(&self) -> Result<Box<dyn NoteTransaction>, StoreError>;
}

/// A workflow failure; absence also covers a record in another notebook.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// Missing or inaccessible record.
    #[error("note not found")]
    NotFound,
    /// Translated infrastructure failure.
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Create a note in a workflow-owned transaction.
/// # Errors
/// Returns quota or storage failures. Uncommitted writes roll back on error.
#[tracing::instrument(skip_all, err)]
pub async fn create(
    store: &dyn NoteStore,
    owner: &NotebookKey,
    content: &NoteContent,
) -> Result<NoteId, WriteError> {
    let mut tx = store.begin().await?;
    let id = tx.insert(owner, content).await?;
    tx.commit().await?;
    Ok(id)
}

/// Update a note with ownership enforced in the atomic write.
/// # Errors
/// Returns absence or storage failures. No commit occurs on absence.
#[tracing::instrument(skip_all, err)]
pub async fn update(
    store: &dyn NoteStore,
    owner: &NotebookKey,
    id: NoteId,
    content: &NoteContent,
) -> Result<NoteId, WriteError> {
    let mut tx = store.begin().await?;
    let id = tx
        .update(owner, id, content)
        .await?
        .ok_or(WriteError::NotFound)?;
    tx.commit().await?;
    Ok(id)
}

/// Delete a note with ownership enforced in the atomic write.
/// # Errors
/// Returns absence or storage failures. No commit occurs on absence.
#[tracing::instrument(skip_all, err)]
pub async fn delete(
    store: &dyn NoteStore,
    owner: &NotebookKey,
    id: NoteId,
) -> Result<(), WriteError> {
    let mut tx = store.begin().await?;
    tx.delete(owner, id).await?.ok_or(WriteError::NotFound)?;
    tx.commit().await?;
    Ok(())
}
