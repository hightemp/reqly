# Reqly developer commands. `make help` lists them.

SHELL := /bin/bash
.DEFAULT_GOAL := help

CARGO ?= cargo
PNPM ?= pnpm
DOCKER ?= docker
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
DATA_DIR ?= $(CURDIR)/.data
BIND ?= 127.0.0.1:8484
# Tests talk to local servers; a system proxy would intercept them.
TEST_ENV := NO_PROXY='*' no_proxy='*'
# Dev servers on localhost must bypass a system proxy (HTTP_PROXY etc.).
LOCAL_ENV := NO_PROXY="localhost,127.0.0.1,::1$${NO_PROXY:+,$$NO_PROXY}" no_proxy="localhost,127.0.0.1,::1$${no_proxy:+,$$no_proxy}"


.PHONY: help install dev dev-server dev-desktop build build-ui build-server build-desktop \
	run-server test test-rust test-ui e2e e2e-desktop coverage lint fmt check types \
	docker docker-up docker-down android clean release-notes release

help: ## Show this help
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z0-9_-]+:.*## / {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# ----- setup & development ----------------------------------------------------

install: ## Install JS dependencies and git hooks
	$(PNPM) install
	$(PNPM) -C ui exec playwright install chromium

dev: ## Run the server and the UI dev server (http://localhost:5173)
	@trap 'kill 0' EXIT; \
	$(LOCAL_ENV) REQLY_BIND=$(BIND) REQLY_DATA_DIR=$(DATA_DIR) $(CARGO) run -p reqly-server & \
	$(LOCAL_ENV) $(PNPM) -C ui dev

dev-server: ## Run only the server (BIND=127.0.0.1:8484)
	REQLY_BIND=$(BIND) REQLY_DATA_DIR=$(DATA_DIR) $(CARGO) run -p reqly-server

dev-desktop: ## Run the desktop app with hot reload
	$(LOCAL_ENV) $(PNPM) -C apps/desktop tauri dev

# ----- builds -------------------------------------------------------------------

build: build-ui build-server ## Build the UI and the release server binary

build-ui: ## Build the UI (ui/dist)
	$(PNPM) -C ui build

build-server: build-ui ## Build the server with the UI embedded (target/release/reqly-server)
	$(CARGO) build --release -p reqly-server --locked

build-desktop: ## Build desktop installers (apps/desktop)
	$(PNPM) -C apps/desktop tauri build

run-server: build-server ## Run the release server
	REQLY_BIND=$(BIND) REQLY_DATA_DIR=$(DATA_DIR) ./target/release/reqly-server

android: ## Build the Android debug APK
	$(PNPM) -C apps/desktop tauri android build --debug

# ----- quality ------------------------------------------------------------------

test: test-rust test-ui ## Run Rust and UI unit/integration tests

test-rust: ## Rust tests
	$(TEST_ENV) $(CARGO) test --workspace

test-ui: ## UI unit tests (Vitest)
	$(PNPM) -C ui test

e2e: ## Browser e2e tests against the server (Playwright)
	$(TEST_ENV) $(PNPM) -C ui e2e

e2e-desktop: ## Desktop e2e tests (needs tauri-driver, Linux)
	$(PNPM) -C ui build
	$(CARGO) build -p reqly-desktop --release --features tauri/custom-protocol
	$(PNPM) -C apps/desktop e2e

coverage: ## Rust (cargo-llvm-cov, >=85% lines) and UI coverage
	$(TEST_ENV) $(CARGO) llvm-cov --workspace --exclude reqly-desktop --exclude spike-http-engine --fail-under-lines 85
	$(PNPM) -C ui coverage

lint: ## rustfmt check, clippy, eslint, prettier and svelte-check
	$(CARGO) fmt --all --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(PNPM) -C ui lint

fmt: ## Format Rust and UI code
	$(CARGO) fmt --all
	$(PNPM) -C ui format

types: ## Regenerate TypeScript types from Rust (ui/src/lib/api/generated)
	$(CARGO) test -p reqly-api -p reqly-script --lib
	$(PNPM) -C ui exec prettier --write src/lib/api/generated

check: lint test e2e ## Everything CI runs (except desktop builds)

# ----- docker -------------------------------------------------------------------

docker: ## Build the server image (reqly-server:<version>)
	$(DOCKER) build -f docker/Dockerfile -t reqly-server:$(VERSION) -t reqly-server:dev .

docker-up: ## Start with docker compose (needs REQLY_AUTH_TOKEN)
	$(DOCKER) compose -f docker/compose.yaml up -d --build

docker-down: ## Stop docker compose
	$(DOCKER) compose -f docker/compose.yaml down

# ----- release --------------------------------------------------------------------

release-notes: ## Print release notes of V (default: current version) from CHANGELOG.md
	@scripts/release-notes.sh $(or $(V),$(VERSION))

release: ## Bump to V, commit and tag (push with: git push --follow-tags)
	@test -n "$(V)" || { echo "usage: make release V=x.y.z"; exit 1; }
	@git diff --quiet && git diff --cached --quiet || { echo "working tree is not clean"; exit 1; }
	@scripts/release-notes.sh $(V) > /dev/null
	sed -i 's/^version = ".*"/version = "$(V)"/' Cargo.toml
	sed -i 's/^  "version": ".*",/  "version": "$(V)",/' ui/package.json
	$(CARGO) update --workspace --quiet
	git add Cargo.toml Cargo.lock ui/package.json
	git commit -m "chore(release): v$(V)"
	git tag -a v$(V) -m "Reqly v$(V)"
	@echo "Tagged v$(V). Push with: git push --follow-tags"

clean: ## Remove build outputs
	$(CARGO) clean
	rm -rf ui/dist ui/coverage ui/test-results ui/playwright-report
