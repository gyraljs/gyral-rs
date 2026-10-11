#!/usr/bin/env python3
"""Verify checked SQL metadata against migrations in an empty temporary database."""
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix="gyral-notes-schema-") as directory:
    database = Path(directory) / "schema.sqlite"
    with sqlite3.connect(database) as connection:
        for migration in sorted((ROOT / "examples/askama-client-app/migrations").glob("*.sql")):
            connection.executescript(migration.read_text())
    environment = os.environ | {"DATABASE_URL": f"sqlite://{database}", "SQLX_OFFLINE": "false"}
    subprocess.run(
        ["cargo", "sqlx", "prepare", "--workspace", "--check", "--", "--package", "gyral-askama-client-example", "--all-targets", "--locked"],
        cwd=ROOT, env=environment, check=True,
    )
