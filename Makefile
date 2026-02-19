.PHONY: build test clean install \
       e2e e2e-build e2e-down \
       e2e-claude e2e-claude-tools e2e-opencode e2e-opencode-tools

# ── Local ──────────────────────────────────────────────────────────

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

clean:
	cargo clean

install: release
	cp target/release/pulse ~/.local/bin/pulse

# ── E2E (run individual suites) ───────────────────────────────────

DC = docker compose -f e2e/docker-compose.yml

e2e-build:
	$(DC) build

e2e-down:
	$(DC) down --remove-orphans

e2e-claude:
	$(DC) up --build --abort-on-container-exit e2e

e2e-claude-tools:
	$(DC) up --build --abort-on-container-exit e2e-cc-tools

e2e-opencode:
	$(DC) up --build --abort-on-container-exit e2e-opencode

e2e-opencode-tools:
	$(DC) up --build --abort-on-container-exit e2e-oc-tools

# ── E2E (run all suites sequentially) ─────────────────────────────

e2e: e2e-claude e2e-claude-tools e2e-opencode e2e-opencode-tools
