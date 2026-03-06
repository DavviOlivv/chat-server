# Makefile para facilitar tarefas comuns do repositório
CARGO := cargo
BINARY := chat_server
IMAGE := chat_server:latest

.PHONY: all build release run client-run test fmt clippy docker-build docker-run docker-build-debian docker-run-debian docker-compose-up docker-compose-debian-up clean help

all: build

build:
	$(CARGO) build

release:
	$(CARGO) build --release

run:
	$(CARGO) run --bin $(BINARY)

client-run:
	$(CARGO) run --bin client

# Run server with metrics enabled on default metrics port
.PHONY: run-metrics
run-metrics:
	METRICS_BIND_ADDR=${METRICS_BIND_ADDR:-0.0.0.0:9090} \
		$(CARGO) run --bin $(BINARY)

test:
	$(CARGO) test

# Executa testes ignorados (stress tests)
.PHONY: stress-test
stress-test:
	$(CARGO) test -- --ignored --nocapture

fmt:
	$(CARGO) fmt --all

clippy:
	$(CARGO) clippy --all-targets -- -D warnings

# Docker helpers
docker-build:
	docker build -t $(IMAGE) .

docker-run:
	docker run --rm -p 8080:8080 \
		-e RUST_LOG=${RUST_LOG:-info} \
		-e LOG_JSON=${LOG_JSON:-1} \
		$(IMAGE)

# Debian variant
docker-build-debian:
	docker build -f Dockerfile.debian -t $(IMAGE)-debian .

docker-run-debian:
	docker run --rm -p 8080:8080 \
		-e RUST_LOG=${RUST_LOG:-info} \
		-e LOG_JSON=${LOG_JSON:-1} \
		$(IMAGE)-debian

# Fast local build: reuse local release binary to build image quickly
docker-build-release:
	cargo build --release
	docker build -f Dockerfile.local -t $(IMAGE)-local .

docker-run-local:
	docker run --rm -p 8080:8080 \
		-e RUST_LOG=${RUST_LOG:-info} \
		-e LOG_JSON=${LOG_JSON:-1} \
		$(IMAGE)-local

# Docker run exposing metrics port 9090 as well
.PHONY: docker-run-with-metrics
docker-run-with-metrics:
	docker run --rm -p 8080:8080 -p 9090:9090 \
		-e RUST_LOG=${RUST_LOG:-info} \
		-e LOG_JSON=${LOG_JSON:-1} \
		-e METRICS_BIND_ADDR=0.0.0.0:9090 \
		$(IMAGE)

docker-compose-up:
	docker compose up --build -d

docker-compose-debian-up:
	docker compose -f docker-compose.debian.yml up --build -d

clean:
	$(CARGO) clean
	rm -f target/release/$(BINARY)
	docker image rm -f $(IMAGE) || true

help:
	@echo "Targets:" \
	&& echo "  build         - cargo build" \
	&& echo "  release       - cargo build --release" \
	&& echo "  run           - cargo run --bin $(BINARY)" \
	&& echo "  client-run    - cargo run --bin client" \
	&& echo "  test          - cargo test" \
	&& echo "  fmt           - cargo fmt --all" \
	&& echo "  clippy        - cargo clippy --all-targets -- -D warnings" \
	&& echo "  docker-build  - docker build -t $(IMAGE) ." \
	&& echo "  docker-build-debian - docker build -f Dockerfile.debian -t $(IMAGE)-debian ." \
	&& echo "  docker-run    - docker run --rm -p 8080:8080 $(IMAGE) (respects RUST_LOG/LOG_JSON env)" \
	&& echo "  docker-run-debian - docker run --rm -p 8080:8080 $(IMAGE)-debian (respects RUST_LOG/LOG_JSON env)" \
	&& echo "  docker-build-release - build local release and docker build -f Dockerfile.local -t $(IMAGE)-local ." \
	&& echo "  docker-run-local - run the local-built image (respects RUST_LOG/LOG_JSON env)" \
	&& echo "  docker-compose-up - docker compose up --build -d" \
	&& echo "  docker-compose-debian-up - docker compose -f docker-compose.debian.yml up --build -d" \
	&& echo "  clean         - cargo clean + remove image" \
	&& echo "  help          - this message"

# stress-test: run ignored stress tests with output
# Use `make stress-test` to run ignored stress tests (prints output).

# Copy host CA certs into repo (local-only convenience)
.PHONY: embed-certs remove-certs
embed-certs:
	@mkdir -p certs
	@./scripts/prepare-certs.sh || true

remove-certs:
	@rm -f certs/ca-certificates.crt || true

# Build image that embeds certs (runs embed-certs automatically)
.PHONY: docker-build-with-certs docker-run-with-certs
docker-build-with-certs:
	make embed-certs
	docker build -f Dockerfile.with-certs -t $(IMAGE)-local-cert .

docker-run-with-certs:
	docker run --rm -p 8080:8080 \
		-e RUST_LOG=${RUST_LOG:-info} \
		-e LOG_JSON=${LOG_JSON:-1} \
		$(IMAGE)-local-cert
