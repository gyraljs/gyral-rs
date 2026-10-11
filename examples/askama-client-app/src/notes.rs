//! Native GET/POST pages. Rust is authoritative; the preview never submits.
use crate::{AppState, assets::Assets, notebook::NotebookSession};
use askama::Template;
use axum::{
    Extension, Form,
    extract::{Path, State, rejection::FormRejection},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use gyral_example_notes_app::{self as workflow, StoreError, WriteError};
use gyral_example_notes_domain::{FieldErrors, Note, NoteContent, NoteId};
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub(super) enum NotePageError {
    #[error("note not found")]
    NotFound,
    #[error("invalid note identifier")]
    Id,
    #[error("invalid form encoding")]
    Form(#[from] FormRejection),
    #[error("invalid CSRF proof")]
    Csrf,
    #[error("template rendering failed")]
    Render(#[from] askama::Error),
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error(transparent)]
    Store(#[from] StoreError),
}
impl IntoResponse for NotePageError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::NotFound | Self::Write(WriteError::NotFound) => {
                (StatusCode::NOT_FOUND, "Note not found in this notebook.")
            }
            Self::Id => (
                StatusCode::BAD_REQUEST,
                "Use a positive integer note identifier.",
            ),
            Self::Form(rejection) => (
                rejection.status(),
                "Submit the original form with title, body, and CSRF fields.",
            ),
            Self::Csrf => (
                StatusCode::FORBIDDEN,
                "This form has expired or belongs to another notebook. Reload the form and try again.",
            ),
            Self::Store(StoreError::Full) | Self::Write(WriteError::Store(StoreError::Full)) => (
                StatusCode::CONFLICT,
                "This notebook is full. Delete a note before creating another.",
            ),
            Self::Render(_)
            | Self::Store(StoreError::Unavailable | StoreError::Corrupt)
            | Self::Write(WriteError::Store(StoreError::Unavailable | StoreError::Corrupt)) => {
                tracing::error!(category = "notes_page", "notes page operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "The notebook operation could not be completed.",
                )
            }
        };
        (status, message).into_response()
    }
}

#[derive(Template)]
#[template(path = "notes-list.html")]
struct ListTemplate<'a> {
    assets: &'a Assets,
    title: &'static str,
    section: &'static str,
    notes: &'a [Note],
}
#[derive(Template)]
#[template(path = "note-detail.html")]
struct DetailTemplate<'a> {
    assets: &'a Assets,
    title: &'static str,
    section: &'static str,
    note: &'a Note,
}
#[derive(Template)]
#[template(path = "note-form.html")]
struct FormTemplate<'a> {
    assets: &'a Assets,
    title: &'static str,
    section: &'static str,
    action: &'a str,
    cancel: &'a str,
    csrf: &'a str,
    note_title: &'a str,
    body: &'a str,
    title_error: &'a str,
    body_error: &'a str,
    submission_error: &'a str,
}
#[derive(Template)]
#[template(path = "note-delete.html")]
struct DeleteTemplate<'a> {
    assets: &'a Assets,
    title: &'static str,
    section: &'static str,
    note: &'a Note,
    csrf: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawNoteForm {
    title: String,
    body: String,
    csrf: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawDeleteForm {
    csrf: String,
}

fn parse_id(raw: String) -> Result<NoteId, NotePageError> {
    NoteId::try_from(raw).map_err(|_| NotePageError::Id)
}
async fn load(
    state: &AppState,
    session: &NotebookSession,
    raw: String,
) -> Result<Note, NotePageError> {
    state
        .notes
        .find(&session.owner, parse_id(raw)?)
        .await?
        .ok_or(NotePageError::NotFound)
}

pub(super) async fn list(
    State(state): State<AppState>,
    Extension(session): Extension<NotebookSession>,
) -> Result<Html<String>, NotePageError> {
    let notes = state.notes.list(&session.owner).await?;
    Ok(Html(
        ListTemplate {
            assets: &state.assets,
            title: "Your notes",
            section: "notes",
            notes: &notes,
        }
        .render()?,
    ))
}
pub(super) async fn detail(
    State(state): State<AppState>,
    Extension(session): Extension<NotebookSession>,
    Path(raw): Path<String>,
) -> Result<Html<String>, NotePageError> {
    let note = load(&state, &session, raw).await?;
    Ok(Html(
        DetailTemplate {
            assets: &state.assets,
            title: "Read note",
            section: "notes",
            note: &note,
        }
        .render()?,
    ))
}
pub(super) async fn new_form(
    State(state): State<AppState>,
    Extension(session): Extension<NotebookSession>,
) -> Result<Html<String>, NotePageError> {
    Ok(Html(render_form(
        &state.assets,
        &session,
        Editor::New,
        "",
        "",
        None,
    )?))
}
pub(super) async fn edit_form(
    State(state): State<AppState>,
    Extension(session): Extension<NotebookSession>,
    Path(raw): Path<String>,
) -> Result<Html<String>, NotePageError> {
    let note = load(&state, &session, raw).await?;
    Ok(Html(render_form(
        &state.assets,
        &session,
        Editor::Edit(note.id),
        note.content.title.as_str(),
        note.content.body.as_str(),
        None,
    )?))
}
pub(super) async fn delete_form(
    State(state): State<AppState>,
    Extension(session): Extension<NotebookSession>,
    Path(raw): Path<String>,
) -> Result<Html<String>, NotePageError> {
    let note = load(&state, &session, raw).await?;
    Ok(Html(
        DeleteTemplate {
            assets: &state.assets,
            title: "Delete note",
            section: "notes",
            note: &note,
            csrf: &session.proof,
        }
        .render()?,
    ))
}

#[derive(Clone, Copy)]
enum Editor {
    New,
    Edit(NoteId),
}
enum FormFeedback {
    Fields(FieldErrors),
    Submission(&'static str),
}
fn render_form(
    assets: &Assets,
    session: &NotebookSession,
    editor: Editor,
    title: &str,
    body: &str,
    feedback: Option<FormFeedback>,
) -> Result<String, askama::Error> {
    let (heading, action, cancel) = match editor {
        Editor::New => ("New note", "/notes".to_owned(), "/notes".to_owned()),
        Editor::Edit(id) => (
            "Edit note",
            format!("/notes/{}", id.value()),
            format!("/notes/{}", id.value()),
        ),
    };
    let (errors, submission_error) = match feedback {
        Some(FormFeedback::Fields(errors)) => (Some(errors), ""),
        Some(FormFeedback::Submission(message)) => (None, message),
        None => (None, ""),
    };
    let title_error = errors
        .as_ref()
        .and_then(|errors| errors.title)
        .map_or_else(String::new, |error| error.to_string());
    let body_error = errors
        .as_ref()
        .and_then(|errors| errors.body)
        .map_or_else(String::new, |error| error.to_string());
    FormTemplate {
        assets,
        title: heading,
        section: "notes",
        action: &action,
        cancel: &cancel,
        csrf: &session.proof,
        note_title: title,
        body,
        title_error: &title_error,
        body_error: &body_error,
        submission_error,
    }
    .render()
}

async fn save(
    state: AppState,
    session: NotebookSession,
    editor: Editor,
    form: Result<Form<RawNoteForm>, FormRejection>,
) -> Result<Response, NotePageError> {
    let raw = form?.0;
    if state.notebooks.verify(&session.owner, &raw.csrf).is_err() {
        return Ok((StatusCode::FORBIDDEN, Html(render_form(&state.assets, &session, editor, &raw.title, &raw.body,
            Some(FormFeedback::Submission("This form has expired or belongs to another notebook. Your draft is kept below. Review it and save again.")))?)).into_response());
    }
    // A guessed edit ID must not reveal a form populated with another owner's data.
    if let Editor::Edit(id) = editor {
        state
            .notes
            .find(&session.owner, id)
            .await?
            .ok_or(NotePageError::NotFound)?;
    }
    let content = match NoteContent::try_from((raw.title.clone(), raw.body.clone())) {
        Ok(content) => content,
        Err(errors) => {
            return Ok((
                StatusCode::UNPROCESSABLE_ENTITY,
                Html(render_form(
                    &state.assets,
                    &session,
                    editor,
                    &raw.title,
                    &raw.body,
                    Some(FormFeedback::Fields(errors)),
                )?),
            )
                .into_response());
        }
    };
    let outcome = match editor {
        Editor::New => workflow::create(state.notes.as_ref(), &session.owner, &content).await,
        Editor::Edit(id) => {
            workflow::update(state.notes.as_ref(), &session.owner, id, &content).await
        }
    };
    let id = match outcome {
        Ok(id) => id,
        Err(WriteError::Store(StoreError::Full)) => return Ok((StatusCode::CONFLICT, Html(render_form(&state.assets, &session, editor, &raw.title, &raw.body,
            Some(FormFeedback::Submission("This notebook is full. Your draft is kept below. Delete a note before saving another.")))?)).into_response()),
        Err(error) => return Err(error.into()),
    };
    Ok(Redirect::to(&format!("/notes/{}", id.value())).into_response())
}

pub(super) async fn create(
    State(state): State<AppState>,
    Extension(session): Extension<NotebookSession>,
    form: Result<Form<RawNoteForm>, FormRejection>,
) -> Result<Response, NotePageError> {
    save(state, session, Editor::New, form).await
}
pub(super) async fn update(
    State(state): State<AppState>,
    Extension(session): Extension<NotebookSession>,
    Path(raw): Path<String>,
    form: Result<Form<RawNoteForm>, FormRejection>,
) -> Result<Response, NotePageError> {
    save(state, session, Editor::Edit(parse_id(raw)?), form).await
}
pub(super) async fn delete(
    State(state): State<AppState>,
    Extension(session): Extension<NotebookSession>,
    Path(raw): Path<String>,
    form: Result<Form<RawDeleteForm>, FormRejection>,
) -> Result<Redirect, NotePageError> {
    let raw_form = form?.0;
    state
        .notebooks
        .verify(&session.owner, &raw_form.csrf)
        .map_err(|_| NotePageError::Csrf)?;
    workflow::delete(state.notes.as_ref(), &session.owner, parse_id(raw)?).await?;
    Ok(Redirect::to("/notes"))
}
