run:
    RUST_LOG=debug cargo run

test:
    RUST_LOG=debug cargo test --workspace

fmt:
    cargo fmt --all

dev: fmt test

deploy: dev
    uv run modal deploy modal_deploy.py