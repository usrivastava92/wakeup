# wakeup - build and install the binary.
#
#   make               # build the release binary
#   make install       # build + copy wakeup to $(PREFIX)/bin
#   make uninstall
#
# Override the install location with PREFIX, e.g.:
#   make install PREFIX=/usr/local        (may need sudo)
#   make install PREFIX=/opt/homebrew

PREFIX ?= $(HOME)/.local
BINDIR := $(PREFIX)/bin
BINS := wakeup

.PHONY: all build install uninstall clean

all: build

build:
	cargo build --release

install: build
	@mkdir -p "$(BINDIR)"
	@for b in $(BINS); do \
		install -m 0755 "target/release/$$b" "$(BINDIR)/$$b" && \
		echo "installed: $(BINDIR)/$$b"; \
	done
	@case ":$$PATH:" in *":$(BINDIR):"*) ;; \
		*) echo "note: add $(BINDIR) to your PATH";; esac

uninstall:
	@for b in $(BINS); do rm -f "$(BINDIR)/$$b" && echo "removed: $(BINDIR)/$$b"; done

clean:
	cargo clean
