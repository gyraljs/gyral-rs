CREATE TABLE notes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    owner TEXT NOT NULL CHECK (length(owner) = 64),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 100),
    body TEXT NOT NULL CHECK (length(body) <= 4000)
) STRICT;
CREATE INDEX notes_owner_id ON notes(owner, id DESC);
