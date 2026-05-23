set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]
set shell := ["sh", "-cu"]

default:
    @just --list

check:
    cargo check --workspace --all-targets

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace --all-targets

preflight: fmt-check clippy test

build:
    cargo build --release --locked

skill-zip:
    python -c "import shutil; shutil.make_archive('mcpcall-skill', 'zip', 'skills', 'mcpcall')"

run *ARGS:
    cargo run -- {{ARGS}}
