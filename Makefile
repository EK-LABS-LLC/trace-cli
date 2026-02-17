.PHONY: build test clean e2e-build e2e-run e2e-down

# ── Local ──────────────────────────────────────────────────────────

build:
	cargo build

test:
	cargo test

clean:
	cargo clean

# ── E2E ────────────────────────────────────────────────────────────

e2e-build:
	docker compose -f e2e/docker-compose.yml build

e2e-run:
	docker compose -f e2e/docker-compose.yml up --build --abort-on-container-exit

e2e-down:
	docker compose -f e2e/docker-compose.yml down --remove-orphans
