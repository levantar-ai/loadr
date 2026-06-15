# PostgreSQL plugin

`loadr-plugin-postgres` adds **PostgreSQL** as a load-test target. It is a
**native protocol plugin**: PostgreSQL support is not built into loadr core —
the heavy [`sqlx`](https://github.com/launchbadge/sqlx) driver ships only inside
this plugin's dynamic library. The plugin enables only `sqlx`'s `postgres`
feature, so it never pulls in `sqlx-mysql` (or its transitive `rsa` dependency,
RUSTSEC-2023-0071): a PostgreSQL-only build is **fully advisory-clean**. MySQL
lives in the separate [MySQL plugin](mysql.md). Once installed, a request to a
`postgres://` or `postgresql://` URL routes straight to this plugin.

The contract it uses is documented in
[Developing a plugin](developing.md#native-protocol-plugins).

## When to use

Reach for this when the thing under test *is* the database: validating a schema
or index under write pressure, sizing a connection pool, finding the row count
at which a query falls over, or proving latency holds at a steady query rate.
For an application that merely *uses* a database behind an HTTP API, test the API
with the `http` handler instead.

## Build and install

```bash
cargo build -p loadr-plugin-postgres --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-postgres/plugin.toml dist/
cp target/release/libloadr_plugin_postgres.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info postgres
```

Installing copies `plugin.toml` and the artifact into `~/.loadr/plugins/postgres/`.
The manifest declares the URL schemes the plugin serves:

```toml
[plugin]
name = "postgres"
kind = "protocol"
type = "native"
entry = "libloadr_plugin_postgres.so"
schemes = ["postgres", "postgresql"]
```

## The target URL

```
postgres://[user[:password]@]host[:port][/database][?params]
```

- **scheme** is `postgres` (alias `postgresql`).
- **port** defaults to the PostgreSQL standard port (`5432`).
- **credentials, database, and query parameters** are passed straight to the
  driver, so any URL `sqlx` accepts works here — including `?sslmode=require`
  for TLS.

## Use it in a test

List the plugin under `plugins:` and target a `postgres://` URL. The statement
and its bind parameters go in the request's **`sql`** block:

```yaml
plugins:
  - name: postgres      # or: { name: postgres, path: target/release/libloadr_plugin_postgres.so }

scenarios:
  main:
    executor: constant-vus
    vus: 10
    duration: 30s
    flow:
      - request:
          name: list cheap products
          url: postgres://loadr:loadr@db.example.com:5432/loadr
          sql:
            query: SELECT id, name, price FROM products WHERE price < $1 ORDER BY price
            params: ["50"]
          checks:
            - { type: status, equals: 1 }                 # 1 = ok, 0 = DB error
            - { type: duration, name: query is fast, max: 250ms }

      - request:
          name: insert order
          url: postgres://loadr:loadr@db.example.com:5432/loadr
          sql:
            query: INSERT INTO orders (sku, qty) VALUES ($1, $2)
            params: ["${row.sku}", "${row.qty}"]
          assert:
            - { type: status, equals: 1 }
```

A complete runnable plan is in `examples/27-postgres.yaml`.

## Expressing the query

- **`query`** — the SQL to run. Use PostgreSQL's `$1, $2, …` placeholder syntax
  for parameters.
- **`params`** — positional bind values, bound *safely* by the driver (never
  string-spliced, so there is no SQL-injection surface). Each value is given as
  text; the plugin infers a type so comparisons against numeric columns work — a
  value that parses as an integer binds as an integer, a decimal as a float,
  everything else as text.

`${...}` interpolation works in both `query` and `params`, so per-VU values and
data-feed columns flow straight into the statement. As a shorthand, a request
with no `sql` block uses its **`body`** as the query text (no parameters); an
empty query is rejected.

## Status, rows, and errors

A request **succeeds** when the query executes without a database error:

| Outcome | `status` | `error` | `extras.rows` |
|---------|----------|---------|---------------|
| SELECT / WITH / SHOW … | `1` | — | rows returned |
| INSERT / UPDATE / DELETE | `1` | — | rows affected |
| database error (bad SQL, constraint, …) | `0` | the DB message | — |
| connection failure / timeout | `0` | the transport error | — |

`extras` carries:

- `extras.backend` — `postgres`.
- `extras.rows` — rows returned (row-producing statements) or affected (DML).

The response **body** is the row count rendered as text, so `body`-based checks
and extraction still work.

## Metrics

loadr turns the plugin's response into the `postgres` metric family:

| Metric | Kind | Meaning |
|--------|------|---------|
| `postgres_reqs` | counter | queries executed |
| `postgres_req_duration` | trend (time) | per-query latency (ms) |
| `postgres_rows` | counter | total rows returned/affected |

```yaml
thresholds:
  checks: [ "rate>0.99" ]
  postgres_req_duration: [ "p(95)<100ms" ]
```

A request is marked **failed** when the query errors (response `status` 0), so
`http_req_failed` (the shared failure-rate metric) tracks DB errors too.

## Connection pooling

The plugin keeps an internal `sqlx::Pool` keyed by the full connection URI,
shared across every VU. A pool is itself a set of cheaply-cloned, reused
connections, so one per distinct URI is the correct model under load — the first
request for a URI establishes it, and all subsequent requests (any VU) reuse it.
The plugin owns a single Tokio runtime and `block_on`s the async driver, because
the protocol ABI is synchronous and carries no per-VU context across the FFI
boundary.

## Testing against a real server

The example harness brings up PostgreSQL with a seeded `products` table:

```bash
docker compose -f examples/harness/docker-compose.yml up -d postgres

LOADR_TEST_POSTGRES_URL=postgres://loadr:loadr@127.0.0.1:5432/loadr \
  cargo test -p loadr-plugin-postgres
```

The integration tests no-op when `LOADR_TEST_POSTGRES_URL` is unset.
