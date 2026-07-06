# Citra PetCare — developer workflow.
# Copy .env.example to .env first; these targets read it automatically.

SHELL := /bin/bash
-include .env
export

.PHONY: help dev fmt fmt-check lint test migrate seed prepare prepare-check build docker-build up down logs deny audit

help: ## list targets
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

dev: ## run the API locally (pretty logs, .env config)
	cargo run -- serve

fmt: ## format the code
	cargo fmt

fmt-check: ## verify formatting (CI mode)
	cargo fmt --check

lint: ## clippy with warnings denied (matches CI)
	cargo clippy --all-targets -- -D warnings

test: ## unit + integration tests (integration tests need Docker)
	cargo test

migrate: ## apply pending migrations to $$DATABASE_URL
	cargo run -- migrate

seed: ## apply migrations + insert demo data (idempotent)
	cargo run -- seed

prepare: ## regenerate .sqlx offline cache (run after changing any query!)
	cargo sqlx prepare
	@echo "→ commit the updated .sqlx/ directory"

prepare-check: ## verify .sqlx is in sync with the queries (CI mode)
	cargo sqlx prepare --check

build: ## optimized release binary
	cargo build --release

docker-build: ## build the production image
	docker build -t citra-petcare:latest .

up: ## build + start api & postgres via docker compose
	docker compose up -d --build

down: ## stop the compose stack
	docker compose down

logs: ## follow api logs
	docker compose logs -f api

deny: ## license & advisory audit (cargo-deny)
	cargo deny check
