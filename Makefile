.PHONY: setup-upstream rebase-upstream

UPSTREAM_URL := git@github.com:chatmail/core.git

# Add the chatmail/core remote as `upstream` if missing, else update its URL.
setup-upstream:
	@if git remote get-url upstream >/dev/null 2>&1; then \
		git remote set-url upstream $(UPSTREAM_URL); \
	else \
		git remote add upstream $(UPSTREAM_URL); \
	fi
	git fetch --tags upstream


# Sync main with a chatmail/core upstream tag, keeping qxp commits on top.
# Specify the tag with TAG, e.g.: make rebase-upstream TAG=v2.49.0
TAG ?=
rebase-upstream:
ifeq ($(strip $(TAG)),)
	$(error TAG is required, e.g. make rebase-upstream TAG=v2.49.0)
endif
	git fetch --tags upstream
	git rebase $(TAG) main
	git push --force-with-lease origin main
