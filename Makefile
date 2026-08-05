.PHONY: run watch clean

DOCKER_COMPOSE_FILE := docker-compose.yml

run:
	cargo run

build:
	cargo build --release
	cp ./target/release/oxideauth-server .
	chmod +x ./oxideauth-server

dev:
	cargo watch -x "run --bin oxideauth" 

# .PHONY: start-db
# start-db:
#     @docker-compose -f $(DOCKER_COMPOSE_FILE) up -d

# stop-db:
#     @docker-compose -f $(DOCKER_COMPOSE_FILE) down

# test: start-db
#     @cargo test
#     @make stop-db

test:
	cargo test --lib

# All tests include integration tests under test/ directory
test-all:
	cargo test -- --test-threads=1

clean:
	cargo clean

rust-docs:
	cargo doc --no-deps

serve-rust-docs:
	python -m http.server --directory ./target/doc 3001
