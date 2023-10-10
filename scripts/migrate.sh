#!/usr/bin/env bash
echo "making migrations $@"

atlas migrate diff $@ \
  --dir "file://migrations" \
  --to "file://schema.sql" \
  --dev-url "docker://postgres/15/dev?search_path=public"
