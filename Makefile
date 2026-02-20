CLIENT_DIR := apps/reasondb-client

.PHONY: docker-up docker-up-d docker-down docker-down-v docker-build docker-logs docker-ps docker-restart docker-watch \
        client-dev client-build client-tauri client-tauri-build client-install

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
	cd $(CLIENT_DIR) && npm install

client-dev:
	cd $(CLIENT_DIR) && npm run dev

client-build:
	cd $(CLIENT_DIR) && npm run build

client-tauri:
	cd $(CLIENT_DIR) && npm run tauri dev

client-tauri-build:
	cd $(CLIENT_DIR) && npm run tauri build
