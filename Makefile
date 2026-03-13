CLIENT_DIR := apps/reasondb-client

.PHONY: docker-up docker-up-d docker-down docker-down-v docker-build docker-logs docker-ps docker-restart docker-watch \
        client-dev client-build client-tauri client-tauri-build client-install \
        seed-docs test-queries seed-and-test \
        setup-hooks lint fmt lint-api lint-docs

docker-up:
	docker compose up --build

docker-up-d:
	docker compose up --build -d

docker-down:
	docker compose down

docker-down-v:
	docker compose down -v

docker-build:
	docker compose build --no-cache

docker-logs:
	docker compose logs -f

docker-ps:
	docker compose ps

docker-restart:
	docker compose down && docker compose up --build -d

docker-watch:
	docker compose up --build --watch

# ── ReasonDB Client ──────────────────────────────────────

client-install:
	cd $(CLIENT_DIR) && yarn install

client-dev:
	cd $(CLIENT_DIR) && yarn dev

client-build:
	cd $(CLIENT_DIR) && yarn build

client-app:
	cd $(CLIENT_DIR) && yarn tauri dev

client-app-build:
	cd $(CLIENT_DIR) && yarn tauri build

# ── Dev Setup ────────────────────────────────────────────

setup-hooks:
	git config core.hooksPath .githooks
	@echo "✓ Git hooks installed (.githooks/pre-commit)"

lint:
	RUSTC_WRAPPER="" cargo clippy --workspace --all-targets -- -D warnings

fmt:
	RUSTC_WRAPPER="" cargo fmt --all

lint-api:
	cd docs && npm run lint:api

lint-docs:
	cd docs && npm run check && npm run lint:api

# ── Seed & Test ──────────────────────────────────────────

seed-docs:
	@bash scripts/seed-docs.sh

test-queries:
	@bash scripts/test-queries.sh

seed-and-test: seed-docs test-queries
