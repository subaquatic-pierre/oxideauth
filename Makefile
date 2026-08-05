.PHONY: run watch clean

run:
	cargo run

build:
	cargo build --release
	cp ./target/release/oxideauth .
	chmod +x ./oxideauth

dev:
	cargo watch -x "run --bin oxideauth" 

test:
	cargo test --lib

# All tests include integration tests under test/ directory
test-all:
	cargo test -- --test-threads=1

clean:
	cargo clean

