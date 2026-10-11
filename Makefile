.PHONY: policy check test build demo browser ci docker docker-smoke measure client-build client-demo client-browser client-docker client-docker-smoke prepare-native operating load release-evidence evaluate-runtime

prepare-native:
	python3 tools/prepare-v8.py

policy:
	python3 tools/check.py

check: policy prepare-native
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

test: prepare-native
	pnpm build:examples
	cargo nextest run --workspace --all-features --locked
	cargo test --workspace --doc --locked

build: prepare-native
	pnpm build:examples
	cargo build --workspace --locked

demo: build
	cargo run --locked -p gyral-axum-example

browser: build
	pnpm test:browser

ci:
	pnpm install --frozen-lockfile
	$(MAKE) check test build
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
	node tools/artifact-smoke.mjs
	node tools/client-artifact-smoke.mjs
	node tools/operating-smoke.mjs
	python3 tools/test-release-tools.py
	pnpm test:browser
	pnpm test:client-browser
	node tools/notes-browser-smoke.mjs
	python3 tools/check-notes-schema.py
	cargo build -p gyral-renderer-domain --no-default-features --locked
	cargo build -p gyral-example-notes-domain --no-default-features --locked
	cargo deny check advisories bans licenses sources
	cargo audit --ignore RUSTSEC-2024-0436 --deny warnings

docker:
	docker build --platform linux/amd64 --build-arg GYRAL_INPUT_SHA256=$$(python3 tools/build-inputs.py) --tag gyral-rs:example .

docker-smoke: docker
	node tools/container-smoke.mjs

measure: prepare-native
	pnpm build:example
	cargo run --release --locked -p gyral-renderer --example render-latency

evaluate-runtime: prepare-native
	pnpm build:example
	cargo nextest run --locked -p gyral-renderer --lib -E 'test(evaluation::) and not test(measure_runtime_configuration)'
	python3 tools/evaluate-runtime.py

operating: build
	node tools/operating-smoke.mjs

load: prepare-native
	pnpm build:example
	cargo build --release --locked -p gyral-axum-example
	node tools/load-smoke.mjs

release-evidence: docker-smoke
	python3 tools/release-evidence.py
	node tools/release-smoke.mjs

client-build:
	pnpm build:client-example
	cargo build --locked -p gyral-askama-client-example

client-demo: client-build
	cargo run --locked -p gyral-askama-client-example

client-browser: client-build
	node tools/client-artifact-smoke.mjs
	pnpm test:client-browser
	node tools/notes-browser-smoke.mjs

client-docker:
	docker build --platform linux/amd64 --file Dockerfile.client --tag gyral-rs:askama-client .

client-docker-smoke: client-docker
	node tools/client-container-smoke.mjs
