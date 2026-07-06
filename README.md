# Citra PetCare API

Internal REST API for **Citra PetCare**, a two-vet small-animal clinic:
owners & patients, visit records with attachments, vaccinations with due-date
reminders, appointments, inventory with a stock-movement ledger, a dashboard
summary, presigned file storage, push notifications, and an offline-sync
change feed for the Flutter app.

Built in Rust on axum + tokio + sqlx (PostgreSQL 16), documented with OpenAPI
(Swagger UI at `/docs`), observable via structured JSON logs and Prometheus
metrics, and shipped as a ~15 MB distroless container.

---

## Stack at a glance

| Concern        | Choice |
| -------------- | ------ |
| HTTP           | axum 0.8 on tokio, tower / tower-http middleware (request-id, tracing, panic catcher, timeout, CORS allowlist, per-IP rate limit via tower_governor, body limit, security headers), graceful shutdown on SIGTERM/SIGINT |
| Database       | PostgreSQL 16, sqlx 0.9 with compile-time-checked queries (`query!`), embedded migrations, offline build via committed `.sqlx/` |
| OpenAPI        | utoipa 5 (code-first), Swagger UI at `/docs`, spec at `/docs/openapi.json` — complete enough to generate a Dart client |
| Auth           | JWT access tokens (15 min) + rotating refresh tokens (30 days, SHA-256-hashed at rest, reuse detection kills the session family), argon2id password hashing. No public registration — users are seeded. |
| Validation     | validator crate wired into a `ValidatedJson` extractor → RFC 7807 responses with per-field errors |
| Errors         | `thiserror` domain enums → central `AppError` → `application/problem+json` on every error path |
| File storage   | `Storage` trait: S3 driver for **OCI Object Storage (S3 Compatibility API)** with presigned PUT/GET, or a local-disk driver for dev with HMAC-signed URLs |
| Notifications  | `Notifier` trait: FCM HTTP v1 driver (service account via gcp_auth, topic-based) or a log driver for dev |
| Scheduler      | tokio-cron-scheduler, daily 07:00 **Asia/Jakarta** reminder job |
| Observability  | tracing (JSON logs, one span per request with request-id), `/healthz`, `/readyz`, Prometheus `/metrics` (+ optional Prometheus/Grafana compose profile) |
| IDs            | UUIDv7 primary keys, client-suppliable; every table has `created_at` / `updated_at` / `deleted_at` (soft delete) for offline sync |

## Repository layout

```
src/
  main.rs               # CLI wiring only: serve | migrate | seed
  config.rs             # typed env config; boot fails listing *every* problem
  db.rs                 # pool + embedded migrator
  error.rs              # central AppError → RFC 7807
  state.rs              # AppState { PgPool, Config, Arc<dyn Storage>, Arc<dyn Notifier> }
  telemetry.rs          # tracing + Prometheus recorder
  scheduler.rs          # daily reminder job (07:00 Asia/Jakarta)
  seed.rs               # demo data (Indonesian sample clinic)
  http/                 # router, middleware, extractors, pagination, RFC 7807, OpenAPI doc
  domain/<module>/      # auth, users, owners, patients, visits, vaccinations,
                        # appointments, inventory, dashboard, sync, notifications, storage
    handlers.rs         #   thin HTTP layer (+ router() with utoipa annotations)
    service.rs          #   business rules
    repo.rs             #   ALL SQL lives here; rows are mapped to domain types
    models.rs / dto.rs  #   domain types / request- & response-shapes
migrations/             # sqlx migrations (embedded into the binary)
tests/                  # integration tests (testcontainers + tower::oneshot)
deploy/                 # nginx vhost, backup script, Prometheus + Grafana provisioning
```

Layering rule: handlers never touch SQL, repos never build HTTP responses,
and sqlx rows never cross the repo boundary.

---

## Local development

Prereqs: Rust (stable, ≥ 1.90), Docker, `sqlx-cli`
(`cargo install sqlx-cli --no-default-features --features postgres,rustls`).

```bash
cp .env.example .env                       # then set JWT_SECRET (openssl rand -hex 32)

# a throwaway Postgres for development
docker run -d --name petcare-pg -e POSTGRES_USER=petcare -e POSTGRES_PASSWORD=petcare \
  -e POSTGRES_DB=petcare -p 127.0.0.1:5433:5432 postgres:16-alpine

make migrate                               # apply migrations (or: cargo run -- migrate)
make seed                                  # demo data — prints the login credentials
make dev                                   # run the API on :8080
```

Now open <http://localhost:8080/docs> and log in with
`citra@citrapetcare.id` / `PetCare#2026` (override with `SEED_PASSWORD`).

Everyday targets: `make fmt lint test` (see `make help` for all).

### The sqlx offline workflow (important!)

SQL in `query!` / `query_as!` macros is **checked against a real database at
compile time**. So that CI and Docker can build without one, the check results
are cached in the committed `.sqlx/` directory.

* While developing you have `DATABASE_URL` pointing at the dev Postgres, and
  the macros check live.
* **After adding or changing any query or migration**, regenerate the cache
  and commit it:

  ```bash
  make prepare        # = cargo sqlx prepare
  git add .sqlx
  ```

* CI runs `cargo sqlx prepare --check` and fails if you forgot.
* Docker/CI builds set `SQLX_OFFLINE=true` and read `.sqlx/` only.

### Tests

```bash
make test
```

* Unit tests live next to the code (config collection, error mapping,
  pagination, password hashing, HMAC URL signing).
* Integration tests (`tests/`) start a disposable **Postgres 16
  testcontainer**, run the embedded migrations, and drive the real router
  in-process via `tower::ServiceExt::oneshot` — covering the auth flow
  (login / refresh rotation / reuse detection / logout), patients CRUD with
  soft delete, the sync change feed with tombstones, and inventory stock
  derivation. Docker must be running.

---

## API conventions

* Base path `/api/v1`; everything except `POST /auth/login` and
  `POST /auth/refresh` requires `Authorization: Bearer <access token>`.
* JSON is camelCase. Enums are SCREAMING_SNAKE (`CAT`, `NO_SHOW`, …).
* Lists use cursor pagination: `?cursor=&limit=` →
  `{ "data": [...], "meta": { "limit", "nextCursor", "hasMore" } }`.
* Every error — auth, validation, 404s, panics, rate limits — is RFC 7807
  `application/problem+json` with a stable `type` URN
  (e.g. `urn:citra-petcare:problem:insufficient-stock`) to switch on in Dart.
* Validation failures (422) carry an `errors` map of per-field messages.

### Offline sync contract

* All primary keys are UUIDv7 and **may be generated by the client**.
* `PUT /{entity}/{id}` is an idempotent full-representation upsert — replaying
  it is safe; upserting a soft-deleted row resurrects it.
* `GET /api/v1/sync/changes?since=<RFC3339>` returns per-entity
  `{ upserts, tombstones }`; use the response's `serverTime` as the next
  `since`. Rows may repeat across pulls (never lost) — upserts make that safe.
* `updated_at` is bumped server-side by a trigger and is the change cursor.

### File uploads

1. `POST /api/v1/storage/presign-upload { fileName, contentType }` →
   `{ key, url, method, headers, expiresAt }`.
2. `PUT` the bytes to `url` with the returned headers (goes directly to OCI —
   the API never proxies file bytes).
3. Store `key` on the entity (`photoKey`, attachment `fileKey`, …).
4. To display: `GET /api/v1/storage/presign-download/{key}` → short-lived GET URL.

### Generating the Dart client

```bash
curl -s http://localhost:8080/docs/openapi.json -o openapi.json
dart pub global run openapi_generator_cli generate -i openapi.json -g dart-dio -o petcare_client
```

---

## Configuration

All configuration comes from the environment (`.env` supported in dev; see
`.env.example` for the full annotated list). Boot fails fast and prints
**every** missing/invalid variable at once. Highlights:

| Variable | Purpose |
| --- | --- |
| `DATABASE_URL` (required) | Postgres connection string |
| `JWT_SECRET` (required, ≥32 chars) | HS256 signing key |
| `AUTO_MIGRATE` | apply migrations on server start (default `true`) |
| `STORAGE_DRIVER` | `local` (dev) or `s3` (OCI Object Storage) |
| `S3_ENDPOINT` | `https://{namespace}.compat.objectstorage.{region}.oraclecloud.com` |
| `S3_ACCESS_KEY_ID` / `S3_SECRET_ACCESS_KEY` | OCI **Customer Secret Keys** |
| `NOTIFIER_DRIVER` | `log` (dev) or `fcm` |
| `FCM_SERVICE_ACCOUNT_PATH` / `FCM_TOPIC` | Firebase service-account JSON + topic both phones subscribe to |
| `RATE_LIMIT_*`, `REQUEST_TIMEOUT_SECS`, `BODY_LIMIT_BYTES`, `CORS_ALLOWED_ORIGINS` | HTTP hardening knobs |

OCI S3-compat specifics are baked into the driver: path-style addressing and
checksum calculation "when required" (OCI rejects `aws-chunked` encoding).

## Observability

* **Logs**: structured JSON (`LOG_FORMAT=json`), one span per request carrying
  the `x-request-id` (generated if absent, echoed in the response).
* **Health**: `/healthz` (liveness), `/readyz` (checks the DB pool, use for
  LB/compose health checks).
* **Metrics**: Prometheus at `/metrics` — `http_requests_total` and an
  `http_request_duration_seconds` histogram labelled by method / route
  template / status. Never expose publicly (the nginx vhost blocks it).
* **Dashboards**: `docker compose --profile observability up -d` starts
  Prometheus + Grafana (localhost:3000, admin/admin) with a provisioned
  API dashboard (request rate, p50/p95/p99, error rate).

## Scheduled job

Daily at **07:00 Asia/Jakarta**: vaccinations due within 3 days or overdue,
items at/below minimum stock, and drugs expiring within 30 days are collected,
persisted as `notifications` rows (visible via `GET /api/v1/notifications`)
and pushed through the Notifier (FCM topic in production). Each category
fires at most once per Jakarta calendar day, so restarts never double-send.

---

## Deployment — Ubuntu VPS on Oracle Cloud (Docker Compose + Nginx + Cloudflare)

Target: an always-free OCI Ampere VPS, Cloudflare in front (orange cloud),
nginx terminating TLS on the box, the API + Postgres in Docker Compose.

### 1. Provision

```bash
sudo apt-get update && sudo apt-get install -y docker.io docker-compose-v2 nginx certbot python3-certbot-nginx awscli
sudo usermod -aG docker $USER   # re-login afterwards
```

Open ports 80/443 in the OCI security list **and** ufw; the API port 8080
stays loopback-only (compose binds it on the host, nginx proxies to it).

### 2. App

```bash
sudo mkdir -p /opt/citra-petcare && sudo chown $USER /opt/citra-petcare
git clone <this repo> /opt/citra-petcare && cd /opt/citra-petcare

cat > .env <<'EOF'
JWT_SECRET=<openssl rand -hex 32>
POSTGRES_PASSWORD=<openssl rand -hex 16>
PUBLIC_BASE_URL=https://be-petcare.holo.my.id
CORS_ALLOWED_ORIGINS=https://app.petcare.holo.my.id
STORAGE_DRIVER=s3
S3_ENDPOINT=https://<namespace>.compat.objectstorage.<region>.oraclecloud.com
S3_REGION=<region>
S3_BUCKET=petcare-files
S3_ACCESS_KEY_ID=<customer secret key id>
S3_SECRET_ACCESS_KEY=<customer secret key>
NOTIFIER_DRIVER=fcm
FCM_SERVICE_ACCOUNT_PATH=/etc/citra/firebase-service-account.json
FCM_TOPIC=clinic
EOF

docker compose up -d --build      # builds, migrates (AUTO_MIGRATE), starts
docker compose exec api /usr/local/bin/citra-petcare seed   # first boot only
curl -s http://127.0.0.1:8080/readyz
```

(For FCM, uncomment the service-account volume mount in `docker-compose.yml`.)

### 3. DNS + TLS + proxy

The production host fronts all containers with
[`nginxproxy/nginx-proxy`](https://github.com/nginx-proxy/nginx-proxy) on
ports 80/443, so the API registers itself via the `VIRTUAL_HOST` /
`VIRTUAL_PORT` / `CERT_NAME` environment variables already set in
`docker-compose.yml`, and joins the external `proxy-network`. TLS uses the
shared Cloudflare Origin wildcard cert for `*.holo.my.id` in
`nginx-proxy`'s certs directory; Cloudflare proxies the domain. Per-vhost
nginx overrides (body size, forwarded headers, and a
`location = /metrics { return 404; }` block) live in
`vhost.d/be-petcare.holo.my.id` of the nginx-proxy stack.

To deploy on a host **without** nginx-proxy instead: remove `VIRTUAL_HOST`,
`CERT_NAME` and the `proxy-network` entries from `docker-compose.yml`, then
use the standalone vhost:

1. Cloudflare DNS: `A be-petcare.holo.my.id → <VPS IP>`, proxied; SSL mode
   **Full (strict)**.
2. `sudo cp deploy/nginx/be-petcare.holo.my.id.conf /etc/nginx/sites-available/`
   and symlink into `sites-enabled/`.
3. `sudo certbot --nginx -d be-petcare.holo.my.id`
4. `sudo nginx -t && sudo systemctl reload nginx`

That vhost restores real client IPs from `CF-Connecting-IP` (feeding the
API's per-IP rate limiter), blocks `/metrics`, and proxies everything else
to `127.0.0.1:8080`.

### 4. Nightly backups

`deploy/backup/pg-backup.sh` dumps Postgres and uploads to the same OCI
bucket via the S3 API, pruning dumps older than 30 days:

```bash
sudo tee /etc/citra-petcare-backup.env <<'EOF'
COMPOSE_DIR=/opt/citra-petcare
BACKUP_S3_BUCKET=petcare-files
BACKUP_S3_PREFIX=backups
BACKUP_S3_ENDPOINT=https://<namespace>.compat.objectstorage.<region>.oraclecloud.com
BACKUP_AWS_PROFILE=oci-backup
EOF
aws configure --profile oci-backup        # OCI customer secret key id/secret
sudo crontab -e                           # 15 3 * * * /opt/citra-petcare/deploy/backup/pg-backup.sh >> /var/log/petcare-backup.log 2>&1
```

Restore: `gunzip -c petcare-….sql.gz | docker compose exec -T db psql -U petcare petcare`.

### 5. Updating

```bash
cd /opt/citra-petcare && git pull
docker compose up -d --build              # AUTO_MIGRATE applies new migrations
```

---

## Design notes for the Rust-curious

* **`Arc<dyn Storage>` / `Arc<dyn Notifier>`** in `AppState`: trait objects
  (with `async_trait`, since native async-fn-in-trait isn't object-safe yet)
  let dev, prod and tests swap drivers without generics spreading through
  every handler signature.
* **`AuthUser` extractor** (`domain/auth/extractor.rs`): adding it as a
  handler parameter *is* the auth guard — the JWT is verified before the
  handler body runs, and failures share the RFC 7807 pipeline.
* **Error conversion chain**: repo/service code just uses `?`. `sqlx::Error`
  converts into `AppError` (classifying unique/FK/check violations into
  409/422), domain enums convert via `#[from]`, and the single
  `IntoResponse` impl renders problem+json.
* **Current stock is never stored** — it's `SUM(±qty)` over the movement
  ledger, answered from a covering partial index. Writing an OUT movement
  row-locks the item so two concurrent OUTs can't oversell.
* **argon2 & blocking**: password hashing runs in `spawn_blocking`; a dummy
  verification runs for unknown emails so login timing doesn't leak which
  accounts exist.
* **Rate limiting** applies to `/api/v1` only, keyed on the real client IP
  (`X-Forwarded-For` from nginx, socket address otherwise).
