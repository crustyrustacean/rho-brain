# rho-brain

A next-generation knowledge base API, built with modern Rust async web technologies.

Successor to [pi-brain](https://landruser8.tail2839f9.ts.net), reimagined with a fully normalized database schema and built on the tokio-rs ecosystem's latest web framework and ORM.

## Tech Stack

- **[topcoat](https://github.com/tokio-rs/topcoat)** — Full-stack Rust web framework (SSR + API routes)
- **[toasty](https://github.com/tokio-rs/toasty)** — Async ORM with SQLite support
- **[tokio](https://tokio.rs)** — Async runtime
- **SQLite** — Embedded database

## Features

- Document CRUD with soft-delete
- Normalized tagging system (many-to-many)
- Typed key-value metadata
- Full-text search
- Pagination support
- JSON API compatible with pi-brain

## API Endpoints

Base path: `/rb`

| Method | Path | Description |
|--------|------|-------------|
| GET | `/rb/health_check` | Health check |
| POST | `/rb/documents` | Create document |
| GET | `/rb/documents` | List documents (paginated) |
| GET | `/rb/documents/{id}` | Get single document |
| PUT | `/rb/documents/{id}` | Update document |
| DELETE | `/rb/documents/{id}` | Soft-delete document |
| POST | `/rb/search` | Full-text search |
| GET | `/rb/search` | Search via query params |
| GET | `/rb/stats` | Knowledge base statistics |

## Database Schema

```
documents:     id (UUID, PK), title, content, created_at, updated_at, deleted_at
tags:          id (i64, PK), name (unique)
document_tags: document_id + tag_id (composite PK)
metadata:      id (i64, PK), document_id, key, value_text, value_int, value_bool
```

## Getting Started

### Prerequisites

- Rust 1.75+ (edition 2024)
- Cargo

### Build

```sh
cargo build
```

### Run

```sh
cargo run
```

The server starts on `127.0.0.1:3000` by default. Set `HOST` and `PORT` environment variables to change.

### Database

The SQLite database file defaults to `rho-brain.db` in the working directory. Set `DATABASE_URL` to override.

```sh
DATABASE_URL=/path/to/db.sqlite cargo run
```

## Development

```sh
# Check compilation
cargo check

# Run tests
cargo test

# Lint
cargo clippy
```

## License

MIT — see [License.txt](License.txt)

## Author

Jeffery D. Mitchell
