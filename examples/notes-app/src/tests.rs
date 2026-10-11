#![allow(clippy::expect_used)]
use super::*;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
enum Failure {
    None,
    Insert,
    Commit,
}
struct FakeStore {
    saved: Arc<Mutex<Vec<Note>>>,
    failure: Failure,
}
struct FakeTransaction {
    saved: Arc<Mutex<Vec<Note>>>,
    staged: Vec<Note>,
    failure: Failure,
}

#[async_trait::async_trait]
impl NoteStore for FakeStore {
    async fn list(&self, _owner: &NotebookKey) -> Result<Vec<Note>, StoreError> {
        Ok(self
            .saved
            .lock()
            .expect("invariant: test lock is not poisoned")
            .clone())
    }
    async fn find(&self, _owner: &NotebookKey, id: NoteId) -> Result<Option<Note>, StoreError> {
        Ok(self
            .saved
            .lock()
            .expect("invariant: test lock is not poisoned")
            .iter()
            .find(|n| n.id == id)
            .cloned())
    }
    async fn begin(&self) -> Result<Box<dyn NoteTransaction>, StoreError> {
        let staged = self
            .saved
            .lock()
            .expect("invariant: test lock is not poisoned")
            .clone();
        Ok(Box::new(FakeTransaction {
            saved: Arc::clone(&self.saved),
            staged,
            failure: self.failure,
        }))
    }
}
#[async_trait::async_trait]
impl NoteTransaction for FakeTransaction {
    async fn insert(
        &mut self,
        _owner: &NotebookKey,
        content: &NoteContent,
    ) -> Result<NoteId, StoreError> {
        if matches!(self.failure, Failure::Insert) {
            return Err(StoreError::Unavailable);
        }
        let id = NoteId::try_from(1).expect("invariant: synthetic ID is positive");
        self.staged.push(Note {
            id,
            content: content.clone(),
        });
        Ok(id)
    }
    async fn update(
        &mut self,
        _owner: &NotebookKey,
        id: NoteId,
        content: &NoteContent,
    ) -> Result<Option<NoteId>, StoreError> {
        let Some(note) = self.staged.iter_mut().find(|note| note.id == id) else {
            return Ok(None);
        };
        note.content = content.clone();
        Ok(Some(id))
    }
    async fn delete(
        &mut self,
        _owner: &NotebookKey,
        id: NoteId,
    ) -> Result<Option<NoteId>, StoreError> {
        let found = self.staged.iter().any(|note| note.id == id);
        self.staged.retain(|note| note.id != id);
        Ok(found.then_some(id))
    }
    async fn commit(self: Box<Self>) -> Result<(), StoreError> {
        if matches!(self.failure, Failure::Commit) {
            return Err(StoreError::Unavailable);
        }
        *self
            .saved
            .lock()
            .expect("invariant: test lock is not poisoned") = self.staged;
        Ok(())
    }
}

fn fixture(failure: Failure) -> (FakeStore, NotebookKey, NoteContent) {
    (
        FakeStore {
            saved: Arc::new(Mutex::new(Vec::new())),
            failure,
        },
        NotebookKey::try_from("a".repeat(64)).expect("invariant: synthetic key is valid"),
        NoteContent::try_from(("Draft".to_owned(), "Synthetic body".to_owned()))
            .expect("invariant: synthetic content is valid"),
    )
}

#[tokio::test]
async fn failures_before_and_during_commit_leave_no_partial_record() {
    for failure in [Failure::Insert, Failure::Commit] {
        let (store, owner, content) = fixture(failure);
        assert!(matches!(
            create(&store, &owner, &content).await,
            Err(WriteError::Store(StoreError::Unavailable))
        ));
        assert!(
            store
                .list(&owner)
                .await
                .expect("invariant: fake is readable")
                .is_empty()
        );
    }
}

#[tokio::test]
async fn workflows_commit_changes_and_preserve_state_when_records_are_missing() {
    let (store, owner, content) = fixture(Failure::None);
    let id = create(&store, &owner, &content)
        .await
        .expect("invariant: healthy fake accepts writes");
    let missing = NoteId::try_from(2).expect("invariant: synthetic ID is positive");
    assert!(matches!(
        update(&store, &owner, missing, &content).await,
        Err(WriteError::NotFound)
    ));
    assert!(matches!(
        delete(&store, &owner, missing).await,
        Err(WriteError::NotFound)
    ));
    assert_eq!(
        store
            .list(&owner)
            .await
            .expect("invariant: fake is readable"),
        vec![Note {
            id,
            content: content.clone()
        }]
    );
    let changed = NoteContent::try_from(("Edited".to_owned(), String::new()))
        .expect("invariant: synthetic content is valid");
    assert_eq!(
        update(&store, &owner, id, &changed)
            .await
            .expect("invariant: healthy fake accepts writes"),
        id
    );
    assert_eq!(
        store
            .find(&owner, id)
            .await
            .expect("invariant: fake is readable")
            .map(|n| n.content),
        Some(changed)
    );
    delete(&store, &owner, id)
        .await
        .expect("invariant: healthy fake accepts writes");
    assert!(
        store
            .list(&owner)
            .await
            .expect("invariant: fake is readable")
            .is_empty()
    );
}
