"""
Modal deployment for lauren-chatbot-rs  (Axum / Rust web server).

Prerequisites
─────────────
Create a Modal secret named "lauren-chatbot-secrets" with these keys:
  OPENROUTER_API_KEY   sk-or-v1-...
  LLM_MODEL            openai/gpt-4o-mini   (or any OpenRouter model slug)
  LLM_BASE_URL         https://openrouter.ai/api/v1
  PAYLOAD_SECRET       <openssl rand -hex 32>
  PORT                 3001

Deploy
──────
  cd lauren-ai-chatbot-rs
  uv run modal token new            # one-time auth
  uv run modal deploy -m modal_deploy

The public HTTPS URL is printed after deploy.
"""

import os

import modal

# ── Paths (resolved on the deploying machine) ─────────────────────────────────

HERE = os.path.dirname(os.path.realpath(__file__))
AGTRS_DIR = os.path.normpath(os.path.join(HERE, "../agtrs"))
INJECTABLE_DIR = os.path.normpath(os.path.join(HERE, "../injectable"))

# ── Image — install Rust, copy source, compile ────────────────────────────────
#
# Directory layout inside the container mirrors the local workspace so that
# Cargo.toml path-dependencies resolve correctly:
#
#   /build/app/../agtrs       → /build/agtrs       ✓
#   /build/app/../injectable  → /build/injectable   ✓

image = (
    modal.Image.debian_slim()
    .apt_install(
        "curl",
        "build-essential",
        "pkg-config",
        "libssl-dev",
        "ca-certificates",
    )
    .run_commands(
        "curl https://sh.rustup.rs -sSf | sh -s -- -y"
        " --default-toolchain stable --profile minimal",
    )
    .add_local_dir(
        HERE,
        "/build/app",
        copy=True,
        ignore=["target", ".git"],
    )
    .add_local_dir(
        AGTRS_DIR,
        "/build/agtrs",
        copy=True,
        ignore=["target", ".git"],
    )
    .add_local_dir(
        INJECTABLE_DIR,
        "/build/injectable",
        copy=True,
        ignore=["target", ".git", "injectable-target"],
    )
    .run_commands(
        "export PATH=$HOME/.cargo/bin:$PATH"
        " && cd /build/app && cargo build --release",
        "cp /build/app/target/release/lauren-chatbot /usr/local/bin/lauren-chatbot",
        "chmod +x /usr/local/bin/lauren-chatbot",
    )
)

# ── App ───────────────────────────────────────────────────────────────────────

app = modal.App("lauren-chatbot-rs")

PORT = 3001


@app.function(
    image=image,
    secrets=[modal.Secret.from_name("lauren-chatbot-secrets")],
    min_containers=1,  # keep warm — avoids cold-start
)
@modal.web_server(port=PORT, startup_timeout=30)
def server():
    """Launch the Axum binary; Modal proxies HTTPS → PORT."""
    import subprocess

    subprocess.Popen(
        ["/usr/local/bin/lauren-chatbot"],
        env={**__import__("os").environ, "PORT": str(PORT)},
    )
