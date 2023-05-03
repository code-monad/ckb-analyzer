#!/usr/bin/env bash
echo "Init db!"
POSTGRES_DB=ckb psql -U postgres  -f /docker-entrypoint-initdb.d/schema.sql
