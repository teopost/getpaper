# GetPaper - common tasks. Run `make` or `make help` for the list.
#
# The stack is defined in docker-compose.yml and configured by .env (see .env.example).

COMPOSE    ?= docker compose
SERVICE    ?= app
DB_SERVICE ?= postgres
DB_USER    ?= codex
DB_NAME    ?= codex
BACKUP_DIR ?= /opt/getpaper-backups
# keep backups for this many days (make backup-clean)
BACKUP_DAYS ?= 30

.DEFAULT_GOAL := help
.PHONY: help start stop restart rebuild down status logs logs-db shell psql backup backup-clean restore update push publish

help: ## Show this list
	@awk 'BEGIN {FS = ":.*## "} /^[a-z-]+:.*## / {printf "  \033[1m%-13s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# ---------------------------------------------------------------------------
# running
# ---------------------------------------------------------------------------

start: ## Start GetPaper and PostgreSQL in the background
	$(COMPOSE) up -d

stop: ## Stop the containers (data is kept)
	$(COMPOSE) stop

restart: ## Restart the application container
	$(COMPOSE) restart $(SERVICE)

rebuild: ## Rebuild the image from the sources and restart with it
	$(COMPOSE) up -d --build
	@$(MAKE) --no-print-directory status

down: ## Remove the containers (volumes and data are kept)
	$(COMPOSE) down

status: ## Show containers, health and published ports
	$(COMPOSE) ps

logs: ## Follow the application log (Ctrl+C to exit)
	$(COMPOSE) logs -f --tail=200 $(SERVICE)

logs-db: ## Follow the PostgreSQL log
	$(COMPOSE) logs -f --tail=200 $(DB_SERVICE)

shell: ## Open a shell in the application container
	$(COMPOSE) exec $(SERVICE) sh

psql: ## Open psql on the database
	$(COMPOSE) exec $(DB_SERVICE) psql -U $(DB_USER) -d $(DB_NAME)

# ---------------------------------------------------------------------------
# data
# ---------------------------------------------------------------------------

backup: ## Dump the database to /opt/getpaper-backups (compressed, root-only)
	@mkdir -p $(BACKUP_DIR) && chmod 700 $(BACKUP_DIR)
	@f=$(BACKUP_DIR)/getpaper-$$(date +%Y%m%d-%H%M%S).sql.gz; \
	$(COMPOSE) exec -T $(DB_SERVICE) pg_dump -U $(DB_USER) -d $(DB_NAME) | gzip > $$f && chmod 600 $$f && \
	echo "Database saved to $$f ($$(du -h $$f | cut -f1))"
	@echo "Attachments are in the Docker volume getpaper_uploads (back it up with your volume backups)."

backup-clean: ## Delete database backups older than 30 days
	find $(BACKUP_DIR) -name 'getpaper-*.sql.gz' -mtime +$(BACKUP_DAYS) -print -delete

restore: ## Restore a database dump: make restore FILE=/opt/getpaper-backups/getpaper-….sql.gz
	@test -n "$(FILE)" || { echo "Usage: make restore FILE=path/to/dump.sql.gz"; exit 1; }
	@test -f "$(FILE)" || { echo "File not found: $(FILE)"; exit 1; }
	@printf "This replaces ALL current data with %s. Type 'restore' to continue: " "$(FILE)"; read a; test "$$a" = restore
	$(COMPOSE) stop $(SERVICE)
	$(COMPOSE) exec -T $(DB_SERVICE) psql -q -U $(DB_USER) -d postgres -c 'DROP DATABASE IF EXISTS $(DB_NAME) WITH (FORCE)' -c 'CREATE DATABASE $(DB_NAME)'
	gunzip -c "$(FILE)" | $(COMPOSE) exec -T $(DB_SERVICE) psql -q -U $(DB_USER) -d $(DB_NAME)
	$(COMPOSE) start $(SERVICE)

# ---------------------------------------------------------------------------
# code
# ---------------------------------------------------------------------------

update: ## Pull the latest code and rebuild
	git pull --ff-only
	@$(MAKE) --no-print-directory rebuild

push: ## Push main to the origin remote
	git push origin main

# Publishes main to the "public" remote with author/committer emails rewritten: the emails listed in
# `git config getpaper.publish.rewrite` become `git config getpaper.publish.email` (local settings,
# never committed). The rewrite is deterministic, so published commits keep their hashes.
publish: ## Push main to the public remote with rewritten author emails
	@from="$$(git config getpaper.publish.rewrite)"; to="$$(git config getpaper.publish.email)"; \
	url="$$(git remote get-url public)"; \
	test -n "$$to" || { echo "Set: git config getpaper.publish.email you@example.com"; exit 1; }; \
	tmp=$$(mktemp -d); trap 'rm -rf "$$tmp"' EXIT; \
	git clone --quiet --no-local --single-branch --branch main . "$$tmp/repo" && cd "$$tmp/repo" && \
	FROM="$$from" TO="$$to" FILTER_BRANCH_SQUELCH_WARNING=1 git filter-branch --env-filter ' \
	  for e in $$FROM; do \
	    [ "$$GIT_AUTHOR_EMAIL" = "$$e" ] && GIT_AUTHOR_EMAIL=$$TO; \
	    [ "$$GIT_COMMITTER_EMAIL" = "$$e" ] && GIT_COMMITTER_EMAIL=$$TO; \
	  done; export GIT_AUTHOR_EMAIL GIT_COMMITTER_EMAIL' -- main > /dev/null && \
	for e in $$from; do \
	  if git log --format='%ae%n%ce' main | grep -qxF "$$e"; then echo "$$e still present, stopping"; exit 1; fi; \
	done && \
	git log --format='  %h %ae %s' main | head -5 && \
	git push "$$url" main:main
