# ShellMounter Makefile
# ============================================================================
# Targets principales:
#   make              → build debug (binario rápido para desarrollo)
#   make release      → build release optimizado
#   make check        → cargo check (solo verifica, no compila)
#   make test         → corre todos los tests
#   make lint         → cargo clippy
#   make fmt          → formatea el código
#   make clean        → limpia target/
#
# Bundles (empaquetado para distribución):
#   make bundle-mac   → .dmg para macOS       (requiere macOS)
#   make bundle-linux → .tar.gz para Linux    (requiere Linux)
#   make bundle-win   → .exe installer Win    (requiere Windows + InnoSetup)
#
# Íconos:
#   make icons        → genera íconos para todas las plataformas
#
# Desarrollo:
#   make watch        → cargo watch (recompila al guardar)
#   make run          → cargo run (debug)
#   make run-release  → cargo run --release
# ============================================================================

.PHONY: all build release check test lint fmt clean \
        bundle-mac bundle-linux bundle-win bundle-all icons \
        watch run run-release help

# ── Default ─────────────────────────────────────────────────────────────────
all: build

# ── Build ───────────────────────────────────────────────────────────────────
build:
	cargo build --features gui

release:
	cargo build --release --features gui

check:
	cargo check --features gui

# ── Test & Lint ─────────────────────────────────────────────────────────────
test:
	cargo test

lint:
	cargo clippy --features gui -- -D warnings

fmt:
	cargo fmt --all -- --check

fmt-fix:
	cargo fmt --all

# ── Clean ───────────────────────────────────────────────────────────────────
clean:
	cargo clean

# ── Icons ───────────────────────────────────────────────────────────────────
icons:
	@bash script/generate-icons

# ── Bundles ─────────────────────────────────────────────────────────────────
bundle-mac:
	@bash script/bundle-mac

bundle-mac-debug:
	@bash script/bundle-mac -d -o

bundle-linux:
	@bash script/bundle-linux

bundle-linux-appimage:
	@bash script/bundle-linux --appimage

bundle-win:
	@powershell -ExecutionPolicy Bypass -File script/bundle-windows.ps1

# ── Bundle all (only on the right OS) ───────────────────────────────────────
bundle-all:
	@case "$$(uname -s)" in \
		Darwin)  $(MAKE) bundle-mac ;; \
		Linux)   $(MAKE) bundle-linux ;; \
		MINGW*|MSYS*|CYGWIN*) $(MAKE) bundle-win ;; \
		*) echo "Unknown OS. Use bundle-mac, bundle-linux, or bundle-win directly." ;; \
	esac

# ── Dev ─────────────────────────────────────────────────────────────────────
watch:
	cargo watch -x 'check --features gui'

run:
	cargo run --features gui

run-release:
	cargo run --release --features gui

# ── Help ────────────────────────────────────────────────────────────────────
help:
	@echo ""
	@echo "  \033[1;36mShellMounter Makefile\033[0m"
	@echo "  ==================="
	@echo ""
	@echo "  \033[1mBuild:\033[0m"
	@echo "    make                Build debug"
	@echo "    make release        Build release (LTO, stripped)"
	@echo "    make check          Cargo check (fast verify)"
	@echo ""
	@echo "  \033[1mTest & Lint:\033[0m"
	@echo "    make test           Run all tests"
	@echo "    make lint           Clippy with -D warnings"
	@echo "    make fmt            Check formatting"
	@echo "    make fmt-fix        Auto-fix formatting"
	@echo ""
	@echo "  \033[1mBundles (distribución):\033[0m"
	@echo "    make bundle-mac     macOS .dmg"
	@echo "    make bundle-linux   Linux .tar.gz"
	@echo "    make bundle-win     Windows .exe installer (PowerShell)"
	@echo "    make bundle-all     Auto-detect OS and bundle"
	@echo ""
	@echo "  \033[1mResources:\033[0m"
	@echo "    make icons          Generate platform icons"
	@echo ""
	@echo "  \033[1mDev:\033[0m"
	@echo "    make watch          Watch + check on save"
	@echo "    make run            Run debug build"
	@echo "    make run-release    Run release build"
	@echo ""
	@echo "  \033[1mClean:\033[0m"
	@echo "    make clean          Remove target/"
	@echo ""
	@echo "  \033[1;33mBundle prerequisites:\033[0m"
	@echo "    macOS:  Xcode CLI tools (xcode-select --install)"
	@echo "    Linux:  ldd, tar, imagemagick (optional for icons)"
	@echo "    Windows: Visual Studio 2022, Inno Setup 6, PowerShell 7+"
	@echo ""
