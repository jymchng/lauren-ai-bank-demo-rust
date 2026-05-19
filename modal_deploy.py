"""
Modal deployment for lauren-chatbot-rs.
"""

from __future__ import annotations

import os
import subprocess
import tarfile
import tempfile
from pathlib import Path

import modal

# ─────────────────────────────────────────────────────────────
# Paths
# ─────────────────────────────────────────────────────────────

HERE = Path(__file__).resolve().parent
AGTRS_DIR = HERE.parent / "agtrs"
INJECTABLE_DIR = HERE.parent / "injectable"

# ─────────────────────────────────────────────────────────────
# Create tarball with only git-tracked files
# ─────────────────────────────────────────────────────────────


def git_tracked_files(repo: Path) -> list[Path]:
    out = subprocess.check_output(
        ["git", "-C", str(repo), "ls-files", "-z"]
    )

    return [
        repo / p
        for p in out.decode().split("\0")
        if p
    ]


def create_repo_tarball(
    repos: list[tuple[Path, str]],
) -> Path:
    """
    Create a tar.gz containing only git-tracked files.

    repos:
        [
            (local_path, archive_prefix),
        ]
    """

    tmp = tempfile.NamedTemporaryFile(
        suffix=".tar.gz",
        delete=False,
    )

    tmp.close()

    with tarfile.open(tmp.name, "w:gz") as tar:
        for repo, prefix in repos:
            for file in git_tracked_files(repo):
                rel = file.relative_to(repo)
                arcname = f"{prefix}/{rel}"

                tar.add(file, arcname=arcname)

    return Path(tmp.name)


SOURCE_ARCHIVE = create_repo_tarball(
    [
        (HERE, "app"),
        (AGTRS_DIR, "agtrs"),
        (INJECTABLE_DIR, "injectable"),
    ]
)

# ─────────────────────────────────────────────────────────────
# Image
# ─────────────────────────────────────────────────────────────

image = (
    modal.Image.debian_slim()
    .apt_install(
        "curl",
        "build-essential",
        "pkg-config",
        "libssl-dev",
        "ca-certificates",
        "tar",
    )
    .run_commands(
        "curl https://sh.rustup.rs -sSf | sh -s -- -y "
        "--default-toolchain stable --profile minimal",
    )

    # Upload ONE archive
    .add_local_file(
        str(SOURCE_ARCHIVE),
        "/tmp/source.tar.gz",
        copy=True,
    )

    # Extract archive
    .run_commands(
        "mkdir -p /build",
        "tar -xzf /tmp/source.tar.gz -C /build",
    )

    # Build
    .run_commands(
        "export PATH=$HOME/.cargo/bin:$PATH"
        " && cd /build/app"
        " && cargo build --release",
        "cp /build/app/target/release/lauren-chatbot "
        "/usr/local/bin/lauren-chatbot",
        "chmod +x /usr/local/bin/lauren-chatbot",
    )
)

# ─────────────────────────────────────────────────────────────
# App
# ─────────────────────────────────────────────────────────────

app = modal.App("lauren-chatbot-rs")

PORT = 3001


@app.function(
    image=image,
    secrets=[modal.Secret.from_name("lauren-chatbot-secrets")],
    min_containers=1,
)
@modal.web_server(port=PORT, startup_timeout=30)
def server():
    import subprocess

    subprocess.Popen(
        ["/usr/local/bin/lauren-chatbot"],
        env={
            **os.environ,
            "PORT": str(PORT),
            "RUST_LOG": "lauren_chatbot=debug",
        },
    )