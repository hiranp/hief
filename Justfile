# HIEF Justfile
HOME := env_var("HOME")

build:
	cargo build --release

test:
	cargo test

lint:
	cargo clippy --all-targets --all-features -- -D warnings

fmt:
	cargo fmt -- --check

run:
	cargo run -- serve

extension-build:
	cd vscode-hief && npm install && npm run build

extension-package:
	cd vscode-hief && npm run package

doctor:
	cargo run -- doctor --fix

install-hooks:
	cargo run -- hooks install

# Build and symlink to ~/bin/hief for global access (git hooks/extension)
install:
	cargo build --release
	mkdir -p {{HOME}}/bin
	ln -sf {{invocation_directory()}}/target/release/hief {{HOME}}/bin/hief
	@echo "Hief installed to ~/bin/hief"

uninstall:
	rm -f {{HOME}}/bin/hief
	@echo "Hief removed from ~/bin/hief"

structural-search pattern lang="rust":
	cargo run -- index structural "{{pattern}}" --language {{lang}}
