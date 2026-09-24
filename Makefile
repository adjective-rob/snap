.PHONY: build install start stop restart logs status clean dev deps deps-macos deps-linux setup-mcp hotkey doctor

OS := $(shell uname -s)
TAURI_BUILD_ARGS :=

ifeq ($(OS),Darwin)
TAURI_BUILD_ARGS := --bundles app
endif

# Install system dependencies
deps:
ifeq ($(OS),Darwin)
	$(MAKE) deps-macos
else
	$(MAKE) deps-linux
endif

# macOS: Xcode CLI tools provide everything; Rust via rustup
deps-macos:
	@command -v xcode-select >/dev/null 2>&1 || xcode-select --install
	@command -v rustup >/dev/null 2>&1 || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
	@command -v node >/dev/null 2>&1 || (command -v brew >/dev/null 2>&1 && brew install node) || echo "Install Node.js from https://nodejs.org"
	@echo "macOS dependencies ready."

# Linux (Ubuntu/Debian). gnome-screenshot is the Wayland capture tool and is
# no longer preinstalled on Ubuntu; scrot + xdotool cover X11.
deps-linux:
	sudo apt install -y gnome-screenshot scrot xdotool pkg-config libwebkit2gtk-4.1-dev \
		build-essential libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev

build:
	cd app && npm install && npx tauri build $(TAURI_BUILD_ARGS)
	cd mcp-server && uv venv .venv && uv pip install -e .

dev:
	cd app && npx tauri dev

install: build
ifeq ($(OS),Darwin)
	$(MAKE) install-macos
else
	$(MAKE) install-linux
endif
	@echo ""
	@echo "Snap installed. Register it with your AI tools:"
	@echo "  ./setup-mcp.sh"
	@echo "Check everything:"
	@echo "  ./snap-doctor.sh"

install-macos:
	mkdir -p ~/Library/LaunchAgents
	sed 's|%SNAP_PATH%|$(PWD)|g; s|%HOME_PATH%|$(HOME)|g' snap.plist > ~/Library/LaunchAgents/com.adjective.snap.plist
	launchctl unload ~/Library/LaunchAgents/com.adjective.snap.plist 2>/dev/null || true
	launchctl load -w ~/Library/LaunchAgents/com.adjective.snap.plist

# Wayland: no tray/global-shortcut support, so the desktop's own hotkey runs
# the overlay (install-hotkey.sh). X11: the tray app owns the hotkey and runs
# as a user service.
install-linux:
ifeq ($(XDG_SESSION_TYPE),wayland)
	./install-hotkey.sh
else
	$(MAKE) install-linux-service
endif

install-linux-service:
	mkdir -p ~/.config/systemd/user
	cp snap.service ~/.config/systemd/user/
	sed -i "s|%h/Desktop/snap|$(PWD)|g" ~/.config/systemd/user/snap.service
	systemctl --user daemon-reload
	systemctl --user enable snap.service
	systemctl --user start snap.service

start:
ifeq ($(OS),Darwin)
	launchctl load -w ~/Library/LaunchAgents/com.adjective.snap.plist
else
	systemctl --user start snap.service
endif

stop:
ifeq ($(OS),Darwin)
	launchctl unload ~/Library/LaunchAgents/com.adjective.snap.plist
else
	systemctl --user stop snap.service
endif

restart:
ifeq ($(OS),Darwin)
	launchctl unload ~/Library/LaunchAgents/com.adjective.snap.plist 2>/dev/null || true
	launchctl load -w ~/Library/LaunchAgents/com.adjective.snap.plist
else
	systemctl --user restart snap.service
endif

logs:
ifeq ($(OS),Darwin)
	tail -f ~/.snap/snap.log
else
	journalctl --user -u snap.service -f
endif

status:
ifeq ($(OS),Darwin)
	launchctl list | grep com.adjective.snap || echo "snap not running"
else
	systemctl --user status snap.service
endif

setup-mcp:
	./setup-mcp.sh

# Register (or re-register) the Wayland hotkey. SNAP_HOTKEY overrides the key.
hotkey:
	./install-hotkey.sh

# Check the install end to end.
doctor:
	./snap-doctor.sh

clean:
	rm -rf app/src-tauri/target app/node_modules
	rm -rf mcp-server/__pycache__ mcp-server/*.egg-info
